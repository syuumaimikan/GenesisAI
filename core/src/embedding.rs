use async_trait::async_trait;

#[async_trait]
pub trait EmbeddingEngine: Send + Sync {
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, crate::error::GenesisError>;
    fn dimension(&self) -> usize;
}
