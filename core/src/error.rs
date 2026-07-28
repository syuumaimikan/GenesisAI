// core/src/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GenesisError {
    #[error("通信エラーが発生しました: {0}")]
    ChannelError(String),

    #[error("サブシステム '{subsystem}' でエラーが発生しました: {reason}")]
    SubsystemError { subsystem: String, reason: String },

    #[error("内部システムエラー: {0}")]
    Internal(#[from] anyhow::Error),
}
