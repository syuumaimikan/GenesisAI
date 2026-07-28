use crate::store::{MemoryEntry, VectorStore};
use tokio::sync::RwLock;

pub struct InMemoryVectorStore {
    entries: RwLock<Vec<MemoryEntry>>,
}

impl InMemoryVectorStore {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn insert(&self, entry: MemoryEntry) -> Result<(), anyhow::Error> {
        let mut writer = self.entries.write().await;
        writer.push(entry);
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

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot_product: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
}
