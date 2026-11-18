//! Telegram Desktop 1:1 Parity Submodule: `ApiSendProgress`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_send_progress.cpp, Telegram/SourceFiles/api/api_send_progress.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum SendProgressType {
    #[default]
    Typing,
    RecordVideo,
    UploadVideo,
    RecordVoice,
    UploadVoice,
    RecordRound,
    UploadRound,
    UploadPhoto,
    UploadFile,
    ChooseLocation,
    ChooseContact,
    ChooseSticker,
    PlayGame,
    Speaking,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendProgress {
    pub progress_type: SendProgressType,
    pub until_timestamp_ms: i64,
    pub progress_percentage: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProgressKey {
    pub peer_id: u64,
    pub top_msg_id: i64,
    pub progress_type: SendProgressType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiSendProgress {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub active_progress: HashMap<ProgressKey, SendProgress>,
    pub updated_at: i64,
}

impl Default for ApiSendProgress {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiSendProgress".to_string(),
            active_progress: HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiSendProgress {
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

    pub fn update(&mut self, peer_id: u64, top_msg_id: i64, progress_type: SendProgressType, progress: i32) {
        let key = ProgressKey {
            peer_id,
            top_msg_id,
            progress_type,
        };
        let until = chrono::Utc::now().timestamp_millis() + 5000;
        self.active_progress.insert(key, SendProgress {
            progress_type,
            until_timestamp_ms: until,
            progress_percentage: progress,
        });
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn cancel(&mut self, peer_id: u64, top_msg_id: i64, progress_type: SendProgressType) {
        let key = ProgressKey {
            peer_id,
            top_msg_id,
            progress_type,
        };
        self.active_progress.remove(&key);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn cancel_typing(&mut self, peer_id: u64) {
        self.active_progress.retain(|k, _| !(k.peer_id == peer_id && k.progress_type == SendProgressType::Typing));
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn has_progress(&self, peer_id: u64) -> bool {
        let now = chrono::Utc::now().timestamp_millis();
        self.active_progress.iter().any(|(k, v)| k.peer_id == peer_id && v.until_timestamp_ms > now)
    }
}
