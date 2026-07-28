use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventPayload {
    MemoryStored { key: String, importance: f32 },
    TaskScheduled { task_id: String, priority: u8 },
    ActionExecuted { action_id: String, success: bool },
    ShutdownRequested,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEnvelope {
    pub id: Uuid,
    pub timestamp: SystemTime,
    pub sender: String,
    pub payload: EventPayload,
}

impl MessageEnvelope {
    pub fn new(sender: impl Into<String>, payload: EventPayload) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: SystemTime::now(),
            sender: sender.into(),
            payload,
        }
    }
}
