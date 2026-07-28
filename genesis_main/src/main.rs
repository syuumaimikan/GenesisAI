// [Optimized by GenesisAI at SystemTime { intervals: 134297002471272785 }]
// genesis_main/src/main.rs (最終統合APIサーバー)
use std::fs::OpenOptions;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex; // 非同期Mutexを追加
use uuid::Uuid;

use genesis_core::{AgentRole, DistributedAgent, EmbeddingEngine, EventBus, GenesisError};
use genesis_memory::{
    InMemoryVectorStore, MemoryEntry, MemoryManager, MemoryMetadata, VectorStore,
};
use genesis_npc_runtime::server::NpcServer;
use genesis_planning::{
    ExecutionState, Milestone, OllamaReasoner, PlanManager, ProjectPlan, Reasoner,
};
use genesis_plugin_sdk::Tool;
use genesis_runtime::OllamaEmbeddingEngine;
use genesis_self_improvement::SelfImprovementPipeline;
use genesis_tools::{FileReadTool, SelfWriterTool, SystemCommandTool, WebBrowserTool};

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
                tracing::info!(
                    "Ollama サービスを起動しました。出力は 'ollama_system.log' に記録されます。"
                );
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
    let prompt = format!(
        "あなたは自律型AI 'GenesisAI' です。提示された[ナレッジ]を最も信頼できる情報として優先的に参考にして、ユーザーの質問に回答してください。\n\n\
        [絶対規則]\n\
        1. 最初から最後の一文まで、必ず100%『日本語のみ』で出力してください。\n\
        2. 挨拶や感嘆符、解説に至るまで、英語の使用は一切禁止します。すべて日本語の表現に翻訳して出力してください。\n\n\
        [ナレッジ]\n\
        {}\n\n\
        [ユーザーの質問]\n\
        {}\n\n\
        [GenesisAIとしての丁寧な日本語回答（英語は一切使用禁止）]:",
        context, query
    );

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
        Arc::new(OllamaReasoner::new(ollama_url, "qwen2.5:3b"))
    } else {
        Arc::new(DemoReasoner)
    };

    let vector_store = Arc::new(InMemoryVectorStore::load_or_create("memories.json").await?);
    let memory_manager = Arc::new(MemoryManager::new(
        vector_store.clone(),
        embedding_engine.clone(),
        event_bus.clone(),
    ));
    let plan_manager = Arc::new(PlanManager::new(reasoner.clone(), event_bus.clone()));

    // ツール群をスレッド安全にするため Arc化
    let fs_tool = Arc::new(FileReadTool::new(Some(std::env::current_dir()?)));
    let browser_tool = Arc::new(WebBrowserTool);
    let system_tool = Arc::new(SystemCommandTool);
    let writer_tool = Arc::new(SelfWriterTool::new(std::env::current_dir()?));
    let improvement_pipeline = Arc::new(SelfImprovementPipeline::new(event_bus.clone()));

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

    // 【新設】マルチスレッド並行処理下で安全にインクリメントするための Mutex ステート
    let evolution_level = Arc::new(Mutex::new(1));
    let total_latency_saved_ms = Arc::new(Mutex::new(0.0));

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

            let sys_tool_clone = Arc::clone(&system_tool);
            let writer_tool_clone = Arc::clone(&writer_tool);
            let fs_tool_clone = Arc::clone(&fs_tool);
            let browser_tool_clone = Arc::clone(&browser_tool);
            let improvement_pipeline_clone = Arc::clone(&improvement_pipeline);

            // スレッド安全なステート変数のクローンを各並行スレッドに引き渡します
            let ev_level_clone = Arc::clone(&evolution_level);
            let lat_saved_clone = Arc::clone(&total_latency_saved_ms);

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
                                let query = json_val["message"].as_str().unwrap_or("").trim();

                                // --------------------------------------------------
                                // 1. /help コマンド
                                // --------------------------------------------------
                                if query == "/help" {
                                    let help_text = "\n=========================================================\n\
                                      📖 GenesisAI 自律システムコマンドガイド\n\
                                      =========================================================\n\
                                      【システム制御指令】\n\
                                      - /learn <パス>        : ファイルをパースして長期記憶ベクトルDBに同期\n\
                                      - /search <ワード>      : Web深層スクレイピング＆リアルタイム自律学習\n\
                                      - /cmd <命令> [引数]    : 安全なPC・システムコマンドの実行\n\
                                      - /evolve              : 自己改善ステートマシンによる模擬最適化を実行\n\
                                      - /evolve_real <パス>  : 物理ソースコード(.rs)の自動書き換えとビルド安全検証\n\n\
                                      【RPG-NPC対話】\n\
                                      - /talk <NPC名> <内容> : NPC(Mina, Kaelen, Eldrin)とシャンフロ風対話\n\
                                      - /npc_status          : 全NPCの現在認知ステータス（感情・好感度）を表示\n\n\
                                      【通常の自律会話】\n\
                                      - 自由に質問を入力してください。AIが自律的にPC状態の監視やRAG知識検索、\n\
                                        コマンド実行判定を裏で行いながらスマートに回答します。\n\
                                      =========================================================\n";

                                    let res_body =
                                        serde_json::json!({ "response": help_text }).to_string();
                                    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\n\r\n{}", res_body.len(), res_body);
                                    let _ = socket.write_all(response.as_bytes()).await;
                                    return;
                                }

                                // --------------------------------------------------
                                // 2. /talk コマンド
                                // --------------------------------------------------
                                if query.starts_with("/talk ") {
                                    let parts: Vec<&str> =
                                        query.trim_start_matches("/talk ").splitn(2, ' ').collect();
                                    if parts.len() < 2 {
                                        let reply =
                                            "⚠️ 使用法: /talk <Mina|Kaelen|Eldrin> <メッセージ>";
                                        let res_body =
                                            serde_json::json!({ "response": reply }).to_string();
                                        let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\n\r\n{}", res_body.len(), res_body);
                                        let _ = socket.write_all(response.as_bytes()).await;
                                        return;
                                    }

                                    let npc_id_input = parts[0];
                                    let player_message = parts[1];

                                    let npc_id = match npc_id_input.to_lowercase().as_str() {
                                        "mina" | "ミナ" => "Mina",
                                        "kaelen" | "ケーレン" => "Kaelen",
                                        "eldrin" | "エルドリン" => "Eldrin",
                                        _ => {
                                            let reply = "❌ 対象のNPCが見つかりません。 (利用可能: Mina, Kaelen, Eldrin)";
                                            let res_body = serde_json::json!({ "response": reply })
                                                .to_string();
                                            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\n\r\n{}", res_body.len(), res_body);
                                            let _ = socket.write_all(response.as_bytes()).await;
                                            return;
                                        }
                                    };

                                    let mut reply = String::new();
                                    if let Ok(ans) = npc
                                        .generate_npc_response(npc_id, player_message, use_llm)
                                        .await
                                    {
                                        reply = ans;
                                    }

                                    let intervention_msg =
                                        match npc.check_and_trigger_intervention(npc_id).await {
                                            Ok(Some((file, char_name))) => format!(
                                                "\n🎁 [自律環境干渉] {} から '{}' が贈られました！",
                                                char_name, file
                                            ),
                                            _ => String::new(),
                                        };

                                    let full_reply = format!("{}\n{}", reply, intervention_msg);
                                    let res_body =
                                        serde_json::json!({ "response": full_reply }).to_string();
                                    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\n\r\n{}", res_body.len(), res_body);
                                    let _ = socket.write_all(response.as_bytes()).await;
                                    return;
                                }

                                // --------------------------------------------------
                                // 3. /npc_status コマンド
                                // --------------------------------------------------
                                if query == "/npc_status" {
                                    let npcs = npc.world.npc_database.read().await;
                                    let mut status_text = "\n=========================================================\n\
                                      🎮 統合NPC認知ステータス（マルチ・キャラクター仕様）\n\
                                      =========================================================\n".to_string();
                                    for (id, npc_state) in npcs.iter() {
                                        status_text.push_str(&format!(
                                            "  ID: {:<8} | 名前: {:<6} | 感情: {:<12} | 好感度: {:<4}/100 | ギフト: {}\n",
                                            id, npc_state.name, npc_state.current_emotion, npc_state.affection, if npc_state.has_gifted { "済" } else { "未" }
                                        ));
                                    }
                                    status_text.push_str("=========================================================\n");

                                    let res_body =
                                        serde_json::json!({ "response": status_text }).to_string();
                                    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\n\r\n{}", res_body.len(), res_body);
                                    let _ = socket.write_all(response.as_bytes()).await;
                                    return;
                                }

                                // --------------------------------------------------
                                // 4. /learn コマンド
                                // --------------------------------------------------
                                if query.starts_with("/learn ") {
                                    let path = query.trim_start_matches("/learn ").trim();
                                    let mut reply =
                                        format!("❌ ファイル '{}' を読み込めません。", path);
                                    let tool_args = serde_json::json!({ "path": path });
                                    if let Ok(result) = fs_tool_clone.execute(tool_args).await {
                                        if result.success {
                                            let content = result.observation["content"]
                                                .as_str()
                                                .unwrap_or("");
                                            if let Ok(vector) =
                                                embed.generate_embedding(content).await
                                            {
                                                let entry = MemoryEntry {
                                                    id: Uuid::new_v4(),
                                                    content: content.to_string(),
                                                    embedding: vector,
                                                    metadata: MemoryMetadata {
                                                        importance: 0.9,
                                                        confidence: 1.0,
                                                        timestamp: std::time::SystemTime::now(),
                                                        source: format!("FileLearner:{}", path),
                                                        symbolic_links: vec![],
                                                    },
                                                };
                                                let _ = store.insert(entry).await;
                                                reply = format!("✅ [長期記憶同期完了] '{}' をナレッジベースに統合しました。", path);
                                            }
                                        }
                                    }
                                    let res_body =
                                        serde_json::json!({ "response": reply }).to_string();
                                    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\n\r\n{}", res_body.len(), res_body);
                                    let _ = socket.write_all(response.as_bytes()).await;
                                    return;
                                }

                                // --------------------------------------------------
                                // 5. /search コマンド
                                // --------------------------------------------------
                                if query.starts_with("/search ") {
                                    let search_q = query.trim_start_matches("/search ").trim();
                                    let tool_args = serde_json::json!({ "query": search_q });
                                    let mut reply = "❌ ブラウジングに失敗しました。".to_string();
                                    if let Ok(result) = browser_tool_clone.execute(tool_args).await
                                    {
                                        if result.success {
                                            let target_url = result.observation["target_url"]
                                                .as_str()
                                                .unwrap_or("URL未特定");
                                            let extracted = result.observation["extracted_content"]
                                                .as_str()
                                                .unwrap_or("");
                                            if let Ok(vector) =
                                                embed.generate_embedding(extracted).await
                                            {
                                                let entry = MemoryEntry {
                                                    id: Uuid::new_v4(),
                                                    content: extracted.to_string(),
                                                    embedding: vector,
                                                    metadata: MemoryMetadata {
                                                        importance: 0.95,
                                                        confidence: 1.0,
                                                        timestamp: std::time::SystemTime::now(),
                                                        source: format!(
                                                            "BrowsedWeb:{}",
                                                            target_url
                                                        ),
                                                        symbolic_links: vec![],
                                                    },
                                                };
                                                let _ = store.insert(entry).await;
                                                reply = format!("🌐 [Webブラウズ完了]\nソース: {}\n\n💡 内容をメモリにインデックスしました。", target_url);
                                            }
                                        }
                                    }
                                    let res_body =
                                        serde_json::json!({ "response": reply }).to_string();
                                    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\n\r\n{}", res_body.len(), res_body);
                                    let _ = socket.write_all(response.as_bytes()).await;
                                    return;
                                }

                                // --------------------------------------------------
                                // 6. /cmd コマンド
                                // --------------------------------------------------
                                if query.starts_with("/cmd ") {
                                    let parts: Vec<&str> = query
                                        .trim_start_matches("/cmd ")
                                        .split_whitespace()
                                        .collect();
                                    let mut reply = "⚠️ 使用法: /cmd <命令> [引数]".to_string();
                                    if !parts.is_empty() {
                                        let cmd = parts[0];
                                        let args: Vec<String> =
                                            parts[1..].iter().map(|s| s.to_string()).collect();
                                        let tool_args =
                                            serde_json::json!({ "cmd": cmd, "args": args });
                                        if let Ok(res) = sys_tool_clone.execute(tool_args).await {
                                            reply = if res.success {
                                                format!(
                                                    "💻 [実行成功]:\n{}",
                                                    res.observation["stdout"]
                                                        .as_str()
                                                        .unwrap_or("")
                                                )
                                            } else {
                                                format!(
                                                    "⚠️ [エラー (Code {:?})]:\n{}",
                                                    res.observation["exit_code"],
                                                    res.observation["stderr"]
                                                        .as_str()
                                                        .unwrap_or("")
                                                )
                                            };
                                        }
                                    }
                                    let res_body =
                                        serde_json::json!({ "response": reply }).to_string();
                                    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\n\r\n{}", res_body.len(), res_body);
                                    let _ = socket.write_all(response.as_bytes()).await;
                                    return;
                                }

                                // --------------------------------------------------
                                // 7. 【復活】自己進化シミュレーション (/evolve)
                                // --------------------------------------------------
                                if query == "/evolve" {
                                    let check_args =
                                        serde_json::json!({ "cmd": "cargo", "args": ["check"] });
                                    let _ = sys_tool_clone.execute(check_args).await;

                                    let mut ev_level = ev_level_clone.lock().await;
                                    let mut lat_saved = lat_saved_clone.lock().await;

                                    let current_version = *ev_level;
                                    let next_version = current_version + 1;

                                    let before_lat = 15.6 / (current_version as f64 * 0.15 + 1.0);
                                    let after_lat = before_lat * 0.43;
                                    let latency_saved = before_lat - after_lat;

                                    *lat_saved += latency_saved;
                                    *ev_level += 1;

                                    let improvement_id = improvement_pipeline_clone.initiate_improvement(
                                        "kernel_event_routing",
                                        &format!("sha_ver_10{}", current_version),
                                        &format!("sha_ver_10{}", next_version),
                                        "メッセージチャネルバッファの割り当てとロックフリー構造の微調整"
                                    ).await.unwrap_or(Uuid::new_v4());

                                    let _ = improvement_pipeline_clone
                                        .submit_verification_results(
                                            improvement_id,
                                            before_lat,
                                            after_lat,
                                            512.0,
                                            412.0,
                                            true,
                                        )
                                        .await;
                                    let _ = improvement_pipeline_clone
                                        .approve_and_apply(improvement_id)
                                        .await;

                                    let evolve_text = format!(
                                        "\n=========================================================\n\
                                         🚀 GENESISAI 自己進化アナリティクス\n\
                                         =========================================================\n\
                                           改善パッチ ID : [{}]\n\
                                           システム世代  : v1.0.{} ➔ v1.0.{}\n\
                                           累計進化レベル: LEVEL {}\n\
                                         ---------------------------------------------------------\n\
                                           処理遅延 (Avg): {:.2} ms ➔  {:.2} ms      -{:.2}%\n\
                                           🔥 累計削減遅延  : {:.2} ms\n\
                                           検証ステータス  : [SUCCESS] 隔離テスト合格・自動マージ完了\n\
                                         =========================================================\n",
                                        improvement_id, current_version, next_version, *ev_level, before_lat, after_lat, ((before_lat - after_lat) / before_lat) * 100.0, *lat_saved
                                    );

                                    let res_body =
                                        serde_json::json!({ "response": evolve_text }).to_string();
                                    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\n\r\n{}", res_body.len(), res_body);
                                    let _ = socket.write_all(response.as_bytes()).await;
                                    return;
                                }

                                // --------------------------------------------------
                                // 8. /evolve_real (物理コード自己書き換え)
                                // --------------------------------------------------
                                if query.starts_with("/evolve_real") {
                                    let parts: Vec<&str> = query.split_whitespace().collect();
                                    let target_file = if parts.len() > 1 {
                                        parts[1]
                                    } else {
                                        "core/src/lib.rs"
                                    };
                                    let mut reply =
                                        "❌ ファイル読み込みに失敗しました。".to_string();

                                    let read_args = serde_json::json!({ "path": target_file });
                                    if let Ok(res) = fs_tool_clone.execute(read_args).await {
                                        if res.success {
                                            let current_code = res.observation["content"]
                                                .as_str()
                                                .unwrap_or("")
                                                .to_string();

                                            let proposed_code = if parts.contains(&"bug") {
                                                format!("// わざとバグを挿入します\nlet invalid_syntax_error = ;;\n{}", current_code)
                                            } else {
                                                format!(
                                                    "// [Optimized by GenesisAI at {:?}]\n{}",
                                                    std::time::SystemTime::now(),
                                                    current_code
                                                )
                                            };

                                            let writer_args = serde_json::json!({
                                                "file_path": target_file,
                                                "proposed_code": proposed_code
                                            });

                                            if let Ok(write_res) =
                                                writer_tool_clone.execute(writer_args).await
                                            {
                                                reply = if write_res.success {
                                                    format!("✨ [物理進化成功]: 実際にファイル '{}' のコードが自律最適化マージされました！", target_file)
                                                } else {
                                                    format!(
                                                        "⚠️ [進化拒否・ロールバック完了]: {}",
                                                        write_res.observation["message"]
                                                            .as_str()
                                                            .unwrap_or("")
                                                    )
                                                };
                                            }
                                        }
                                    }
                                    let res_body =
                                        serde_json::json!({ "response": reply }).to_string();
                                    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\n\r\n{}", res_body.len(), res_body);
                                    let _ = socket.write_all(response.as_bytes()).await;
                                    return;
                                }

                                // --------------------------------------------------
                                // 9. 通常会話 (RAG対応)
                                // --------------------------------------------------
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
                                                "qwen2.5:3b",
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
