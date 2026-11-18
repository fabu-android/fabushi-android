//! Telegram Desktop 1:1 Parity Submodule: `ApiCredits`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_credits.cpp, Telegram/SourceFiles/api/api_credits.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::api::api_credits_history_entry::CreditsHistoryEntry;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiCredits {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub balances: HashMap<u64, i64>,
    pub history: HashMap<u64, Vec<CreditsHistoryEntry>>,
    pub updated_at: i64,
}

impl Default for ApiCredits {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiCredits".to_string(),
            balances: HashMap::new(),
            history: HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiCredits {
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

    pub fn balance(&self, peer_id: u64) -> i64 {
        self.balances.get(&peer_id).copied().unwrap_or(0)
    }

    pub fn add_credits(&mut self, peer_id: u64, amount: i64, title: impl Into<String>) {
        let bal = self.balances.entry(peer_id).or_insert(0);
        *bal += amount;
        self.history.entry(peer_id).or_default().push(CreditsHistoryEntry {
            id: format!("tx_{:x}", uuid::Uuid::new_v4().as_u128() as u64),
            date: chrono::Utc::now().timestamp(),
            stars_amount: amount,
            peer_id,
            title: title.into(),
            description: String::new(),
            is_incoming: true,
            is_refund: false,
        });
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
