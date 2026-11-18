//! Telegram Desktop 1:1 Parity Submodule: `ApiTodoLists`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_todo_lists.cpp, Telegram/SourceFiles/api/api_todo_lists.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::cards::{TodoCard, TodoItem};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiTodoLists {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub active_todo_lists: HashMap<(u64, i64), TodoCard>,
    pub updated_at: i64,
}

impl Default for ApiTodoLists {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiTodoLists".to_string(),
            active_todo_lists: HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiTodoLists {
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

    pub fn register_todo(&mut self, peer_id: u64, msg_id: i64, card: TodoCard) {
        self.active_todo_lists.insert((peer_id, msg_id), card);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn toggle_completion(&mut self, peer_id: u64, msg_id: i64, item_id: &str, completed: bool) -> bool {
        if let Some(card) = self.active_todo_lists.get_mut(&(peer_id, msg_id)) {
            if let Some(item) = card.items.iter_mut().find(|i| i.id == item_id) {
                item.completed = completed;
                self.updated_at = chrono::Utc::now().timestamp();
                return true;
            }
        }
        false
    }

    pub fn add_item(&mut self, peer_id: u64, msg_id: i64, item: TodoItem) -> bool {
        if let Some(card) = self.active_todo_lists.get_mut(&(peer_id, msg_id)) {
            card.items.push(item);
            self.updated_at = chrono::Utc::now().timestamp();
            true
        } else {
            false
        }
    }
}
