pub mod bus;
pub mod embedding;
pub mod error;
pub mod message;
pub mod multi_agent;

pub use bus::EventBus;
pub use embedding::EmbeddingEngine;
pub use error::GenesisError;
pub use message::{EventPayload, MessageEnvelope};
pub use multi_agent::{AgentRole, DistributedAgent};
