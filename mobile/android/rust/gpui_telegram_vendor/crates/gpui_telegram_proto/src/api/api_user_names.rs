//! Telegram Desktop 1:1 Parity Submodule: `ApiUserNames`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_user_names.cpp, Telegram/SourceFiles/api/api_user_names.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UsernameError {
    TooMuch,
    Flood,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct UsernameEntry {
    pub username: String,
    pub is_active: bool,
    pub is_editable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiUserNames {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub peer_usernames: HashMap<u64, Vec<UsernameEntry>>,
    pub updated_at: i64,
}

impl Default for ApiUserNames {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiUserNames".to_string(),
            peer_usernames: HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiUserNames {
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

    pub fn set_usernames(&mut self, peer_id: u64, entries: Vec<UsernameEntry>) {
        self.peer_usernames.insert(peer_id, entries);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn toggle(&mut self, peer_id: u64, username: &str, active: bool) -> Result<(), UsernameError> {
        if let Some(list) = self.peer_usernames.get_mut(&peer_id) {
            if let Some(entry) = list.iter_mut().find(|u| u.username == username) {
                entry.is_active = active;
                self.updated_at = chrono::Utc::now().timestamp();
                Ok(())
            } else {
                Err(UsernameError::Unknown)
            }
        } else {
            Err(UsernameError::Unknown)
        }
    }

    pub fn reorder(&mut self, peer_id: u64, ordered_usernames: &[String]) {
        if let Some(list) = self.peer_usernames.get_mut(&peer_id) {
            list.sort_by_key(|entry| {
                ordered_usernames
                    .iter()
                    .position(|u| u == &entry.username)
                    .unwrap_or(usize::MAX)
            });
            self.updated_at = chrono::Utc::now().timestamp();
        }
    }
}
