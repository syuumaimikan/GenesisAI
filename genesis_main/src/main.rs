// genesis_main/src/main.rs
use std::fs::OpenOptions;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

use genesis_core::{AgentRole, DistributedAgent, EmbeddingEngine, EventBus, GenesisError};
use genesis_memory::{InMemoryVectorStore, MemoryEntry, MemoryManager, VectorStore};
use genesis_npc_runtime::server::NpcServer;
use genesis_planning::{
    ExecutionState, Milestone, OllamaReasoner, PlanManager, ProjectPlan, Reasoner,
};
use genesis_plugin_sdk::Tool;
use genesis_runtime::OllamaEmbeddingEngine;
use genesis_self_improvement::SelfImprovementPipeline;
use genesis_tools::{
    FileReadTool, SelfWriterTool, SystemCommandTool, WebBrowserTool, WebSearchTool,
};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

// --- Mockフォールバック定義 ---
struct MockEmbeddingEngine;
#[async_trait::async_trait]
impl EmbeddingEngine for MockEmbeddingEngine {
    async fn generate_embedding(&self, _text: &str) -> Result<Vec<f32>, GenesisError> {
        Ok(vec![0.1; 1536])
    }
    fn dimension(&self) -> usize {
        1536
    }
}

struct DemoReasoner;
#[async_trait::async_trait]
impl Reasoner for DemoReasoner {
    async fn decompose_goal(&self, goal: &str) -> Result<ProjectPlan, anyhow::Error> {
        Ok(ProjectPlan {
            id: Uuid::new_v4(),
            goal: goal.to_string(),
            milestones: vec![Milestone {
                id: Uuid::new_v4(),
                title: "Mock: 環境セットアップ".to_string(),
                tasks: vec![],
                state: ExecutionState::InProgress,
            }],
            state: ExecutionState::InProgress,
        })
    }
    async fn reflect_and_replan(
        &self,
        current_plan: &ProjectPlan,
        _: Uuid,
        _: &str,
    ) -> Result<ProjectPlan, anyhow::Error> {
        Ok(current_plan.clone())
    }
}

// Ollama自動起動
async fn try_auto_start_ollama() {
    let client = reqwest::Client::new();
    let check = client.get("http://localhost:11434").send().await;

    if check.is_err() {
        tracing::warn!("Ollama サービスが起動していません。バックグラウンド自動起動を試みます...");
        let log_file_res = OpenOptions::new()
            .create(true)
            .append(true)
            .open("ollama_system.log");
        if let Ok(log_file) = log_file_res {
            let stderr_file = log_file.try_clone().unwrap_or_else(|_| {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("ollama_system.log")
                    .unwrap()
            });

            #[cfg(target_os = "windows")]
            let process = std::process::Command::new("ollama")
                .arg("serve")
                .stdout(std::process::Stdio::from(log_file))
                .stderr(std::process::Stdio::from(stderr_file))
                .creation_flags(0x08000000)
                .spawn();

            #[cfg(not(target_os = "windows"))]
            let process = std::process::Command::new("ollama")
                .arg("serve")
                .stdout(std::process::Stdio::from(log_file))
                .stderr(std::process::Stdio::from(stderr_file))
                .spawn();

            if process.is_ok() {
                tracing::info!("Ollama サービスを起動しました。ログは 'ollama_system.log' にリダイレクトされます。");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

async fn query_llm_with_rag(
    client: &reqwest::Client,
    endpoint: &str,
    model: &str,
    query: &str,
    context: &str,
) -> Result<String, anyhow::Error> {
    let prompt = format!("あなたは自律型AI 'GenesisAI' です。学習された知識を利用して、日本語で回答してください。\n\n[ナレッジ]\n{}\n\n[ユーザー]\n{}", context, query);
    let request_body = serde_json::json!({ "model": model, "prompt": prompt, "stream": false });
    let res = client
        .post(&format!("{}/api/generate", endpoint))
        .json(&request_body)
        .send()
        .await?;
    let parsed: serde_json::Value = res.json().await?;
    Ok(parsed["response"].as_str().unwrap_or("").to_string())
}

// ==========================================
// APIサーバー起動
// ==========================================
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    try_auto_start_ollama().await;

    let event_bus = EventBus::new(1024);
    let ollama_url = "http://localhost:11434";
    let use_real_llm = reqwest::Client::new()
        .get(ollama_url)
        .timeout(Duration::from_millis(1000))
        .send()
        .await
        .is_ok();

    let embedding_engine: Arc<dyn EmbeddingEngine> = if use_real_llm {
        Arc::new(OllamaEmbeddingEngine::new(
            ollama_url,
            "nomic-embed-text",
            768,
        ))
    } else {
        Arc::new(MockEmbeddingEngine)
    };

    let reasoner: Arc<dyn Reasoner> = if use_real_llm {
        Arc::new(OllamaReasoner::new(ollama_url, "llama3"))
    } else {
        Arc::new(DemoReasoner)
    };

    let vector_store = Arc::new(InMemoryVectorStore::new());
    let memory_manager = Arc::new(MemoryManager::new(
        vector_store.clone(),
        embedding_engine.clone(),
        event_bus.clone(),
    ));
    let plan_manager = Arc::new(PlanManager::new(reasoner.clone(), event_bus.clone()));

    // 【解決】マルチスレッドで共有するため、各ツールをArcでラップします
    let fs_tool = Arc::new(FileReadTool::new(Some(std::env::current_dir()?)));
    let _browser_tool = Arc::new(WebBrowserTool);
    let system_tool = Arc::new(SystemCommandTool);
    let writer_tool = Arc::new(SelfWriterTool::new(std::env::current_dir()?));
    let _improvement_pipeline = Arc::new(SelfImprovementPipeline::new(event_bus.clone()));

    // 分散マルチエージェント
    let researcher = DistributedAgent::new("Alice", AgentRole::Researcher, event_bus.clone());
    let analyst = DistributedAgent::new("Bob", AgentRole::Analyst, event_bus.clone());
    researcher.start_listening().await;
    analyst.start_listening().await;

    // NPC サーバー
    let npc_server = Arc::new(NpcServer::init(8081, "npcs.json").await?);

    // バックグラウンドループ
    let mm_handle = memory_manager.clone();
    tokio::spawn(async move {
        let _ = mm_handle.start_loop().await;
    });
    let pm_handle = plan_manager.clone();
    tokio::spawn(async move {
        let _ = pm_handle.start_monitoring_loop().await;
    });

    let api_listener = TcpListener::bind("127.0.0.1:8080").await?;
    tracing::info!("🚀 [GenesisAI Headless Backend] 起動完了 (Port: 8080)");

    let client = reqwest::Client::new();

    loop {
        if let Ok((mut socket, _)) = api_listener.accept().await {
            let store = vector_store.clone();
            let embed = embedding_engine.clone();
            let client_clone = client.clone();
            let use_llm = use_real_llm;
            let npc = npc_server.clone();

            // 【解決】Arcのクローン（低コストなポインタ増殖）を行い、安全にスレッドに渡します
            let sys_tool_clone = Arc::clone(&system_tool);
            let _writer_tool_clone = Arc::clone(&writer_tool);
            let _fs_tool_clone = Arc::clone(&fs_tool);

            tokio::spawn(async move {
                let mut buffer = [0; 4096];
                if let Ok(size) = socket.read(&mut buffer).await {
                    let request = String::from_utf8_lossy(&buffer[..size]);

                    if request.contains("POST /api/chat") {
                        if let Some(body_start) = request.find("\r\n\r\n") {
                            let json_str = &request[body_start + 4..];
                            if let Ok(json_val) =
                                serde_json::from_str::<serde_json::Value>(json_str)
                            {
                                let query = json_val["message"].as_str().unwrap_or("");

                                // 自律動作判定
                                let mut observed_context = String::new();
                                if query.contains("バージョン") || query.contains("ファイル")
                                {
                                    let tool_args = serde_json::json!({ "cmd": "cargo", "args": ["--version"] });
                                    if let Ok(res) = sys_tool_clone.execute(tool_args).await {
                                        observed_context = format!(
                                            "Observation:\n{}",
                                            res.observation["stdout"].as_str().unwrap_or("")
                                        );
                                    }
                                }

                                let mut reply = format!(
                                    "Ollamaがオフラインのため、Mock対応します。受け取った質問: {}",
                                    query
                                );
                                if let Ok(vector) = embed.generate_embedding(query).await {
                                    if let Ok(memories) = store.search(&vector, 1).await {
                                        let mut context = observed_context;
                                        for (entry, _) in memories {
                                            context.push_str(&entry.content);
                                        }

                                        if use_llm {
                                            if let Ok(ans) = query_llm_with_rag(
                                                &client_clone,
                                                "http://localhost:11434",
                                                "llama3",
                                                query,
                                                &context,
                                            )
                                            .await
                                            {
                                                reply = ans;
                                            }
                                        } else {
                                            if query == "おはよう" {
                                                reply = "おはようございます！自律フロントエンド接続に成功しました。".to_string();
                                            }
                                        }
                                    }
                                }

                                let res_body = serde_json::json!({ "response": reply }).to_string();
                                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\n\r\n{}", res_body.len(), res_body);
                                let _ = socket.write_all(response.as_bytes()).await;
                            }
                        }
                    } else if request.contains("POST /api/npc/talk") {
                        if let Some(body_start) = request.find("\r\n\r\n") {
                            let json_str = &request[body_start + 4..];
                            if let Ok(json_val) =
                                serde_json::from_str::<serde_json::Value>(json_str)
                            {
                                let npc_id = json_val["npc_id"].as_str().unwrap_or("Eldrin");
                                let msg = json_val["message"].as_str().unwrap_or("");

                                let mut reply = String::new();
                                if let Ok(ans) =
                                    npc.generate_npc_response(npc_id, msg, use_llm).await
                                {
                                    reply = ans;
                                }

                                let intervention_msg =
                                    match npc.check_and_trigger_intervention(npc_id).await {
                                        Ok(Some((file, char_name))) => format!(
                                            "🎁 {} から '{}' が贈られました！",
                                            char_name, file
                                        ),
                                        _ => String::new(),
                                    };

                                let res_body = serde_json::json!({ "response": reply, "intervention": intervention_msg }).to_string();
                                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\n\r\n{}", res_body.len(), res_body);
                                let _ = socket.write_all(response.as_bytes()).await;
                            }
                        }
                    } else {
                        let response = "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: POST, GET, OPTIONS\r\nAccess-Control-Allow-Headers: *\r\nContent-Length: 0\r\n\r\n";
                        let _ = socket.write_all(response.as_bytes()).await;
                    }
                }
            });
        }
    }
}
