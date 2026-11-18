//! Telegram Desktop 1:1 Parity Submodule: `ApiMessagesSearchMerged`
//! Subsystem: `api` | Feature ID: `TCG-N21`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_messages_search_merged.cpp, Telegram/SourceFiles/api/api_messages_search_merged.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use crate::api::api_messages_search::{FoundMessages, MessagesSearchRequest};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiMessagesSearchMerged {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub primary_history_id: u64,
    pub migrated_history_id: Option<u64>,
    pub current_request: Option<MessagesSearchRequest>,
    pub merged_results: FoundMessages,
    pub migrated_enabled: bool,
    pub updated_at: i64,
}

impl Default for ApiMessagesSearchMerged {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiMessagesSearchMerged".to_string(),
            primary_history_id: 0,
            migrated_history_id: None,
            current_request: None,
            merged_results: FoundMessages::default(),
            migrated_enabled: true,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiMessagesSearchMerged {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_histories(primary_id: u64, migrated_id: Option<u64>) -> Self {
        Self {
            primary_history_id: primary_id,
            migrated_history_id: migrated_id,
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

    pub fn disable_migrated(&mut self) {
        self.migrated_enabled = false;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn search(&mut self, req: MessagesSearchRequest) {
        self.current_request = Some(req);
        self.merged_results = FoundMessages::default();
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn append_results(&mut self, results: FoundMessages) {
        self.merged_results.message_ids.extend(results.message_ids);
        if results.total >= 0 {
            self.merged_results.total = results.total;
        }
        self.merged_results.next_token = results.next_token;
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
