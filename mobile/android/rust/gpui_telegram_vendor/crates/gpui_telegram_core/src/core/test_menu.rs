//! Telegram Desktop 1:1 Parity Module: `TestMenu`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_menu.h, Telegram/SourceFiles/test/test_menu.cpp

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestMenuItem {
    pub id: String,
    pub title: String,
    pub is_enabled: bool,
    pub is_separator: bool,
}

/// Simulated context menu tree for verifying menu item states and clicks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestMenu {
    pub items: Vec<TestMenuItem>,
    pub selected_item_id: Option<String>,
}

impl TestMenu {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_action(&mut self, id: impl Into<String>, title: impl Into<String>) {
        self.items.push(TestMenuItem {
            id: id.into(),
            title: title.into(),
            is_enabled: true,
            is_separator: false,
        });
    }

    pub fn add_separator(&mut self) {
        self.items.push(TestMenuItem {
            id: String::new(),
            title: String::new(),
            is_enabled: false,
            is_separator: true,
        });
    }

    pub fn trigger(&mut self, item_id: &str) -> bool {
        if let Some(item) = self.items.iter().find(|i| i.id == item_id) {
            if item.is_enabled {
                self.selected_item_id = Some(item_id.to_string());
                return true;
            }
        }
        false
    }
}
