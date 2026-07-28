// tools/src/web.rs
use async_trait::async_trait;
use genesis_plugin_sdk::{Tool, ToolError, ToolResult};
use serde::Deserialize;

pub struct WebSearchTool;

#[derive(Deserialize)]
struct SearchArgs {
    query: String,
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &'static str {
        "web_search"
    }
    fn description(&self) -> &'static str {
        "Webを検索して最新情報を取得します。"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "検索クエリ" }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        let parsed: SearchArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgument(e.to_string()))?;

        let client = reqwest::Client::new();
        let url = format!(
            "https://api.duckduckgo.com/?q={}&format=json&no_html=1",
            urlencoding::encode(&parsed.query)
        );

        match client.get(&url).send().await {
            Ok(response) => {
                if let Ok(json) = response.json::<serde_json::Value>().await {
                    let abstract_text = json["AbstractText"].as_str().unwrap_or("");
                    let results = if abstract_text.is_empty() {
                        serde_json::json!({ "results": "直接の回答が見つかりませんでした。より具体的なクエリを推奨します。" })
                    } else {
                        serde_json::json!({
                            "abstract": abstract_text,
                            "source": json["AbstractSource"].as_str().unwrap_or("DuckDuckGo")
                        })
                    };

                    Ok(ToolResult {
                        success: true,
                        observation: results,
                        logs: vec![format!("検索成功: {}", parsed.query)],
                    })
                } else {
                    Err(ToolError::ExecutionFailed(
                        "検索結果の解析に失敗しました。".to_string(),
                    ))
                }
            }
            Err(e) => Err(ToolError::ExecutionFailed(format!(
                "ネットワークエラー: {}",
                e
            ))),
        }
    }
}
