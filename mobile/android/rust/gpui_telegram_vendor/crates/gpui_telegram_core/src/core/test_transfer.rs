//! Telegram Desktop 1:1 Parity Module: `TestTransfer`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_transfer.h, Telegram/SourceFiles/test/test_transfer.cpp

use serde::{Deserialize, Serialize};

/// File upload / download transfer verification fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestTransfer {
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub is_finished: bool,
}

impl Default for TestTransfer {
    fn default() -> Self {
        Self {
            total_bytes: 0,
            transferred_bytes: 0,
            is_finished: false,
        }
    }
}

impl TestTransfer {
    pub fn new(total_bytes: u64) -> Self {
        Self {
            total_bytes,
            transferred_bytes: 0,
            is_finished: false,
        }
    }

    pub fn update_progress(&mut self, bytes: u64) {
        self.transferred_bytes = bytes.min(self.total_bytes);
        if self.transferred_bytes >= self.total_bytes && self.total_bytes > 0 {
            self.is_finished = true;
        }
    }

    pub fn progress_ratio(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.transferred_bytes as f64) / (self.total_bytes as f64)
        }
    }
}
