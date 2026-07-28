// cli_frontend/src/main.rs
use std::io::{self, Write};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=========================================================");
    println!("  GenesisAI 接続ターミナル (Decoupled CLI Frontend)");
    println!("  バックエンドAPI (127.0.0.1:8080) に接続しています...");
    println!("=========================================================\n");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;
    let stdin = io::stdin();
    let mut input = String::new();

    loop {
        print!("GenesisAI (CLI) > ");
        io::stdout().flush()?;
        input.clear();
        stdin.read_line(&mut input)?;

        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "exit" || trimmed == "quit" {
            break;
        }

        let payload = serde_json::json!({ "message": trimmed });

        // バックエンドの共通APIエンドポイントにアクセス
        match client
            .post("http://127.0.0.1:8080/api/chat")
            .json(&payload)
            .send()
            .await
        {
            Ok(res) => {
                if let Ok(json) = res.json::<serde_json::Value>().await {
                    let reply = json["response"].as_str().unwrap_or("APIエラー");
                    println!("\n🤖 GenesisAI:\n{}", reply);
                }
            }
            Err(_) => {
                println!("❌ バックエンドサーバーに接続できません。cargo run -p genesis_main を起動してください。");
            }
        }
        println!();
    }
    Ok(())
}
