//! Telegram Desktop 1:1 Parity Submodule: `ApiTranscribes`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_transcribes.cpp, Telegram/SourceFiles/api/api_transcribes.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TranscribeEntry {
    pub result: String,
    pub shown: bool,
    pub failed: bool,
    pub pending: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiTranscribes {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub entries: HashMap<(u64, i64), TranscribeEntry>,
    pub trials_count: i32,
    pub trials_max_length_ms: u64,
    pub updated_at: i64,
}

impl Default for ApiTranscribes {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiTranscribes".to_string(),
            entries: HashMap::new(),
            trials_count: 5,
            trials_max_length_ms: 120_000,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiTranscribes {
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

    pub fn request_transcription(&mut self, peer_id: u64, msg_id: i64) {
        self.entries.insert((peer_id, msg_id), TranscribeEntry {
            result: String::new(),
            shown: true,
            failed: false,
            pending: true,
        });
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn set_result(&mut self, peer_id: u64, msg_id: i64, text: impl Into<String>) {
        if let Some(entry) = self.entries.get_mut(&(peer_id, msg_id)) {
            entry.result = text.into();
            entry.pending = false;
            self.updated_at = chrono::Utc::now().timestamp();
        }
    }
}
