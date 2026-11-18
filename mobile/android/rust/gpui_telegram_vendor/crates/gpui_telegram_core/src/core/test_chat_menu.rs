//! Telegram Test Harness: Chat Context Menu Actions & Routing
//! Upstream sources: `Telegram/SourceFiles/test/test_chat_menu.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuItem {
    pub id: u32,
    pub title: String,
    pub is_destructive: bool,
}

/// Test fixture verifying chat context menu composition and action trigger callbacks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestChatMenu {
    pub items: Vec<MenuItem>,
    pub selected_item_id: Option<u32>,
    pub is_open: bool,
}

impl TestChatMenu {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_item(&mut self, id: u32, title: impl Into<String>, is_destructive: bool) {
        self.items.push(MenuItem {
            id,
            title: title.into(),
            is_destructive,
        });
    }

    pub fn open(&mut self) {
        self.is_open = true;
    }

    pub fn select_item(&mut self, id: u32) -> bool {
        if self.items.iter().any(|i| i.id == id) {
            self.selected_item_id = Some(id);
            self.is_open = false;
            true
        } else {
            false
        }
    }
}
