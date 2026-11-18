use crate::types::UserId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Session security token representation with SHA-256 hash tracking and
/// constant-time token verification matching `inherited/host/src/device_pairing_tokens.rs`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionToken {
    pub session_id: String,
    pub user_id: UserId,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub device_name: String,
    pub token_hash: String,
}

impl SessionToken {
    /// Compute hex-encoded SHA-256 digest of a raw token
    pub fn hash_token(raw_token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(raw_token.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Create a new session token by hashing the provided raw token
    pub fn new(
        session_id: impl Into<String>,
        user_id: UserId,
        expires_at: DateTime<Utc>,
        device_name: impl Into<String>,
        raw_token: &str,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            user_id,
            created_at: Utc::now(),
            expires_at,
            device_name: device_name.into(),
            token_hash: Self::hash_token(raw_token),
        }
    }

    /// Create a session token with an already hashed token string
    pub fn new_with_hash(
        session_id: impl Into<String>,
        user_id: UserId,
        created_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        device_name: impl Into<String>,
        token_hash: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            user_id,
            created_at,
            expires_at,
            device_name: device_name.into(),
            token_hash: token_hash.into(),
        }
    }

    /// Check if session has expired against current wall-clock time
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }

    /// Check if session is currently valid
    pub fn is_valid(&self) -> bool {
        !self.is_expired()
    }

    /// Constant-time verification of candidate token hash against stored hash
    pub fn verify_token_hash(&self, candidate_hash: &str) -> bool {
        if candidate_hash.len() != self.token_hash.len() {
            return false;
        }
        candidate_hash
            .as_bytes()
            .ct_eq(self.token_hash.as_bytes())
            .into()
    }

    /// Constant-time verification of raw plaintext token by hashing and comparing
    pub fn verify_raw_token(&self, candidate_raw: &str) -> bool {
        let computed_hash = Self::hash_token(candidate_raw);
        self.verify_token_hash(&computed_hash)
    }
}

