//! Telegram Desktop 1:1 Parity Submodule: `ApiCloudPassword`
//! Subsystem: `api` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_cloud_password.cpp, Telegram/SourceFiles/api/api_cloud_password.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CloudPasswordState {
    pub has_password: bool,
    pub has_recovery: bool,
    pub hint: Option<String>,
    pub email_unconfirmed_pattern: Option<String>,
    pub reset_retry_date: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiCloudPassword {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub state: CloudPasswordState,
    pub updated_at: i64,
}

impl Default for ApiCloudPassword {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiCloudPassword".to_string(),
            state: CloudPasswordState::default(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiCloudPassword {
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

    pub fn set_password(&mut self, _new_password: &str, hint: Option<String>, recovery_email: Option<String>) {
        self.state.has_password = true;
        self.state.hint = hint;
        self.state.has_recovery = recovery_email.is_some();
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn clear_password(&mut self) {
        self.state = CloudPasswordState::default();
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn start_reset(&mut self, wait_seconds: i64) {
        self.state.reset_retry_date = Some(chrono::Utc::now().timestamp() + wait_seconds);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn cancel_reset(&mut self) {
        self.state.reset_retry_date = None;
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
