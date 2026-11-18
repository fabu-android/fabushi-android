//! Telegram Test Harness: In-Memory Assertion & Log Interceptor
//! Upstream sources: `Telegram/SourceFiles/test/test_log.cpp`

use serde::{Deserialize, Serialize};

/// Test fixture capturing assertions, warnings, and trace outputs during test runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestLog {
    pub messages: Vec<String>,
    pub assertion_failures: Vec<String>,
}

impl TestLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn log(&mut self, msg: impl Into<String>) {
        self.messages.push(msg.into());
    }

    pub fn record_assertion_failure(&mut self, err: impl Into<String>) {
        self.assertion_failures.push(err.into());
    }

    pub fn has_failures(&self) -> bool {
        !self.assertion_failures.is_empty()
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.assertion_failures.clear();
    }
}
