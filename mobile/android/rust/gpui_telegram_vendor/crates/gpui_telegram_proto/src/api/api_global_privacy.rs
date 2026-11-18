//! Telegram Desktop 1:1 Parity Submodule: `ApiGlobalPrivacy`
//! Subsystem: `api` | Feature ID: `TCG-N33`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_global_privacy.cpp, Telegram/SourceFiles/api/api_global_privacy.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UnarchiveOnNewMessage {
    #[default]
    None,
    NotInFoldersUnmuted,
    AnyUnmuted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GlobalPrivacySettings {
    pub archive_and_mute_new_non_contacts: bool,
    pub unarchive_on_new_message: UnarchiveOnNewMessage,
    pub hide_read_time: bool,
    pub new_require_premium: bool,
    pub new_charge_stars: i32,
    pub paid_reaction_shown_peer: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiGlobalPrivacy {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub settings: GlobalPrivacySettings,
    pub updated_at: i64,
}

impl Default for ApiGlobalPrivacy {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiGlobalPrivacy".to_string(),
            settings: GlobalPrivacySettings::default(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiGlobalPrivacy {
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

    pub fn update_archive_and_mute(&mut self, archive_and_mute: bool) {
        self.settings.archive_and_mute_new_non_contacts = archive_and_mute;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn update_unarchive(&mut self, policy: UnarchiveOnNewMessage) {
        self.settings.unarchive_on_new_message = policy;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn update_hide_read_time(&mut self, hide: bool) {
        self.settings.hide_read_time = hide;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn update_messages_privacy(&mut self, require_premium: bool, charge_stars: i32) {
        self.settings.new_require_premium = require_premium;
        self.settings.new_charge_stars = charge_stars;
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
