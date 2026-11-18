//! Telegram Desktop 1:1 Parity Submodule: `ApiMedia`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_media.cpp, Telegram/SourceFiles/api/api_media.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use crate::api::api_common::RemoteFileInfo;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PreparedMedia {
    Photo { file: RemoteFileInfo, ttl_seconds: Option<i32> },
    Document { file: RemoteFileInfo, ttl_seconds: Option<i32> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiMedia {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub prepared_media_queue: Vec<PreparedMedia>,
    pub updated_at: i64,
}

impl Default for ApiMedia {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiMedia".to_string(),
            prepared_media_queue: Vec::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiMedia {
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

    pub fn prepare_photo(&mut self, file: RemoteFileInfo, ttl: Option<i32>) {
        self.prepared_media_queue.push(PreparedMedia::Photo {
            file,
            ttl_seconds: ttl,
        });
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn prepare_document(&mut self, file: RemoteFileInfo, ttl: Option<i32>) {
        self.prepared_media_queue.push(PreparedMedia::Document {
            file,
            ttl_seconds: ttl,
        });
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn has_attached_stickers(file: &RemoteFileInfo) -> bool {
        !file.attached_stickers.is_empty()
    }
}
