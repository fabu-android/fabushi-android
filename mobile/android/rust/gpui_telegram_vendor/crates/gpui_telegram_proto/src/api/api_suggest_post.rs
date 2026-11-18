//! Telegram Desktop 1:1 Parity Submodule: `ApiSuggestPost`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_suggest_post.cpp, Telegram/SourceFiles/api/api_suggest_post.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuggestAction {
    Accept,
    Decline,
    SuggestChanges,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuggestedPostRecord {
    pub peer_id: u64,
    pub msg_id: i64,
    pub status: SuggestAction,
    pub decline_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiSuggestPost {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub records: Vec<SuggestedPostRecord>,
    pub updated_at: i64,
}

impl Default for ApiSuggestPost {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiSuggestPost".to_string(),
            records: Vec::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiSuggestPost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_label(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.is_enabled = enabled;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn handle_suggest(&mut self, peer_id: u64, msg_id: i64, action: SuggestAction, reason: Option<String>) {
        self.records.push(SuggestedPostRecord {
            peer_id,
            msg_id,
            status: action,
            decline_reason: reason,
        });
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
