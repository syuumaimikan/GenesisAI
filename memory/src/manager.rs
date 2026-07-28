use crate::store::{MemoryEntry, MemoryMetadata, VectorStore};
use genesis_core::{EmbeddingEngine, EventBus, EventPayload};
use std::sync::Arc;
use std::time::SystemTime;
use uuid::Uuid;

pub struct MemoryManager<S: VectorStore> {
    store: Arc<S>,
    embedding_engine: Arc<dyn EmbeddingEngine>,
    event_bus: EventBus,
}

impl<S: VectorStore + 'static> MemoryManager<S> {
    pub fn new(
        store: Arc<S>,
        embedding_engine: Arc<dyn EmbeddingEngine>,
        event_bus: EventBus,
    ) -> Self {
        Self {
            store,
            embedding_engine,
            event_bus,
        }
    }

    pub async fn start_loop(&self) -> Result<(), anyhow::Error> {
        let mut rx = self.event_bus.subscribe();
        tracing::info!("長期記憶管理タスクが起動しました。");

        while let Ok(msg) = rx.recv().await {
            match msg.payload {
                EventPayload::MemoryStored { key, importance } => {
                    let content_to_embed = format!("メタナレッジキー: {}", key);

                    match self
                        .embedding_engine
                        .generate_embedding(&content_to_embed)
                        .await
                    {
                        Ok(vector) => {
                            let entry = MemoryEntry {
                                id: Uuid::new_v4(),
                                content: content_to_embed,
                                embedding: vector,
                                metadata: MemoryMetadata {
                                    importance,
                                    confidence: 1.0,
                                    timestamp: SystemTime::now(),
                                    source: msg.sender.clone(),
                                    symbolic_links: vec![],
                                },
                            };

                            if let Err(e) = self.store.insert(entry).await {
                                tracing::error!("記憶の永続化に失敗: {:?}", e);
                            } else {
                                tracing::info!("インデックス完了: 記憶項目 '{}'", key);
                            }
                        }
                        Err(e) => {
                            tracing::error!("埋め込みの生成に失敗しました: {:?}", e);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}
