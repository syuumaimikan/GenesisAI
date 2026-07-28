// tools/src/writer.rs
use async_trait::async_trait;
use genesis_plugin_sdk::{Tool, ToolError, ToolResult};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Stdio;

pub struct SelfWriterTool {
    workspace_dir: PathBuf,
}

impl SelfWriterTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace_dir: workspace,
        }
    }
}

#[derive(Deserialize)]
struct WriteArgs {
    file_path: String,
    proposed_code: String,
}

#[async_trait]
impl Tool for SelfWriterTool {
    fn name(&self) -> &'static str {
        "self_writer"
    }
    fn description(&self) -> &'static str {
        "物理ソースコード(.rs)を書き換え、ビルド監査（cargo check）が合格するか確認します。エラー時は即時に自己修復します。"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "書き換えるRustファイルの相対パス (例: core/src/bus.rs)" },
                "proposed_code": { "type": "string", "description": "変更後のRustソースコードの全文" }
            },
            "required": ["file_path", "proposed_code"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        let parsed: WriteArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgument(e.to_string()))?;

        let target_path = self.workspace_dir.join(&parsed.file_path);

        // セーフガード: ワークスペースの外のファイルを改ざんされるのを防ぐサンドボックスチェック
        if !target_path.starts_with(&self.workspace_dir) {
            return Err(ToolError::PermissionDenied(
                "セキュリティ制限: ワークスペース外部への書き込みはブロックされました。"
                    .to_string(),
            ));
        }

        if !target_path.exists() {
            return Err(ToolError::ExecutionFailed(
                "書き換え対象のソースファイルが存在しません。".to_string(),
            ));
        }

        // 1. 物理的な一時バックアップ (.rs.bak) の生成 (ロールバックの保証)
        let backup_path = target_path.with_extension("rs.bak");
        tokio::fs::copy(&target_path, &backup_path)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("バックアップ失敗: {}", e)))?;

        // 2. ソースファイルの物理的な書き換え
        tokio::fs::write(&target_path, &parsed.proposed_code)
            .await
            .map_err(|e| {
                ToolError::ExecutionFailed(format!("ファイル書き込みに失敗しました: {}", e))
            })?;

        // 3. 裏で「cargo check」を実行してビルドを物理検証
        println!("🔍 [コンパイラ監査ゲート] 実際に 'cargo check' を実行してコード整合性を検証しています...");
        let child = tokio::process::Command::new("cargo")
            .arg("check")
            .current_dir(&self.workspace_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                // コマンド起動自体が失敗した場合、即座に書き戻して撤退
                let _ = tokio::fs::copy(&backup_path, &target_path).await;
                let _ = tokio::fs::remove_file(&backup_path).await;
                return Err(ToolError::ExecutionFailed(format!(
                    "コンパイラの起動失敗により自動ロールバックされました: {}",
                    e
                )));
            }
        };

        let output = child.wait_with_output().await.map_err(|e| {
            ToolError::ExecutionFailed(format!("プロセスの待機に失敗しました: {}", e))
        })?;

        // 4. コンパイル結果の評価
        if output.status.success() {
            // A. 成功：バックアップを消去して完全適用
            let _ = tokio::fs::remove_file(&backup_path).await;
            Ok(ToolResult {
                success: true,
                observation: serde_json::json!({
                    "message": "ビルド整合性監査に合格。進化コードの適用を物理マージしました。",
                    "compiled": true
                }),
                logs: vec![format!(
                    "Patched and compiled successfully: {}",
                    parsed.file_path
                )],
            })
        } else {
            // B. 失敗：ミリ秒単位で即座にオリジナルの正常なコードにロールバック（自己修復）
            let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();
            println!("⚠️ [ビルド破壊エラーを検出しました] 自動生成されたコードにエラーが含まれています。");
            println!(
                "🔄 [自己復元中] 物理ソースコードの即時ロールバック（巻き戻し）を実行します..."
            );

            let restore_res = tokio::fs::copy(&backup_path, &target_path).await;
            let _ = tokio::fs::remove_file(&backup_path).await;

            match restore_res {
                Ok(_) => {
                    println!("✅ [自己修復完了] システムは正常に元の安定したコードベースに復元されました。破綻は回避されました。");
                    Ok(ToolResult {
                        success: false,
                        observation: serde_json::json!({
                            "message": "構文エラーが検出されたため、マージは自動拒否され復元されました。",
                            "compiled": false,
                            "compiler_stderr": stderr_str.trim()
                        }),
                        logs: vec![format!(
                            "Compile failed, rolled back changes on: {}",
                            parsed.file_path
                        )],
                    })
                }
                Err(e) => Err(ToolError::ExecutionFailed(format!(
                    "【警告】ロールバックの物理復元に失敗しました！: {}",
                    e
                ))),
            }
        }
    }
}
