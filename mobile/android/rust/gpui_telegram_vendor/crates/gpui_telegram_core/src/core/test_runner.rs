//! Telegram Desktop 1:1 Parity Module: `TestRunner`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_runner.h, Telegram/SourceFiles/test/test_runner.cpp

use serde::{Deserialize, Serialize};

/// Runner orchestrating sequential asynchronous or synchronous test stages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestRunner {
    pub current_stage: usize,
    pub total_stages: usize,
    pub log_output: Vec<String>,
    pub failed: bool,
}

impl TestRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn log(&mut self, text: impl Into<String>) {
        self.log_output.push(text.into());
    }

    pub fn check(&mut self, condition: bool, description: &str) {
        if condition {
            self.log(format!("TEST_RESULT: PASS {}", description));
        } else {
            self.failed = true;
            self.log(format!("TEST_RESULT: FAIL {}", description));
        }
    }

    pub fn advance_stage(&mut self) {
        self.current_stage += 1;
    }
}
