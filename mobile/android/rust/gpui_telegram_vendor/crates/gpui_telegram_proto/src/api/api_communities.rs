//! Telegram Desktop 1:1 Parity Submodule: `ApiCommunities`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_communities.cpp, Telegram/SourceFiles/api/api_communities.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PeerLinkAction {
    #[default]
    Visible,
    Hidden,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommunityPeerRequest {
    pub peer_id: u64,
    pub requested_by: u64,
    pub date: i64,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiCommunities {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub community_links: HashMap<u64, Vec<u64>>,
    pub collapsed_communities: Vec<u64>,
    pub updated_at: i64,
}

impl Default for ApiCommunities {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiCommunities".to_string(),
            community_links: HashMap::new(),
            collapsed_communities: Vec::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiCommunities {
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

    pub fn add_peer_link(&mut self, community_id: u64, peer_id: u64) {
        let list = self.community_links.entry(community_id).or_default();
        if !list.contains(&peer_id) {
            list.push(peer_id);
            self.updated_at = chrono::Utc::now().timestamp();
        }
    }

    pub fn remove_peer_link(&mut self, community_id: u64, peer_id: u64) -> bool {
        if let Some(list) = self.community_links.get_mut(&community_id) {
            let initial = list.len();
            list.retain(|p| *p != peer_id);
            if list.len() < initial {
                self.updated_at = chrono::Utc::now().timestamp();
                return true;
            }
        }
        false
    }

    pub fn toggle_collapsed(&mut self, community_id: u64, collapsed: bool) {
        if collapsed {
            if !self.collapsed_communities.contains(&community_id) {
                self.collapsed_communities.push(community_id);
            }
        } else {
            self.collapsed_communities.retain(|c| *c != community_id);
        }
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
