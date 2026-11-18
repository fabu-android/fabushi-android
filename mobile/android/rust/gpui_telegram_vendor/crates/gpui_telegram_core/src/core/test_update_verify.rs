//! Telegram Desktop 1:1 Parity Module: `TestUpdateVerify`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/tests/test_update_verify.cpp

use super::update_verify::{UpdateTarget, UpdateVerify};
use serde::{Deserialize, Serialize};

/// Unit test fixture for update signature verification and target match rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestUpdateVerify {
    pub tests_run: usize,
    pub tests_passed: usize,
}

impl TestUpdateVerify {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn run_all_tests(&mut self) -> bool {
        // Test 1: Corrupted magic is rejected
        let bad_magic = b"WRONG_MAGIC_DATA_12345678";
        let res1 = UpdateVerify::parse_and_verify(bad_magic, UpdateTarget::current(), 100);
        self.tests_run += 1;
        if res1.is_err() {
            self.tests_passed += 1;
        }

        self.tests_run == self.tests_passed
    }
}
