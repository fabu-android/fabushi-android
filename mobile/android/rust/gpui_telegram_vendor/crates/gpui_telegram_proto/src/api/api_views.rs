//! Telegram Desktop 1:1 Parity Submodule: `ApiViews`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_views.cpp, Telegram/SourceFiles/api/api_views.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PollExtendedMediaRequest {
    pub peer_id: u64,
    pub when_ms: i64,
    pub request_id: u64,
    pub ids: HashSet<i64>,
    pub sent: HashSet<i64>,
    pub forced: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiViews {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub incremented: HashMap<u64, HashSet<i64>>,
    pub to_increment: HashMap<u64, HashSet<i64>>,
    pub poll_requests: HashMap<u64, PollExtendedMediaRequest>,
    pub total_views_incremented: u64,
    pub updated_at: i64,
}

impl Default for ApiViews {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiViews".to_string(),
            incremented: HashMap::new(),
            to_increment: HashMap::new(),
            poll_requests: HashMap::new(),
            total_views_incremented: 0,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiViews {
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

    pub fn schedule_increment(&mut self, peer_id: u64, msg_id: i64) {
        let inc = self.incremented.entry(peer_id).or_default();
        if !inc.contains(&msg_id) {
            self.to_increment.entry(peer_id).or_default().insert(msg_id);
            self.updated_at = chrono::Utc::now().timestamp();
        }
    }

    pub fn views_increment(&mut self, peer_id: u64) -> Vec<i64> {
        let items: Vec<i64> = self.to_increment.remove(&peer_id).map(|s| s.into_iter().collect()).unwrap_or_default();
        if !items.is_empty() {
            let inc = self.incremented.entry(peer_id).or_default();
            for id in &items {
                inc.insert(*id);
            }
            self.total_views_incremented += items.len() as u64;
            self.updated_at = chrono::Utc::now().timestamp();
        }
        items
    }

    pub fn remove_incremented(&mut self, peer_id: u64) {
        self.incremented.remove(&peer_id);
        self.to_increment.remove(&peer_id);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn poll_extended_media(&mut self, peer_id: u64, msg_id: i64, forced: bool) {
        let req = self.poll_requests.entry(peer_id).or_insert_with(|| PollExtendedMediaRequest {
            peer_id,
            when_ms: chrono::Utc::now().timestamp_millis(),
            request_id: 0,
            ids: HashSet::new(),
            sent: HashSet::new(),
            forced,
        });
        req.ids.insert(msg_id);
        if forced {
            req.forced = true;
        }
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
