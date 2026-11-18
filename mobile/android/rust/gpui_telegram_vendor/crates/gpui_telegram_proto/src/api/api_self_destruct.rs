//! Telegram Desktop 1:1 Parity Submodule: `ApiSelfDestruct`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_self_destruct.cpp, Telegram/SourceFiles/api/api_self_destruct.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiSelfDestruct {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub days_account_ttl: i32,
    pub default_history_ttl_seconds: i64,
    pub updated_at: i64,
}

impl Default for ApiSelfDestruct {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiSelfDestruct".to_string(),
            days_account_ttl: 180,
            default_history_ttl_seconds: 0,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiSelfDestruct {
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

    pub fn update_account_ttl(&mut self, days: i32) {
        self.days_account_ttl = days;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn update_default_history_ttl(&mut self, seconds: i64) {
        self.default_history_ttl_seconds = seconds;
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
