//! Telegram Desktop 1:1 Parity Submodule: `ApiRingtones`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_ringtones.cpp, Telegram/SourceFiles/api/api_ringtones.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

pub const RINGTONE_MAX_SIZE_BYTES: i64 = 300 * 1024;
pub const RINGTONE_MAX_DURATION_MS: u64 = 5000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiRingtones {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub ringtone_document_ids: Vec<u64>,
    pub max_saved_count: usize,
    pub updated_at: i64,
}

impl Default for ApiRingtones {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiRingtones".to_string(),
            ringtone_document_ids: Vec::new(),
            max_saved_count: 100,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiRingtones {
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

    pub fn add_ringtone(&mut self, document_id: u64) {
        if !self.ringtone_document_ids.contains(&document_id) {
            self.ringtone_document_ids.push(document_id);
            self.updated_at = chrono::Utc::now().timestamp();
        }
    }

    pub fn remove_ringtone(&mut self, document_id: u64) -> bool {
        let initial = self.ringtone_document_ids.len();
        self.ringtone_document_ids.retain(|id| *id != document_id);
        if self.ringtone_document_ids.len() < initial {
            self.updated_at = chrono::Utc::now().timestamp();
            true
        } else {
            false
        }
    }
}
