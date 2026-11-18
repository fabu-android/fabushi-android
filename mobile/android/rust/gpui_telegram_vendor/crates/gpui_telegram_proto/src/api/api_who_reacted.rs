//! Telegram Desktop 1:1 Parity Submodule: `ApiWhoReacted`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_who_reacted.cpp, Telegram/SourceFiles/api/api_who_reacted.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WhoReactedList {
    #[default]
    All,
    One,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhoReadPeer {
    pub peer_id: u64,
    pub date: i64,
    pub reaction: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiWhoReacted {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub cached_reactors: HashMap<(u64, i64), Vec<WhoReadPeer>>,
    pub updated_at: i64,
}

impl Default for ApiWhoReacted {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiWhoReacted".to_string(),
            cached_reactors: HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiWhoReacted {
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

    pub fn add_reactor(&mut self, peer_id: u64, msg_id: i64, reactor_id: u64, reaction: impl Into<String>) {
        let list = self.cached_reactors.entry((peer_id, msg_id)).or_default();
        if let Some(entry) = list.iter_mut().find(|r| r.peer_id == reactor_id) {
            entry.reaction = Some(reaction.into());
            entry.date = chrono::Utc::now().timestamp();
        } else {
            list.push(WhoReadPeer {
                peer_id: reactor_id,
                date: chrono::Utc::now().timestamp(),
                reaction: Some(reaction.into()),
            });
        }
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn get_reactors(&self, peer_id: u64, msg_id: i64) -> Vec<WhoReadPeer> {
        self.cached_reactors.get(&(peer_id, msg_id)).cloned().unwrap_or_default()
    }

    pub fn remove_reactor(&mut self, peer_id: u64, msg_id: i64, reactor_id: u64) -> bool {
        if let Some(list) = self.cached_reactors.get_mut(&(peer_id, msg_id)) {
            let initial = list.len();
            list.retain(|r| r.peer_id != reactor_id);
            if list.len() < initial {
                self.updated_at = chrono::Utc::now().timestamp();
                return true;
            }
        }
        false
    }
}
