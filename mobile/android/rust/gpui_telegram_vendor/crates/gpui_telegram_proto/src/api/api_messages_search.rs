//! Telegram Desktop 1:1 Parity Submodule: `ApiMessagesSearch`
//! Subsystem: `api` | Feature ID: `TCG-N21`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_messages_search.cpp, Telegram/SourceFiles/api/api_messages_search.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SearchFilter {
    #[default]
    NoFilter,
    Pinned,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MessagesSearchRequest {
    pub query: String,
    pub from_peer_id: Option<u64>,
    pub reaction_tags: Vec<String>,
    pub top_msg_id: i64,
    pub filter: SearchFilter,
    pub offset_id: i64,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FoundMessages {
    pub total: i32,
    pub message_ids: Vec<i64>,
    pub next_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiMessagesSearch {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub current_request: Option<MessagesSearchRequest>,
    pub last_result: Option<FoundMessages>,
    pub updated_at: i64,
}

impl Default for ApiMessagesSearch {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiMessagesSearch".to_string(),
            current_request: None,
            last_result: None,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiMessagesSearch {
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

    pub fn search(&mut self, req: MessagesSearchRequest) {
        self.current_request = Some(req);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn set_results(&mut self, found: FoundMessages) {
        self.last_result = Some(found);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn has_more(&self) -> bool {
        self.last_result.as_ref().map(|r| r.next_token.is_some()).unwrap_or(false)
    }
}
