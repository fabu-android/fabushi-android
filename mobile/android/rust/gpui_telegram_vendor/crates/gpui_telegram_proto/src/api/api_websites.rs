//! Telegram Desktop 1:1 Parity Submodule: `ApiWebsites`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_websites.cpp, Telegram/SourceFiles/api/api_websites.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WebsiteEntry {
    pub hash: u64,
    pub bot_id: u64,
    pub active_time: i64,
    pub domain: String,
    pub browser: String,
    pub platform: String,
    pub ip: String,
    pub location: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiWebsites {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub websites: Vec<WebsiteEntry>,
    pub updated_at: i64,
}

impl Default for ApiWebsites {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiWebsites".to_string(),
            websites: Vec::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiWebsites {
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

    pub fn set_websites(&mut self, websites: Vec<WebsiteEntry>) {
        self.websites = websites;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn terminate(&mut self, hash: u64) -> bool {
        let initial = self.websites.len();
        self.websites.retain(|w| w.hash != hash);
        if self.websites.len() < initial {
            self.updated_at = chrono::Utc::now().timestamp();
            true
        } else {
            false
        }
    }
}
