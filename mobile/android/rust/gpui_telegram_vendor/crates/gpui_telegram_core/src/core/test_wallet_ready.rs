//! Telegram Desktop 1:1 Parity Module: `TestWalletReady`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_wallet_ready.h, Telegram/SourceFiles/test/test_wallet_ready.cpp

use serde::{Deserialize, Serialize};

/// Embedded TON wallet readiness and key derivation fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestWalletReady {
    pub has_passcode: bool,
    pub has_seed_phrase: bool,
    pub is_connected_to_mainnet: bool,
}

impl TestWalletReady {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_ready(&self) -> bool {
        self.has_passcode && self.has_seed_phrase && self.is_connected_to_mainnet
    }
}
