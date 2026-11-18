//! Telegram Desktop 1:1 Parity Submodule: `ApiAttachedStickers`
//! Subsystem: `api` | Feature ID: `TCG-N15`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_attached_stickers.cpp, Telegram/SourceFiles/api/api_attached_stickers.h

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttachedStickerSet {
    pub set_id: u64,
    pub title: String,
    pub short_name: String,
    pub count: i32,
    pub stickers: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiAttachedStickers {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub attached_sets_by_media: HashMap<u64, Vec<AttachedStickerSet>>,
    pub updated_at: i64,
}

impl Default for ApiAttachedStickers {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiAttachedStickers".to_string(),
            attached_sets_by_media: HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiAttachedStickers {
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

    pub fn register_attached_sets(&mut self, media_id: u64, sets: Vec<AttachedStickerSet>) {
        self.attached_sets_by_media.insert(media_id, sets);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn get_attached_sets(&self, media_id: u64) -> Vec<AttachedStickerSet> {
        self.attached_sets_by_media.get(&media_id).cloned().unwrap_or_default()
    }
}
