// memory/src/index.rs (物理永続化対応の完全コード)
use crate::store::{MemoryEntry, VectorStore};
use tokio::sync::RwLock;

pub struct InMemoryVectorStore {
    entries: RwLock<Vec<MemoryEntry>>,
    config_path: String, // 永続化用JSONファイルのパス
}

impl InMemoryVectorStore {
    /// 【新設】指定されたパスのJSONファイルから記憶をロードする。存在しない場合は自動新規作成します。
    pub async fn load_or_create(path: &str) -> Result<Self, anyhow::Error> {
        let path_buf = std::path::Path::new(path);

        let entries = if path_buf.exists() {
            tracing::info!("📂 [記憶システム] 長期記憶ファイル '{}' から過去の記憶データをすべて脳内へ復元します。", path);
            let data = tokio::fs::read_to_string(path_buf).await?;
            serde_json::from_str(&data)?
        } else {
            tracing::info!(
                "✨ [記憶システム] 新規の長期記憶データベース '{}' を初期化作成します。",
                path
            );
            let empty_db: Vec<MemoryEntry> = Vec::new();
            let pretty_json = serde_json::to_string_pretty(&empty_db)?;
            tokio::fs::write(path_buf, pretty_json).await?;
            empty_db
        };

        Ok(Self {
            entries: RwLock::new(entries),
            config_path: path.to_string(),
        })
    }

    /// 現在脳内にあるすべての長期記憶を物理ディスクへ不揮発保存します。
    pub async fn save_to_disk(&self) -> Result<(), anyhow::Error> {
        let reader = self.entries.read().await;
        let pretty_json = serde_json::to_string_pretty(&*reader)?;
        tokio::fs::write(&self.config_path, pretty_json).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn insert(&self, entry: MemoryEntry) -> Result<(), anyhow::Error> {
        let mut writer = self.entries.write().await;
        writer.push(entry);
        drop(writer); // 書き込みロックを速やかに解放（デッドロック対策）

        // 【新設】記憶が格納された瞬間、自動で 'memories.json' に非同期保存
        self.save_to_disk().await?;
        Ok(())
    }

    async fn search(
        &self,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<(MemoryEntry, f32)>, anyhow::Error> {
        let reader = self.entries.read().await;

        if reader.is_empty() {
            return Ok(Vec::new());
        }

        let mut results: Vec<(MemoryEntry, f32)> = reader
            .iter()
            .map(|entry| {
                let sim = cosine_similarity(&entry.embedding, query_vector);
                (entry.clone(), sim)
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);

        Ok(results)
    }
}

/// 高速コサイン類似度計算
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot_product = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    for (x, y) in a.iter().zip(b) {
        dot_product += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let norm_a = norm_a.sqrt();
    let norm_b = norm_b.sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
}
