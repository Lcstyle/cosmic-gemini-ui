use thiserror::Error;

#[derive(Debug, Error)]
pub enum HydraError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("crypto error: {0}")]
    Crypto(String),

    #[error("invalid event signature")]
    InvalidSignature,

    #[error("event log integrity violation: {0}")]
    LogIntegrity(String),

    #[error("peer error: {0}")]
    Peer(String),

    #[error("transport error: {0}")]
    Transport(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("tor error: {0}")]
    Tor(String),
}
