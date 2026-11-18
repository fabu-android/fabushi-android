//! Telegram Desktop 1:1 Parity Module: `TestOpenHandoff`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_open_handoff.h, Telegram/SourceFiles/test/test_open_handoff.cpp

use serde::{Deserialize, Serialize};

/// Test fixture verifying URL and file opening handoff to external applications.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestOpenHandoff {
    pub last_opened_url: Option<String>,
    pub last_opened_path: Option<String>,
    pub handoff_succeeded: bool,
}

impl TestOpenHandoff {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_url_open(&mut self, url: impl Into<String>) {
        self.last_opened_url = Some(url.into());
        self.handoff_succeeded = true;
    }

    pub fn record_path_open(&mut self, path: impl Into<String>) {
        self.last_opened_path = Some(path.into());
        self.handoff_succeeded = true;
    }
}
