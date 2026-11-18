//! Telegram Desktop 1:1 Parity Module: `TestMain`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/tests/test_main.h, Telegram/SourceFiles/tests/test_main.cpp

use serde::{Deserialize, Serialize};

/// Main test suite driver and filter runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestMain {
    pub filter: Option<String>,
    pub run_count: usize,
    pub passed_count: usize,
    pub failed_count: usize,
}

impl Default for TestMain {
    fn default() -> Self {
        Self {
            filter: None,
            run_count: 0,
            passed_count: 0,
            failed_count: 0,
        }
    }
}

impl TestMain {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_filter(filter: impl Into<String>) -> Self {
        Self {
            filter: Some(filter.into()),
            ..Self::default()
        }
    }

    pub fn should_run_test(&self, test_name: &str) -> bool {
        match &self.filter {
            Some(f) => test_name.contains(f),
            None => true,
        }
    }

    pub fn record_result(&mut self, passed: bool) {
        self.run_count += 1;
        if passed {
            self.passed_count += 1;
        } else {
            self.failed_count += 1;
        }
    }

    pub fn is_all_passed(&self) -> bool {
        self.failed_count == 0 && self.run_count > 0
    }
}
