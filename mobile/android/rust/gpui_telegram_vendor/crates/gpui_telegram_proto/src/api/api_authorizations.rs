//! Telegram Desktop 1:1 Parity Submodule: `ApiAuthorizations`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_authorizations.cpp, Telegram/SourceFiles/api/api_authorizations.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AuthorizationEntry {
    pub hash: u64,
    pub incomplete: bool,
    pub calls_disabled: bool,
    pub api_id: i32,
    pub active_time: i64,
    pub app_name: String,
    pub app_version: String,
    pub ip: String,
    pub country: String,
    pub region: String,
    pub system_version: String,
    pub platform: String,
    pub device_model: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiAuthorizations {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub sessions: Vec<AuthorizationEntry>,
    pub ttl_days: i32,
    pub calls_disabled_here: bool,
    pub updated_at: i64,
}

impl Default for ApiAuthorizations {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiAuthorizations".to_string(),
            sessions: Vec::new(),
            ttl_days: 180,
            calls_disabled_here: false,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiAuthorizations {
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

    pub fn set_sessions(&mut self, sessions: Vec<AuthorizationEntry>) {
        self.sessions = sessions;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn terminate(&mut self, hash: u64) -> bool {
        let initial = self.sessions.len();
        self.sessions.retain(|s| s.hash != hash);
        if self.sessions.len() < initial {
            self.updated_at = chrono::Utc::now().timestamp();
            true
        } else {
            false
        }
    }

    pub fn terminate_all_others(&mut self, current_hash: u64) -> usize {
        let initial = self.sessions.len();
        self.sessions.retain(|s| s.hash == current_hash);
        let removed = initial - self.sessions.len();
        self.updated_at = chrono::Utc::now().timestamp();
        removed
    }

    pub fn update_ttl(&mut self, days: i32) {
        self.ttl_days = days;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn toggle_calls_disabled(&mut self, hash: u64, disabled: bool) {
        if hash == 0 {
            self.calls_disabled_here = disabled;
        } else if let Some(s) = self.sessions.iter_mut().find(|s| s.hash == hash) {
            s.calls_disabled = disabled;
        }
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
