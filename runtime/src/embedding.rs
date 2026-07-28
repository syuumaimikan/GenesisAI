use async_trait::async_trait;
use genesis_core::embedding::EmbeddingEngine;
use genesis_core::error::GenesisError;
use serde::{Deserialize, Serialize};

pub struct OllamaEmbeddingEngine {
    client: reqwest::Client,
    endpoint: String,
    model: String,
    dimension: usize,
}

impl OllamaEmbeddingEngine {
    pub fn new(endpoint: &str, model: &str, dimension: usize) -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint: endpoint.to_string(),
            model: model.to_string(),
            dimension,
        }
    }
}

#[derive(Serialize)]
struct OllamaEmbeddingRequest {
    model: String,
    prompt: String,
}

#[derive(Deserialize)]
struct OllamaEmbeddingResponse {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingEngine for OllamaEmbeddingEngine {
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, GenesisError> {
        let request_body = OllamaEmbeddingRequest {
            model: self.model.clone(),
            prompt: text.to_string(),
        };

        let response = self
            .client
            .post(&format!("{}/api/embeddings", self.endpoint))
            .json(&request_body)
            .send()
            .await
            .map_err(|e| GenesisError::SubsystemError {
                subsystem: "OllamaEmbedding".to_string(),
                reason: format!("HTTPリクエスト失敗: {}", e),
            })?;

        let parsed: OllamaEmbeddingResponse =
            response
                .json()
                .await
                .map_err(|e| GenesisError::SubsystemError {
                    subsystem: "OllamaEmbedding".to_string(),
                    reason: format!("JSON解析失敗: {}", e),
                })?;

        if parsed.embedding.len() != self.dimension {
            return Err(GenesisError::SubsystemError {
                subsystem: "OllamaEmbedding".to_string(),
                reason: format!(
                    "期待値の次元 {} と、受信値の次元 {} が不一致です。",
                    self.dimension,
                    parsed.embedding.len()
                ),
            });
        }

        Ok(parsed.embedding)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}
