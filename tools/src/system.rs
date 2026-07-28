// tools/src/system.rs
use async_trait::async_trait;
use genesis_plugin_sdk::{Tool, ToolError, ToolResult};
use serde::Deserialize;
use std::process::Stdio;

pub struct SystemCommandTool;

#[derive(Deserialize)]
struct CommandArgs {
    cmd: String,
    args: Vec<String>,
}

#[async_trait]
impl Tool for SystemCommandTool {
    fn name(&self) -> &'static str {
        "system_command"
    }
    fn description(&self) -> &'static str {
        "システムコマンドを実行します（破壊的なコマンドは実行不可能です）。"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "cmd": { "type": "string", "description": "コマンド名 (例: cargo, git, ping)" },
                "args": { "type": "array", "items": { "type": "string" }, "description": "コマンド引数のリスト" }
            },
            "required": ["cmd", "args"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        let parsed: CommandArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgument(e.to_string()))?;

        // 破壊的、またはシステムを停止させるコマンドのブラックリスト判定
        let blocked_commands = [
            "rm", "del", "format", "mkfs", "dd", "shred", "poweroff", "reboot",
        ];
        if blocked_commands.contains(&parsed.cmd.as_str()) {
            return Err(ToolError::PermissionDenied(
                "セキュリティ保護のため、システム破壊コマンドの実行は拒否されました。".to_string(),
            ));
        }

        // コマンド実行スロー
        let mut child = tokio::process::Command::new(&parsed.cmd)
            .args(&parsed.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ToolError::ExecutionFailed(format!("プロセス起動失敗: {}", e)))?;

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("プロセスの待機中にエラー: {}", e)))?;

        let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(ToolResult {
            success: output.status.success(),
            observation: serde_json::json!({
                "exit_code": output.status.code(),
                "stdout": stdout_str.trim(),
                "stderr": stderr_str.trim()
            }),
            logs: vec![format!(
                "コマンド実行完了: {} {:?}",
                parsed.cmd, parsed.args
            )],
        })
    }
}
