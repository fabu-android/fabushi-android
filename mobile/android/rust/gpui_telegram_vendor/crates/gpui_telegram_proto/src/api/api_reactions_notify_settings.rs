//! Telegram Desktop 1:1 Parity Submodule: `ApiReactionsNotifySettings`
//! Subsystem: `api` | Feature ID: `TCG-N14`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_reactions_notify_settings.cpp, Telegram/SourceFiles/api/api_reactions_notify_settings.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReactionsNotifyFrom {
    None,
    Contacts,
    #[default]
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiReactionsNotifySettings {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub messages_from: ReactionsNotifyFrom,
    pub poll_votes_from: ReactionsNotifyFrom,
    pub stories_from: ReactionsNotifyFrom,
    pub show_previews: bool,
    pub updated_at: i64,
}

impl Default for ApiReactionsNotifySettings {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiReactionsNotifySettings".to_string(),
            messages_from: ReactionsNotifyFrom::All,
            poll_votes_from: ReactionsNotifyFrom::All,
            stories_from: ReactionsNotifyFrom::All,
            show_previews: true,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiReactionsNotifySettings {
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

    pub fn update_messages_from(&mut self, from: ReactionsNotifyFrom) {
        self.messages_from = from;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn update_show_previews(&mut self, show: bool) {
        self.show_previews = show;
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
