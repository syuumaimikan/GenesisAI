// tools/src/browser.rs
use async_trait::async_trait;
use genesis_plugin_sdk::{Tool, ToolError, ToolResult};
use scraper::{Html, Selector};
use serde::Deserialize;

pub struct WebBrowserTool;

#[derive(Deserialize)]
struct BrowseArgs {
    query: String,
}

#[async_trait]
impl Tool for WebBrowserTool {
    fn name(&self) -> &'static str {
        "web_browser"
    }
    fn description(&self) -> &'static str {
        "Webを検索し、最適な日本語のページ本文を直接解析してインデックス化します。"
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
        let parsed: BrowseArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgument(e.to_string()))?;

        // 【改善】HTTPヘッダーに「日本語優先(ja)」を設定します
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT_LANGUAGE,
            "ja,en-US;q=0.9,en;q=0.8".parse().unwrap(),
        );

        let client = reqwest::Client::builder()
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko)",
            )
            .default_headers(headers)
            .build()
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        // 【改善】検索クエリの末尾に「&kl=jp-jp」（日本語地域制限）を追加します
        let search_url = format!(
            "https://html.duckduckgo.com/html/?q={}&kl=jp-jp",
            urlencoding::encode(&parsed.query)
        );

        let search_html = client
            .get(&search_url)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("検索リクエスト失敗: {}", e)))?
            .text()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("HTML取得失敗: {}", e)))?;

        // スコープ制限によって Non-Send な scraper 構造体の寿命を .await の前に終わらせる
        let (target_url, snippets) = {
            let document = Html::parse_document(&search_html);
            let result_selector = Selector::parse(".result__snippet").unwrap();
            let link_selector = Selector::parse("a.result__url").unwrap();

            let mut snippets = Vec::new();
            let mut target_url = String::new();

            for snippet_el in document.select(&result_selector) {
                snippets.push(snippet_el.text().collect::<Vec<_>>().join(" "));
            }

            for link_el in document.select(&link_selector) {
                if let Some(href) = link_el.value().attr("href") {
                    if href.contains("uddg=") {
                        if let Some(pos) = href.find("uddg=") {
                            let encoded_part = &href[pos + 5..];
                            let clean_encoded =
                                encoded_part.split('&').next().unwrap_or(encoded_part);
                            if let Ok(decoded) = urlencoding::decode(clean_encoded) {
                                target_url = decoded.to_string();
                                break;
                            }
                        }
                    } else if href.starts_with("http") {
                        target_url = href.to_string();
                        break;
                    }
                }
            }
            (target_url, snippets)
        };

        if target_url.is_empty() {
            return Err(ToolError::ExecutionFailed(
                "該当するターゲットURLの検出、およびデコードに失敗しました。".to_string(),
            ));
        }

        tracing::info!("ターゲットページを日本語解析中: {}", target_url);
        let page_html = client
            .get(&target_url)
            .send()
            .await
            .map_err(|e| {
                ToolError::ExecutionFailed(format!(
                    "ページ「{}」へのアクセスに失敗しました: {}",
                    target_url, e
                ))
            })?
            .text()
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        // 本文を抽出
        let page_doc = Html::parse_document(&page_html);
        let p_selector = Selector::parse("p").unwrap();

        let mut article_text = String::new();
        for p in page_doc.select(&p_selector).take(10) {
            let p_text = p.text().collect::<Vec<_>>().join(" ");
            if p_text.len() > 30 {
                article_text.push_str(&p_text);
                article_text.push('\n');
            }
        }

        if article_text.is_empty() {
            article_text = snippets.join("\n");
        }

        Ok(ToolResult {
            success: true,
            observation: serde_json::json!({
                "target_url": target_url,
                "extracted_content": article_text.chars().take(2000).collect::<String>()
            }),
            logs: vec![format!("Browse completed for URL: {}", target_url)],
        })
    }
}
