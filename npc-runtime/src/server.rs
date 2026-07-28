// npc-runtime/src/server.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::RwLock;

/// NPCが好感度達成時にプレゼント（自律PC干渉）するファイルの構造定義
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiftConfig {
    pub file_name: String,    // 生成する物理ファイル名 (例: "mina_secret.txt")
    pub file_content: String, // 物理ファイルの内容文
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcState {
    pub name: String,
    pub personality: String,
    pub background: String,
    pub current_emotion: String,
    pub affection: i32,
    pub dialogue_history: Vec<(String, String)>,
    pub goals: Vec<String>,
    pub has_gifted: bool,
    pub gift: Option<GiftConfig>, // 【新設】JSONからのみ読み込まれる、物理ファイル干渉の独自設定
}

impl NpcState {
    pub fn evaluate_sentiment(&mut self, player_message: &str) {
        let lower_msg = player_message.to_lowercase();
        if lower_msg.contains("ありがとう")
            || lower_msg.contains("助け")
            || lower_msg.contains("お土産")
            || lower_msg.contains("すごい")
            || lower_msg.contains("尊敬")
        {
            self.affection = (self.affection + 10).min(100);
            self.current_emotion = "喜んでいる、親密".to_string();
        } else if lower_msg.contains("バカ")
            || lower_msg.contains("弱い")
            || lower_msg.contains("うるさい")
            || lower_msg.contains("邪魔")
        {
            self.affection = (self.affection - 15).max(-100);
            self.current_emotion = "不快、敵対的".to_string();
        } else {
            self.current_emotion = "通常".to_string();
        }
    }
}

pub struct SimulationWorld {
    pub npc_database: RwLock<HashMap<String, NpcState>>,
    pub config_path: String,
}

impl SimulationWorld {
    pub async fn load_or_create_default(path: &str) -> Result<Self, anyhow::Error> {
        let path_buf = std::path::Path::new(path);

        let npcs = if path_buf.exists() {
            tracing::info!(
                "📂 [NPCシステム] データベースファイル '{}' からキャラクターデータを読み込みます。",
                path
            );
            let data = tokio::fs::read_to_string(path_buf).await?;
            serde_json::from_str(&data)?
        } else {
            tracing::info!(
                "✨ [NPCシステム] '{}' が見つからないため、テンプレートスキーマを自動作成します。",
                path
            );
            let mut default_db = HashMap::new();

            // プログラムからはハードコードを消去し、初回ファイル作成時のみのテンプレートとして利用
            default_db.insert("Eldrin".to_string(), NpcState {
                name: "エルドリン".to_string(),
                personality: "賢明で慎重な、古代の魔法学者。少し頑固だが知識を尊ぶ長老。".to_string(),
                background: "深脊界の古代碑文を100年間研究しているが、未だ完全な解読には至っていない。".to_string(),
                current_emotion: "平穏".to_string(),
                affection: 0,
                dialogue_history: Vec::new(),
                goals: vec!["古代碑文の完全解読".to_string()],
                has_gifted: false,
                gift: Some(GiftConfig {
                    file_name: "eldrin_ancient_key.txt".to_string(),
                    file_content: "【魔法学者エルドリンの研究室の鍵】\nお主の熱意に免じて書庫のアクセス権を授けよう。".to_string(),
                }),
            });

            default_db.insert(
                "Mina".to_string(),
                NpcState {
                    name: "ミナ".to_string(),
                    personality:
                        "明るく元気な宿屋『木漏れ日亭』の看板娘。お喋りが大好きでお節介焼き。"
                            .to_string(),
                    background: "旅人や商人から様々な裏情報を仕入れている情報通。".to_string(),
                    current_emotion: "元気".to_string(),
                    affection: 0,
                    dialogue_history: Vec::new(),
                    goals: vec!["宿屋を大繁盛させる".to_string()],
                    has_gifted: false,
                    gift: Some(GiftConfig {
                        file_name: "mina_secret_gossip.txt".to_string(),
                        file_content:
                            "【ミナの秘密の噂話メモ】\n幻の第七碑文は廃鉱山の地下3階にあるそうよ！"
                                .to_string(),
                    }),
                },
            );

            // 物理JSONの書き出し
            let pretty_json = serde_json::to_string_pretty(&default_db)?;
            tokio::fs::write(path_buf, pretty_json).await?;

            default_db
        };

        Ok(Self {
            npc_database: RwLock::new(npcs),
            config_path: path.to_string(),
        })
    }

    pub async fn save_to_disk(&self) -> Result<(), anyhow::Error> {
        let db = self.npc_database.read().await;
        let pretty_json = serde_json::to_string_pretty(&*db)?;
        tokio::fs::write(&self.config_path, pretty_json).await?;
        Ok(())
    }
}

pub struct NpcServer {
    pub world: Arc<SimulationWorld>,
    port: u16,
}

impl NpcServer {
    pub async fn init(port: u16, config_path: &str) -> Result<Self, anyhow::Error> {
        let world = Arc::new(SimulationWorld::load_or_create_default(config_path).await?);
        Ok(Self { world, port })
    }

    /// 【修正：完全汎用化】
    /// ハードコードされたマッチングを全廃。
    /// JSONの「gift」設定に定義されているファイル名とコンテンツを動的に取得して物理生成します。
    pub async fn check_and_trigger_intervention(
        &self,
        npc_id: &str,
    ) -> Result<Option<(String, String)>, anyhow::Error> {
        let mut npc_db = self.world.npc_database.write().await;
        let npc = npc_db.get_mut(npc_id).unwrap();

        if npc.affection >= 20 && !npc.has_gifted {
            if let Some(ref gift_cfg) = npc.gift {
                npc.has_gifted = true;

                let file_name = gift_cfg.file_name.clone();
                let content = gift_cfg.file_content.clone();
                // 【解決】ロックを解除（drop）する前に、必要な文字列をクローンして所有権を奪っておきます
                let npc_name = npc.name.clone();

                // 物理プレゼントファイルを生成（非同期）
                tokio::fs::write(&file_name, &content).await?;

                // ここで安全に書き込みロックを解放します（デッドロック回避）
                drop(npc_db);

                // 読み込みロックを取得してJSONに保存
                self.world.save_to_disk().await?;

                // クローン済みの npc_name を返すため、ボローチェッカーをパスします
                return Ok(Some((file_name, npc_name)));
            }
        }
        Ok(None)
    }

    pub async fn generate_npc_response(
        &self,
        npc_id: &str,
        player_message: &str,
        use_real_llm: bool,
    ) -> Result<String, anyhow::Error> {
        let mut npc_db = self.world.npc_database.write().await;
        let npc = npc_db
            .get_mut(npc_id)
            .ok_or_else(|| anyhow::anyhow!("NPCが存在しません。"))?;

        npc.evaluate_sentiment(player_message);

        let history_str: String = npc
            .dialogue_history
            .iter()
            .take(5)
            .map(|(p, n)| format!("プレイヤー: 「{}」\n{}: 「{}」\n", p, npc.name, n))
            .collect();

        let prompt = format!(
            "あなたはファンタジーRPGのNPC「{}」としてロールプレイを行います。プロフィール、感情、好感度に基づいて、プレイヤーの発言に完全になりきって返答してください。\n\n\
            【プロフィール】: {}\n\
            【背景・目的】: {}\n\
            【現在の感情】: {}\n\
            【好感度 (-100〜100)】: {} (値が高いほど親密、マイナスは冷酷・拒絶)\n\n\
            【過去の会話履歴】:\n\
            {}\n\
            【プレイヤーの発言】:\n\
            「{}」\n\n\
            お喋りや説明は禁止。{}としての台詞のみを日本語で出力してください。地の文（ト書き）を含めても完結に含めて構いません。",
            npc.name, npc.personality, npc.background, npc.current_emotion, npc.affection, history_str, player_message, npc.name
        );

        let reply = if use_real_llm {
            let client = reqwest::Client::new();
            let body = serde_json::json!({
                "model": "llama3",
                "prompt": prompt,
                "stream": false
            });
            match client
                .post("http://localhost:11434/api/generate")
                .json(&body)
                .send()
                .await
            {
                Ok(res) => {
                    let parsed: serde_json::Value = res.json().await?;
                    parsed["response"]
                        .as_str()
                        .unwrap_or("...すまぬ、少々考え込んでおった。")
                        .to_string()
                }
                Err(_) => self.generate_mock_reply(npc, player_message),
            }
        } else {
            self.generate_mock_reply(npc, player_message)
        };

        npc.dialogue_history
            .push((player_message.to_string(), reply.clone()));

        drop(npc_db);
        self.world.save_to_disk().await?;

        Ok(reply)
    }

    /// 【修正：完全汎用化】
    /// データベース全体のデータ項目（名前、感情、目標、発言）をテンプレートとして動的結合し、
    /// JSONに定義されたあらゆるキャラクター（追加NPC含む）に対して汎用的に動作する模擬台詞エンジンに進化。
    fn generate_mock_reply(&self, npc: &NpcState, message: &str) -> String {
        let name = &npc.name;
        if npc.affection >= 20 {
            format!(
                "({}で微笑みながら) おお、お前か！お前の「{}」という言葉、胸に響いたぞ！お近づきの印に、私からの特別なギフトを渡そう！", 
                npc.current_emotion, message
            )
        } else if npc.affection <= -15 {
            format!(
                "(冷酷に睨みつけながら) 鬱陶しいな…。「{}」などとほざく奴は、私の目的({:?})の邪魔だ。立ち去れ。", 
                message, npc.goals
            )
        } else {
            format!(
                "(真剣な表情で) ふむ。「{}」か。お前の主張も一理ある。私の背景（{}）から考えても、傾聴に値する内容だ。", 
                message, npc.background
            )
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
                            if request.contains("GET /npc/eldrin") {
                                let npcs = world_clone.npc_database.read().await;
                                if let Some(npc) = npcs.get("Eldrin") {
                                    let response_json = serde_json::to_string_pretty(npc).unwrap();
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
