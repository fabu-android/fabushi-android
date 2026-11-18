//! Telegram Desktop 1:1 Parity Submodule: `ApiBlockedPeers`
//! Subsystem: `api` | Feature ID: `TCG-N11`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_blocked_peers.cpp, Telegram/SourceFiles/api/api_blocked_peers.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockedItem {
    pub peer_id: u64,
    pub date: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BlockedSlice {
    pub list: Vec<BlockedItem>,
    pub total: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiBlockedPeers {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub slice: BlockedSlice,
    pub updated_at: i64,
}

impl Default for ApiBlockedPeers {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiBlockedPeers".to_string(),
            slice: BlockedSlice::default(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiBlockedPeers {
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

    pub fn block(&mut self, peer_id: u64) {
        if !self.is_blocked(peer_id) {
            self.slice.list.push(BlockedItem {
                peer_id,
                date: chrono::Utc::now().timestamp(),
            });
            self.slice.total = self.slice.list.len() as i32;
            self.updated_at = chrono::Utc::now().timestamp();
        }
    }

    pub fn unblock(&mut self, peer_id: u64) -> bool {
        let initial_len = self.slice.list.len();
        self.slice.list.retain(|i| i.peer_id != peer_id);
        if self.slice.list.len() < initial_len {
            self.slice.total = self.slice.list.len() as i32;
            self.updated_at = chrono::Utc::now().timestamp();
            true
        } else {
            false
        }
    }

    pub fn is_blocked(&self, peer_id: u64) -> bool {
        self.slice.list.iter().any(|i| i.peer_id == peer_id)
    }
}
