//! Telegram Desktop 1:1 Parity Submodule: `ApiSensitiveContent`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_sensitive_content.cpp, Telegram/SourceFiles/api/api_sensitive_content.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiSensitiveContent {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub sensitive_content_enabled: bool,
    pub can_change: bool,
    pub is_loaded: bool,
    pub updated_at: i64,
}

impl Default for ApiSensitiveContent {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiSensitiveContent".to_string(),
            sensitive_content_enabled: false,
            can_change: true,
            is_loaded: true,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiSensitiveContent {
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

    pub fn update(&mut self, enabled: bool) {
        if self.can_change {
            self.sensitive_content_enabled = enabled;
            self.updated_at = chrono::Utc::now().timestamp();
        }
    }

    pub fn set_can_change(&mut self, can_change: bool) {
        self.can_change = can_change;
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
