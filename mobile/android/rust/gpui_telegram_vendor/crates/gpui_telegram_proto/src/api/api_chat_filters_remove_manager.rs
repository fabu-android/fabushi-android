//! Telegram Desktop 1:1 Parity Submodule: `ApiChatFiltersRemoveManager`
//! Subsystem: `api` | Feature ID: `TCG-N37`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_chat_filters_remove_manager.cpp, Telegram/SourceFiles/api/api_chat_filters_remove_manager.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiChatFiltersRemoveManager {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub removing_filter_id: Option<i32>,
    pub leave_chats: bool,
    pub updated_at: i64,
}

impl Default for ApiChatFiltersRemoveManager {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiChatFiltersRemoveManager".to_string(),
            removing_filter_id: None,
            leave_chats: false,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiChatFiltersRemoveManager {
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

    pub fn request_removal(&mut self, filter_id: i32, leave_chats: bool) {
        self.removing_filter_id = Some(filter_id);
        self.leave_chats = leave_chats;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn complete_removal(&mut self) -> Option<i32> {
        let id = self.removing_filter_id.take();
        self.leave_chats = false;
        self.updated_at = chrono::Utc::now().timestamp();
        id
    }
}
