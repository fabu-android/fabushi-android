//! Telegram Desktop 1:1 Parity Module: `TestWalletIntercept`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_wallet_intercept.h, Telegram/SourceFiles/test/test_wallet_intercept.cpp

use serde::{Deserialize, Serialize};

/// TonConnect URL interception and internal wallet routing fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestWalletIntercept {
    pub intercepted_queries: Vec<String>,
}

impl TestWalletIntercept {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_intercept(&mut self, url: &str) -> bool {
        if url.starts_with("tc://") || url.starts_with("tonsite://") {
            self.intercepted_queries.push(url.to_string());
            true
        } else {
            false
        }
    }
}
