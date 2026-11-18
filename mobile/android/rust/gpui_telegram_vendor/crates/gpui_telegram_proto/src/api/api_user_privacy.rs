//! Telegram Desktop 1:1 Parity Submodule: `ApiUserPrivacy`
//! Subsystem: `api` | Feature ID: `TCG-N33`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_user_privacy.cpp, Telegram/SourceFiles/api/api_user_privacy.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrivacyKey {
    PhoneNumber,
    AddedByPhone,
    LastSeen,
    Calls,
    Invites,
    CallsPeer2Peer,
    Forwards,
    ProfilePhoto,
    Voices,
    About,
    Birthday,
    GiftsAutoSave,
    NoPaidMessages,
    SavedMusic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PrivacyOption {
    #[default]
    Everyone,
    Contacts,
    CloseFriends,
    Nobody,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PrivacyExceptions {
    pub peers: Vec<u64>,
    pub premiums: bool,
    pub miniapps: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PrivacyRule {
    pub option: PrivacyOption,
    pub always: PrivacyExceptions,
    pub never: PrivacyExceptions,
    pub ignore_always: bool,
    pub ignore_never: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiUserPrivacy {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub rules: HashMap<PrivacyKey, PrivacyRule>,
    pub updated_at: i64,
}

impl Default for ApiUserPrivacy {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiUserPrivacy".to_string(),
            rules: HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiUserPrivacy {
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

    pub fn get_rule(&self, key: PrivacyKey) -> PrivacyRule {
        self.rules.get(&key).cloned().unwrap_or_default()
    }

    pub fn set_rule(&mut self, key: PrivacyKey, rule: PrivacyRule) {
        self.rules.insert(key, rule);
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
