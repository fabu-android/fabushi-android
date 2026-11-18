//! Telegram Desktop 1:1 Parity Module: `TestQueuedDrag`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_queued_drag.h, Telegram/SourceFiles/test/test_queued_drag.cpp

use serde::{Deserialize, Serialize};

/// Queued drag & drop state machine test fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestQueuedDrag {
    pub mime_types: Vec<String>,
    pub is_dragging: bool,
    pub is_cancelled: bool,
}

impl TestQueuedDrag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start_drag(&mut self, mimes: Vec<String>) {
        self.mime_types = mimes;
        self.is_dragging = true;
        self.is_cancelled = false;
    }

    pub fn cancel_drag(&mut self) {
        self.is_dragging = false;
        self.is_cancelled = true;
    }
}
