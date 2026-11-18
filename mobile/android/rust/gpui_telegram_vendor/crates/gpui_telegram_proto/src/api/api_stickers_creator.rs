//! Telegram Desktop 1:1 Parity Submodule: `ApiStickersCreator`
//! Subsystem: `api` | Feature ID: `TCG-N15`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_stickers_creator.cpp, Telegram/SourceFiles/api/api_stickers_creator.h

use serde::{Deserialize, Serialize};

pub const STICKERS_IN_OWNED_SET_MAX: usize = 120;
pub const EMOJI_IN_OWNED_SET_MAX: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StickersType {
    #[default]
    Stickers,
    CustomEmoji,
    Masks,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateStickerSetArgs {
    pub title: String,
    pub short_name: String,
    pub stickers_type: StickersType,
    pub animated: bool,
    pub video: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StickerUploadItem {
    pub file_bytes: Vec<u8>,
    pub emoji: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiStickersCreator {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub owned_sets: Vec<CreateStickerSetArgs>,
    pub pending_uploads: Vec<StickerUploadItem>,
    pub updated_at: i64,
}

impl Default for ApiStickersCreator {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiStickersCreator".to_string(),
            owned_sets: Vec::new(),
            pending_uploads: Vec::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiStickersCreator {
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

    pub fn create_set(&mut self, args: CreateStickerSetArgs) {
        self.owned_sets.push(args);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn queue_upload(&mut self, item: StickerUploadItem) {
        self.pending_uploads.push(item);
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
