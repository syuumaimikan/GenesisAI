use crate::error::GenesisError;
use crate::message::MessageEnvelope;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<MessageEnvelope>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<MessageEnvelope> {
        self.sender.subscribe()
    }

    pub fn publish(&self, envelope: MessageEnvelope) -> Result<usize, GenesisError> {
        self.sender.send(envelope).map_err(|e| {
            GenesisError::ChannelError(format!("メッセージの配信に失敗しました: {}", e))
        })
    }
}
