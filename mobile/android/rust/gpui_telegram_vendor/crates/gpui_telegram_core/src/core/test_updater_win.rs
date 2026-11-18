//! Telegram Desktop 1:1 Parity Module: `TestUpdaterWin`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/tests/test_updater_win.cpp

use super::updater::Updater;
use serde::{Deserialize, Serialize};

/// Unit test fixture for Windows updater argument parsing and process spawning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestUpdaterWin {
    pub passed: bool,
}

impl TestUpdaterWin {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn run_test(&mut self) -> bool {
        let parsed = Updater::parse_update_process("12345", "987654321");
        self.passed = parsed == Some((12345, 987654321));
        self.passed
    }
}
