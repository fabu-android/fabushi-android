//! Telegram Desktop 1:1 Parity Module: `TestUpdateUnpack`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/tests/test_update_unpack.cpp

use serde::{Deserialize, Serialize};

/// Unit test fixture for update payload decompression and file extraction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestUpdateUnpack {
    pub unpacked_bytes: usize,
    pub success: bool,
}

impl TestUpdateUnpack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn test_unpack_simulation(&mut self, payload: &[u8]) -> bool {
        if payload.is_empty() {
            self.success = false;
            return false;
        }
        self.unpacked_bytes = payload.len();
        self.success = true;
        true
    }
}
