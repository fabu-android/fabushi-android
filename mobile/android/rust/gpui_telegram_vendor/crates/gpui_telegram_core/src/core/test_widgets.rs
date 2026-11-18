//! Telegram Desktop 1:1 Parity Module: `TestWidgets`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_widgets.h, Telegram/SourceFiles/test/test_widgets.cpp

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestWidgetNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub is_focused: bool,
}

/// Widget hierarchy and focus chain inspection fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestWidgets {
    pub nodes: Vec<TestWidgetNode>,
}

impl TestWidgets {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_widget(&mut self, id: impl Into<String>, parent: Option<String>) {
        self.nodes.push(TestWidgetNode {
            id: id.into(),
            parent_id: parent,
            is_focused: false,
        });
    }

    pub fn set_focus(&mut self, id: &str) {
        for node in &mut self.nodes {
            node.is_focused = node.id == id;
        }
    }
}
