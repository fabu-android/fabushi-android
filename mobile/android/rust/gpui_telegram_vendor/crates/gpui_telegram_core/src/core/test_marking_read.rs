//! Telegram Desktop 1:1 Parity Module: `TestMarkingRead`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_marking_read.h, Telegram/SourceFiles/test/test_marking_read.cpp

use serde::{Deserialize, Serialize};

/// Test verification fixture for chat read receipts and inbox unread count decrement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestMarkingRead {
    pub peer_id: u64,
    pub max_read_inbox_id: u64,
    pub unread_count: i32,
}

impl Default for TestMarkingRead {
    fn default() -> Self {
        Self {
            peer_id: 0,
            max_read_inbox_id: 0,
            unread_count: 0,
        }
    }
}

impl TestMarkingRead {
    pub fn new(peer_id: u64, initial_unread: i32) -> Self {
        Self {
            peer_id,
            max_read_inbox_id: 0,
            unread_count: initial_unread.max(0),
        }
    }

    pub fn mark_read_up_to(&mut self, message_id: u64, read_items_count: i32) {
        if message_id > self.max_read_inbox_id {
            self.max_read_inbox_id = message_id;
            self.unread_count = (self.unread_count - read_items_count).max(0);
        }
    }

    pub fn is_fully_read(&self) -> bool {
        self.unread_count == 0
    }
}
