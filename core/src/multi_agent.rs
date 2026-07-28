// core/src/multi_agent.rs
use crate::bus::EventBus;
use crate::message::{EventPayload, MessageEnvelope};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AgentRole {
    Researcher,  // Web検索やファイル収集を担う
    Analyst,     // データ選別やコード監査を担う
    Synthesizer, // 最終的なまとめ・ドキュメンテーションを担う
}

pub struct DistributedAgent {
    pub name: String,
    pub role: AgentRole,
    event_bus: EventBus,
}

impl DistributedAgent {
    pub fn new(name: &str, role: AgentRole, event_bus: EventBus) -> Self {
        Self {
            name: name.to_string(),
            role,
            event_bus,
        }
    }

    pub async fn start_listening(&self) {
        let mut rx = self.event_bus.subscribe();
        let agent_name = self.name.clone();
        let role = self.role;
        let bus = self.event_bus.clone();

        tokio::spawn(async move {
            tracing::info!(
                "協調エージェント [{}] ({:?}) がネットワークに参加しました。",
                agent_name,
                role
            );
            while let Ok(msg) = rx.recv().await {
                // 自分のロールに関係するイベントを監視して自動協調
                match (role, &msg.payload) {
                    (AgentRole::Researcher, EventPayload::TaskScheduled { task_id, priority })
                        if task_id.contains("RESEARCH") =>
                    {
                        tracing::info!(
                            "🤖 [{}] タスクを受託。検索探索を実施します。TaskID: {}",
                            agent_name,
                            task_id
                        );
                        // リサーチ結果イベントを発行
                        let notification = MessageEnvelope::new(
                            &agent_name,
                            EventPayload::MemoryStored {
                                key: format!("research_done_for_{}", task_id),
                                importance: 0.8,
                            },
                        );
                        let _ = bus.publish(notification);
                    }
                    (AgentRole::Analyst, EventPayload::MemoryStored { key, .. })
                        if key.contains("research_done") =>
                    {
                        tracing::info!(
                            "🤖 [{}] リサーチ結果を検知。分析・評価を開始します。Key: {}",
                            agent_name,
                            key
                        );
                    }
                    _ => {}
                }
            }
        });
    }
}
