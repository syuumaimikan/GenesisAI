use async_trait::async_trait;
use genesis_plugin_sdk::{Tool, ToolError, ToolResult};
use serde::Deserialize;
use std::path::PathBuf;

pub struct FileReadTool {
    sandbox_dir: Option<PathBuf>,
}

impl FileReadTool {
    pub fn new(sandbox: Option<PathBuf>) -> Self {
        Self {
            sandbox_dir: sandbox,
        }
    }
}

#[derive(Deserialize)]
struct ReadArgs {
    path: String,
}

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &'static str {
        "file_read"
    }

    fn description(&self) -> &'static str {
        "ワークスペース内の指定された相対パスからファイルを読み込みます。"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "ワークスペース起点での相対パス。"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        let parsed: ReadArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgument(e.to_string()))?;

        let mut target_path = PathBuf::from(parsed.path);

        if let Some(ref sandbox) = self.sandbox_dir {
            let resolved = sandbox.join(&target_path);
            if !resolved.starts_with(sandbox) {
                return Err(ToolError::PermissionDenied(
                    "許可されたワークスペース外部へのアクセスは拒否されました。".to_string(),
                ));
            }
            target_path = resolved;
        }

        match tokio::fs::read_to_string(&target_path).await {
            Ok(content) => Ok(ToolResult {
                success: true,
                observation: serde_json::json!({ "content": content }),
                logs: vec![format!("ファイルを正常に展開しました: {:?}", target_path)],
            }),
            Err(e) => Err(ToolError::ExecutionFailed(format!(
                "ファイル読み込みに失敗しました: {}",
                e
            ))),
        }
    }
}
