//! Telegram Desktop 1:1 Parity Submodule: `ApiUnreadThings`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_unread_things.cpp, Telegram/SourceFiles/api/api_unread_things.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ThreadUnreadState {
    pub thread_id: i64,
    pub unread_mentions_count: u32,
    pub unread_reactions_count: u32,
    pub unread_poll_votes_count: u32,
    pub read_ids: HashSet<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiUnreadThings {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub states: HashMap<i64, ThreadUnreadState>,
    pub pending_requests: HashMap<i64, u64>,
    pub updated_at: i64,
}

impl Default for ApiUnreadThings {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiUnreadThings".to_string(),
            states: HashMap::new(),
            pending_requests: HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiUnreadThings {
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

    pub fn track_mentions(&self, thread_id: i64) -> bool {
        self.states
            .get(&thread_id)
            .map(|s| s.unread_mentions_count > 0)
            .unwrap_or(false)
    }

    pub fn track_reactions(&self, thread_id: i64) -> bool {
        self.states
            .get(&thread_id)
            .map(|s| s.unread_reactions_count > 0)
            .unwrap_or(false)
    }

    pub fn track_poll_votes(&self, thread_id: i64) -> bool {
        self.states
            .get(&thread_id)
            .map(|s| s.unread_poll_votes_count > 0)
            .unwrap_or(false)
    }

    pub fn set_counts(&mut self, thread_id: i64, mentions: u32, reactions: u32, poll_votes: u32) {
        let entry = self.states.entry(thread_id).or_default();
        entry.thread_id = thread_id;
        entry.unread_mentions_count = mentions;
        entry.unread_reactions_count = reactions;
        entry.unread_poll_votes_count = poll_votes;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn media_and_mentions_read(&mut self, thread_id: i64, read_ids: &[i64]) {
        if let Some(state) = self.states.get_mut(&thread_id) {
            for id in read_ids {
                state.read_ids.insert(*id);
            }
            state.unread_mentions_count = state.unread_mentions_count.saturating_sub(read_ids.len() as u32);
        }
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn cancel_requests(&mut self, thread_id: i64) {
        self.pending_requests.remove(&thread_id);
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
