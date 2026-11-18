//! Telegram Desktop 1:1 Parity Submodule: `ApiPolls`
//! Subsystem: `api` | Feature ID: `TCG-N16`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_polls.cpp, Telegram/SourceFiles/api/api_polls.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::cards::PollCard;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiPolls {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub active_polls: HashMap<(u64, i64), PollCard>,
    pub my_votes: HashMap<(u64, i64), Vec<Vec<u8>>>,
    pub closed_polls: Vec<(u64, i64)>,
    pub updated_at: i64,
}

impl Default for ApiPolls {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiPolls".to_string(),
            active_polls: HashMap::new(),
            my_votes: HashMap::new(),
            closed_polls: Vec::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiPolls {
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

    pub fn register_poll(&mut self, peer_id: u64, msg_id: i64, card: PollCard) {
        self.active_polls.insert((peer_id, msg_id), card);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn send_votes(&mut self, peer_id: u64, msg_id: i64, options: Vec<Vec<u8>>) {
        self.my_votes.insert((peer_id, msg_id), options);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn close_poll(&mut self, peer_id: u64, msg_id: i64) {
        if !self.closed_polls.contains(&(peer_id, msg_id)) {
            self.closed_polls.push((peer_id, msg_id));
            if let Some(card) = self.active_polls.get_mut(&(peer_id, msg_id)) {
                card.is_closed = true;
            }
            self.updated_at = chrono::Utc::now().timestamp();
        }
    }
}
