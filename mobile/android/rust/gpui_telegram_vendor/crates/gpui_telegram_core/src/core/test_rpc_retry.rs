//! Telegram Desktop 1:1 Parity Module: `TestRpcRetry`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_rpc_retry.h, Telegram/SourceFiles/test/test_rpc_retry.cpp

use serde::{Deserialize, Serialize};

/// Test fixture simulating exponential backoff and FLOOD_WAIT retry scheduling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestRpcRetry {
    pub max_attempts: u32,
    pub current_attempts: u32,
    pub backoff_delay_ms: u64,
}

impl Default for TestRpcRetry {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            current_attempts: 0,
            backoff_delay_ms: 500,
        }
    }
}

impl TestRpcRetry {
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            current_attempts: 0,
            backoff_delay_ms: 500,
        }
    }

    pub fn record_failure(&mut self) -> bool {
        self.current_attempts += 1;
        if self.current_attempts < self.max_attempts {
            self.backoff_delay_ms = (self.backoff_delay_ms * 2).min(30_000);
            true
        } else {
            false
        }
    }
}
