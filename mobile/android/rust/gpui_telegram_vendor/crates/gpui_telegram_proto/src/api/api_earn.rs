//! Telegram Desktop 1:1 Parity Submodule: `ApiEarn`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_earn.cpp, Telegram/SourceFiles/api/api_earn.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChannelEarnState {
    pub channel_id: u64,
    pub available_balance_nano_ton: i64,
    pub current_balance_nano_ton: i64,
    pub overall_revenue_nano_ton: i64,
    pub withdrawal_enabled: bool,
    pub restricted_sponsored: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiEarn {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub channel_states: std::collections::HashMap<u64, ChannelEarnState>,
    pub updated_at: i64,
}

impl Default for ApiEarn {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiEarn".to_string(),
            channel_states: std::collections::HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiEarn {
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

    pub fn update_state(&mut self, state: ChannelEarnState) {
        self.channel_states.insert(state.channel_id, state);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn restrict_sponsored(&mut self, channel_id: u64, restricted: bool) {
        if let Some(state) = self.channel_states.get_mut(&channel_id) {
            state.restricted_sponsored = restricted;
            self.updated_at = chrono::Utc::now().timestamp();
        }
    }
}
