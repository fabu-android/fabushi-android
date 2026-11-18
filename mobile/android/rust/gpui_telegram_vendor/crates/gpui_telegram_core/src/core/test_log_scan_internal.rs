//! Telegram Desktop 1:1 Parity Module: `TestLogScanInternal`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_log_scan_internal.h

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestLogMarker {
    Pass(String),
    Fail(String),
    Note(String),
    Completed,
}

/// Scanner for parsing and verifying evidence log files emitted by test runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestLogScanInternal {
    pub passed_count: usize,
    pub failed_count: usize,
    pub failure_messages: Vec<String>,
    pub completed: bool,
}

impl TestLogScanInternal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scan_line(&mut self, line: &str) -> Option<TestLogMarker> {
        let trimmed = line.trim();
        if let Some(msg) = trimmed.strip_prefix("TEST_RESULT: PASS ") {
            self.passed_count += 1;
            Some(TestLogMarker::Pass(msg.to_string()))
        } else if let Some(msg) = trimmed.strip_prefix("TEST_RESULT: FAIL ") {
            self.failed_count += 1;
            self.failure_messages.push(msg.to_string());
            Some(TestLogMarker::Fail(msg.to_string()))
        } else if let Some(msg) = trimmed.strip_prefix("NOTE: ") {
            Some(TestLogMarker::Note(msg.to_string()))
        } else if trimmed == "COMPLETED: ALL" {
            self.completed = true;
            Some(TestLogMarker::Completed)
        } else {
            None
        }
    }
}
