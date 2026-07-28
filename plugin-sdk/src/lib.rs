use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, Serialize, Deserialize)]
pub enum ToolError {
    #[error("引数の形式が不正です: {0}")]
    InvalidArgument(String),

    #[error("実行エラーが発生しました: {0}")]
    ExecutionFailed(String),

    #[error("セキュリティ制限によるアクセス拒否: {0}")]
    PermissionDenied(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub observation: serde_json::Value,
    pub logs: Vec<String>,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn input_schema(&self) -> serde_json::Value;
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError>;
}
