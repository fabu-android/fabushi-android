//! Telegram Desktop 1:1 Parity Submodule: `ApiFilterUpdates`
//! Subsystem: `api` | Feature ID: `TCG-N37`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_filter_updates.h

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateTypeFilter {
    All,
    MessagesOnly,
    UserStatusOnly,
    ChatParticipantsOnly,
    ReactionsOnly,
    PinnedOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiFilterUpdates {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub filter: UpdateTypeFilter,
    pub filtered_count: u64,
    pub updated_at: i64,
}

impl Default for ApiFilterUpdates {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiFilterUpdates".to_string(),
            filter: UpdateTypeFilter::All,
            filtered_count: 0,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiFilterUpdates {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_filter(filter: UpdateTypeFilter) -> Self {
        Self {
            filter,
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

    pub fn set_filter(&mut self, filter: UpdateTypeFilter) {
        self.filter = filter;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn matches_type(&self, update_tag: &str) -> bool {
        match self.filter {
            UpdateTypeFilter::All => true,
            UpdateTypeFilter::MessagesOnly => update_tag.contains("Message") || update_tag.contains("Edit"),
            UpdateTypeFilter::UserStatusOnly => update_tag.contains("UserStatus"),
            UpdateTypeFilter::ChatParticipantsOnly => update_tag.contains("Participant") || update_tag.contains("ChatAdmin"),
            UpdateTypeFilter::ReactionsOnly => update_tag.contains("Reaction"),
            UpdateTypeFilter::PinnedOnly => update_tag.contains("Pinned"),
        }
    }

    pub fn record_filtered(&mut self) {
        self.filtered_count += 1;
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
