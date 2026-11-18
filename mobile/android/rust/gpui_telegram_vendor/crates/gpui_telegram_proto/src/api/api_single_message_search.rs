//! Telegram Desktop 1:1 Parity Submodule: `ApiSingleMessageSearch`
//! Subsystem: `api` | Feature ID: `TCG-N21`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_single_message_search.cpp, Telegram/SourceFiles/api/api_single_message_search.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SingleMessageSearchKey {
    pub domain_or_id: String,
    pub post_id: i64,
}

impl SingleMessageSearchKey {
    pub fn new(domain_or_id: impl Into<String>, post_id: i64) -> Self {
        Self {
            domain_or_id: domain_or_id.into(),
            post_id,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.domain_or_id.is_empty() || self.post_id == 0
    }
}

pub fn convert_peer_search_query(query: &str) -> String {
    let trimmed = query.trim();
    if let Some(username) = trimmed.strip_prefix('@') {
        username.to_string()
    } else if let Some(stripped) = trimmed.strip_prefix("https://t.me/") {
        stripped.to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiSingleMessageSearch {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub cached_lookups: HashMap<SingleMessageSearchKey, (u64, i64)>,
    pub pending_requests: Vec<SingleMessageSearchKey>,
    pub updated_at: i64,
}

impl Default for ApiSingleMessageSearch {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiSingleMessageSearch".to_string(),
            cached_lookups: HashMap::new(),
            pending_requests: Vec::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiSingleMessageSearch {
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

    pub fn lookup(&mut self, key: SingleMessageSearchKey) -> Option<(u64, i64)> {
        if let Some(&cached) = self.cached_lookups.get(&key) {
            Some(cached)
        } else {
            if !self.pending_requests.contains(&key) {
                self.pending_requests.push(key);
                self.updated_at = chrono::Utc::now().timestamp();
            }
            None
        }
    }

    pub fn store_lookup(&mut self, key: SingleMessageSearchKey, peer_id: u64, msg_id: i64) {
        self.cached_lookups.insert(key, (peer_id, msg_id));
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn clear(&mut self) {
        self.cached_lookups.clear();
        self.pending_requests.clear();
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
