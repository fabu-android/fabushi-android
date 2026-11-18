//! Telegram Desktop 1:1 Parity Submodule: `ApiShareThemes`
//! Subsystem: `api` | Feature ID: `TCG-N31`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_share_themes.cpp, Telegram/SourceFiles/api/api_share_themes.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ShareThemeData {
    pub theme_id: u64,
    pub title: String,
    pub slug: String,
    pub wallpaper_document_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiShareThemes {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub themes_by_peer: HashMap<u64, ShareThemeData>,
    pub updated_at: i64,
}

impl Default for ApiShareThemes {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiShareThemes".to_string(),
            themes_by_peer: HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiShareThemes {
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

    pub fn apply_theme(&mut self, peer_id: u64, theme: ShareThemeData) {
        self.themes_by_peer.insert(peer_id, theme);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn get_theme(&self, peer_id: u64) -> Option<&ShareThemeData> {
        self.themes_by_peer.get(&peer_id)
    }
}
