//! Telegram Desktop 1:1 Parity Submodule: `ApiPeerSearch`
//! Subsystem: `api` | Feature ID: `TCG-N21`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_peer_search.cpp, Telegram/SourceFiles/api/api_peer_search.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PeerSearchType {
    #[default]
    WithSponsored,
    JustPeers,
    Channels,
    Bots,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SponsoredSearchResult {
    pub peer_id: u64,
    pub random_id: Vec<u8>,
    pub sponsor_info: String,
    pub additional_info: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PeerSearchResult {
    pub query: String,
    pub my_contacts: Vec<u64>,
    pub global_peers: Vec<u64>,
    pub sponsored: Vec<SponsoredSearchResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiPeerSearch {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub search_type: PeerSearchType,
    pub cache: HashMap<String, PeerSearchResult>,
    pub active_queries: Vec<String>,
    pub updated_at: i64,
}

impl Default for ApiPeerSearch {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiPeerSearch".to_string(),
            search_type: PeerSearchType::WithSponsored,
            cache: HashMap::new(),
            active_queries: Vec::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiPeerSearch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_type(search_type: PeerSearchType) -> Self {
        Self {
            search_type,
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

    pub fn search(&mut self, query: impl Into<String>) -> Option<PeerSearchResult> {
        let q = query.into();
        if let Some(cached) = self.cache.get(&q) {
            Some(cached.clone())
        } else {
            if !self.active_queries.contains(&q) {
                self.active_queries.push(q);
                self.updated_at = chrono::Utc::now().timestamp();
            }
            None
        }
    }

    pub fn store_result(&mut self, result: PeerSearchResult) {
        self.active_queries.retain(|q| q != &result.query);
        self.cache.insert(result.query.clone(), result);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.active_queries.clear();
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
