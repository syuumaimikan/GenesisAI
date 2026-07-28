use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetadata {
    pub importance: f32,
    pub confidence: f32,
    pub timestamp: SystemTime,
    pub source: String,
    pub symbolic_links: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: Uuid,
    pub content: String,
    pub embedding: Vec<f32>,
    pub metadata: MemoryMetadata,
}

#[async_trait::async_trait]
pub trait VectorStore: Send + Sync {
    async fn insert(&self, entry: MemoryEntry) -> Result<(), anyhow::Error>;
    async fn search(
        &self,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<(MemoryEntry, f32)>, anyhow::Error>;
}
