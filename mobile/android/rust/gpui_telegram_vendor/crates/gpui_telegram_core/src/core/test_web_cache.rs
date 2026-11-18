//! Telegram Desktop 1:1 Parity Module: `TestWebCache`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_web_cache.h, Telegram/SourceFiles/test/test_web_cache.cpp

use serde::{Deserialize, Serialize};

/// Webview browser cache lifecycle and purge verification fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestWebCache {
    pub cached_entries_count: usize,
    pub cached_bytes_total: u64,
}

impl TestWebCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_entry(&mut self, bytes: u64) {
        self.cached_entries_count += 1;
        self.cached_bytes_total += bytes;
    }

    pub fn clear(&mut self) {
        self.cached_entries_count = 0;
        self.cached_bytes_total = 0;
    }
}
