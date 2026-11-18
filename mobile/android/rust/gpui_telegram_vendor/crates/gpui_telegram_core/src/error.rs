use thiserror::Error;

/// Unified error hierarchy for GPUI Telegram domain, networking, storage, and sync operations
#[derive(Debug, Error)]
pub enum TelegramError {
    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Session stale or generation fenced: expected {expected}, actual {actual}")]
    GenerationFenced { expected: u64, actual: u64 },

    #[error("Entity not found: {0}")]
    NotFound(String),

    #[error("PTS sequence gap detected: last_seen {last_seen}, incoming {incoming}")]
    PtsSequenceGap { last_seen: u64, incoming: u64 },

    #[error("Snapshot required for recovery")]
    SnapshotRequired,

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, TelegramError>;

impl From<serde_json::Error> for TelegramError {
    fn from(err: serde_json::Error) -> Self {
        TelegramError::Serialization(err.to_string())
    }
}

impl From<std::io::Error> for TelegramError {
    fn from(err: std::io::Error) -> Self {
        TelegramError::Internal(err.to_string())
    }
}

impl From<std::num::ParseIntError> for TelegramError {
    fn from(err: std::num::ParseIntError) -> Self {
        TelegramError::Validation(err.to_string())
    }
}

