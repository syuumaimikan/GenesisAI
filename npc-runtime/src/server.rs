// npc-runtime/src/server.rs
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::RwLock;

pub struct NpcState {
    pub name: String,
    pub personality: String,
    pub memories: Vec<String>,
}

pub struct SimulationWorld {
    pub world_history: RwLock<Vec<String>>,
    pub npc_database: RwLock<HashMap<String, NpcState>>,
}

impl SimulationWorld {
    pub fn new() -> Self {
        let mut npcs = HashMap::new();
        npcs.insert(
            "Eldrin".to_string(),
            NpcState {
                name: "Eldrin".to_string(),
                personality: "賢明で慎重な、古代の魔法学者。".to_string(),
                memories: vec!["プレイヤーに古代碑文の解読を依頼した".to_string()],
            },
        );

        Self {
            world_history: RwLock::new(vec!["世界創生期".to_string()]),
            npc_database: RwLock::new(npcs),
        }
    }
}

pub struct NpcServer {
    world: Arc<SimulationWorld>,
    port: u16,
}

impl NpcServer {
    pub fn new(port: u16) -> Self {
        Self {
            world: Arc::new(SimulationWorld::new()),
            port,
        }
    }

    pub async fn start_api_server(&self) -> Result<(), anyhow::Error> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.port)).await?;
        tracing::info!(
            "🎮 [NPC-Runtime Server] 起動完了。Godot/Unity 接続待機中 (Port: {})",
            self.port
        );

        let world = self.world.clone();
        tokio::spawn(async move {
            loop {
                if let Ok((mut socket, _)) = listener.accept().await {
                    let world_clone = world.clone();
                    tokio::spawn(async move {
                        let mut buffer = [0; 1024];
                        if let Ok(size) = socket.read(&mut buffer).await {
                            let request = String::from_utf8_lossy(&buffer[..size]);

                            // 簡単なHTTP REST風レスポンスの生成 (Godot/UnityからのJSON POST対応)
                            if request.contains("GET /npc/eldrin") {
                                let npcs = world_clone.npc_database.read().await;
                                if let Some(npc) = npcs.get("Eldrin") {
                                    let response_json = serde_json::json!({
                                        "name": npc.name,
                                        "personality": npc.personality,
                                        "current_memory": npc.memories
                                    })
                                    .to_string();

                                    let http_res = format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                        response_json.len(), response_json
                                    );
                                    let _ = socket.write_all(http_res.as_bytes()).await;
                                }
                            } else {
                                let error_res =
                                    "HTTP/1.1 404 NOT FOUND\r\nContent-Length: 0\r\n\r\n";
                                let _ = socket.write_all(error_res.as_bytes()).await;
                            }
                        }
                    });
                }
            }
        });

        Ok(())
    }
}
