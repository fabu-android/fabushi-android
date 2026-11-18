//! Telegram Desktop 1:1 Parity Submodule: `ApiRichTasks`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_rich_tasks.cpp, Telegram/SourceFiles/api/api_rich_tasks.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RichTaskItem {
    pub item_id: i32,
    pub title: String,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiRichTasks {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub tasks_by_msg: HashMap<(u64, i64), Vec<RichTaskItem>>,
    pub updated_at: i64,
}

impl Default for ApiRichTasks {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiRichTasks".to_string(),
            tasks_by_msg: HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiRichTasks {
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

    pub fn toggling_allowed(&self, _peer_id: u64, _msg_id: i64) -> bool {
        true
    }

    pub fn toggle_task(&mut self, peer_id: u64, msg_id: i64, item_id: i32, completed: bool) -> bool {
        if let Some(items) = self.tasks_by_msg.get_mut(&(peer_id, msg_id)) {
            if let Some(task) = items.iter_mut().find(|t| t.item_id == item_id) {
                task.completed = completed;
                self.updated_at = chrono::Utc::now().timestamp();
                return true;
            }
        }
        false
    }
}
