//! Telegram Desktop 1:1 Parity Submodule: `ApiCreditsHistoryEntry`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_credits_history_entry.cpp, Telegram/SourceFiles/api/api_credits_history_entry.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CreditsHistoryEntry {
    pub id: String,
    pub date: i64,
    pub stars_amount: i64,
    pub peer_id: u64,
    pub title: String,
    pub description: String,
    pub is_incoming: bool,
    pub is_refund: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiCreditsHistoryEntry {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub entry: CreditsHistoryEntry,
    pub updated_at: i64,
}

impl Default for ApiCreditsHistoryEntry {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiCreditsHistoryEntry".to_string(),
            entry: CreditsHistoryEntry::default(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiCreditsHistoryEntry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_entry(entry: CreditsHistoryEntry) -> Self {
        Self {
            entry,
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
}
