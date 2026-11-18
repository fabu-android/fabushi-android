//! Telegram Desktop 1:1 Parity Submodule: `ApiChatFilters`
//! Subsystem: `api` | Feature ID: `TCG-N37`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_chat_filters.cpp, Telegram/SourceFiles/api/api_chat_filters.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChatFilterData {
    pub filter_id: i32,
    pub title: String,
    pub icon_emoji: Option<String>,
    pub pinned_peers: Vec<u64>,
    pub include_peers: Vec<u64>,
    pub exclude_peers: Vec<u64>,
    pub exclude_muted: bool,
    pub exclude_read: bool,
    pub exclude_archived: bool,
    pub include_contacts: bool,
    pub include_non_contacts: bool,
    pub include_groups: bool,
    pub include_channels: bool,
    pub include_bots: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiChatFilters {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub filters: Vec<ChatFilterData>,
    pub updated_at: i64,
}

impl Default for ApiChatFilters {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiChatFilters".to_string(),
            filters: Vec::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiChatFilters {
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

    pub fn add_filter(&mut self, filter: ChatFilterData) {
        self.filters.retain(|f| f.filter_id != filter.filter_id);
        self.filters.push(filter);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn remove_filter(&mut self, filter_id: i32) -> bool {
        let initial_len = self.filters.len();
        self.filters.retain(|f| f.filter_id != filter_id);
        if self.filters.len() < initial_len {
            self.updated_at = chrono::Utc::now().timestamp();
            true
        } else {
            false
        }
    }

    pub fn save_new_filter_pinned(&mut self, filter_id: i32, peer_id: u64) {
        if let Some(f) = self.filters.iter_mut().find(|f| f.filter_id == filter_id) {
            if !f.pinned_peers.contains(&peer_id) {
                f.pinned_peers.push(peer_id);
                self.updated_at = chrono::Utc::now().timestamp();
            }
        }
    }
}
