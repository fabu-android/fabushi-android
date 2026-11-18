//! Telegram Desktop 1:1 Parity Module: `TestMessages`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_messages.h, Telegram/SourceFiles/test/test_messages.cpp

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestMessageItem {
    pub message_id: u64,
    pub sender_id: u64,
    pub text: String,
    pub date: i64,
    pub is_outgoing: bool,
}

/// Simulated messages fixture for checking history layout, bubbled geometry, and dates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestMessages {
    pub items: Vec<TestMessageItem>,
}

impl TestMessages {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn append(&mut self, id: u64, sender: u64, text: impl Into<String>, outgoing: bool) {
        self.items.push(TestMessageItem {
            message_id: id,
            sender_id: sender,
            text: text.into(),
            date: chrono::Utc::now().timestamp(),
            is_outgoing: outgoing,
        });
    }

    pub fn count(&self) -> usize {
        self.items.len()
    }

    pub fn find_by_id(&self, id: u64) -> Option<&TestMessageItem> {
        self.items.iter().find(|m| m.message_id == id)
    }
}
