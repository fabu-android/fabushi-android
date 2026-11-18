//! Telegram Desktop 1:1 Parity Submodule: `ApiPeerPhoto`
//! Subsystem: `api` | Feature ID: `TCG-N11`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_peer_photo.cpp, Telegram/SourceFiles/api/api_peer_photo.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum EmojiListType {
    #[default]
    Profile,
    Group,
    Background,
    NoChannelStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UploadType {
    #[default]
    Default,
    Suggestion,
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct UserPhoto {
    pub image_data: Vec<u8>,
    pub markup_document_id: u64,
    pub video_data: Option<Vec<u8>>,
    pub video_start_ts: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UploadProgress {
    pub peer_id: u64,
    pub progress: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiPeerPhoto {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub active_uploads: HashMap<u64, UploadProgress>,
    pub user_photos: HashMap<u64, Vec<u64>>,
    pub non_personal_photos: HashMap<u64, u64>,
    pub emoji_lists: HashMap<EmojiListType, Vec<u64>>,
    pub updated_at: i64,
}

impl Default for ApiPeerPhoto {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiPeerPhoto".to_string(),
            active_uploads: HashMap::new(),
            user_photos: HashMap::new(),
            non_personal_photos: HashMap::new(),
            emoji_lists: HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiPeerPhoto {
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

    pub fn upload(&mut self, peer_id: u64, _photo: UserPhoto, _upload_type: UploadType) {
        self.active_uploads.insert(peer_id, UploadProgress {
            peer_id,
            progress: 0.0,
        });
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn update_progress(&mut self, peer_id: u64, progress: f64) {
        if let Some(entry) = self.active_uploads.get_mut(&peer_id) {
            entry.progress = progress;
            self.updated_at = chrono::Utc::now().timestamp();
        }
    }

    pub fn complete_upload(&mut self, peer_id: u64, photo_id: u64) {
        self.active_uploads.remove(&peer_id);
        self.user_photos.entry(peer_id).or_default().push(photo_id);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn cancel_upload(&mut self, peer_id: u64) {
        self.active_uploads.remove(&peer_id);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn set_emoji_list(&mut self, list_type: EmojiListType, documents: Vec<u64>) {
        self.emoji_lists.insert(list_type, documents);
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
