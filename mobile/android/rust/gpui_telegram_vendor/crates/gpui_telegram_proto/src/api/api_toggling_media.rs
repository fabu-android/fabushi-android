//! Telegram Desktop 1:1 Parity Submodule: `ApiTogglingMedia`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_toggling_media.cpp, Telegram/SourceFiles/api/api_toggling_media.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiTogglingMedia {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub faved_stickers: HashSet<u64>,
    pub recent_stickers: Vec<u64>,
    pub saved_gifs: HashSet<u64>,
    pub saved_ringtones: HashSet<u64>,
    pub updated_at: i64,
}

impl Default for ApiTogglingMedia {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiTogglingMedia".to_string(),
            faved_stickers: HashSet::new(),
            recent_stickers: Vec::new(),
            saved_gifs: HashSet::new(),
            saved_ringtones: HashSet::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiTogglingMedia {
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

    pub fn toggle_faved_sticker(&mut self, document_id: u64, faved: bool) {
        if faved {
            self.faved_stickers.insert(document_id);
        } else {
            self.faved_stickers.remove(&document_id);
        }
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn toggle_recent_sticker(&mut self, document_id: u64, saved: bool) {
        self.recent_stickers.retain(|id| *id != document_id);
        if saved {
            self.recent_stickers.insert(0, document_id);
        }
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn toggle_saved_gif(&mut self, document_id: u64, saved: bool) {
        if saved {
            self.saved_gifs.insert(document_id);
        } else {
            self.saved_gifs.remove(&document_id);
        }
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn toggle_saved_ringtone(&mut self, document_id: u64, saved: bool) {
        if saved {
            self.saved_ringtones.insert(document_id);
        } else {
            self.saved_ringtones.remove(&document_id);
        }
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
