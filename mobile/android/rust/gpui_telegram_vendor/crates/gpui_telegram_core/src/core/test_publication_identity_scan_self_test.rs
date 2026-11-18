//! Telegram Desktop 1:1 Parity Module: `TestPublicationIdentityScanSelfTest`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_publication_identity_scan_self_test.cpp

use super::test_publication_identity_scan::TestPublicationIdentityScan;
use serde::{Deserialize, Serialize};

/// Self-test driver verifying publication identity scanning logic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestPublicationIdentityScanSelfTest {
    pub passed: bool,
}

impl TestPublicationIdentityScanSelfTest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn run(&mut self) -> bool {
        let valid = TestPublicationIdentityScan {
            peer_id: 1001,
            is_verified: true,
            is_scam: false,
            is_fake: false,
        };
        let scam = TestPublicationIdentityScan {
            peer_id: 1002,
            is_verified: false,
            is_scam: true,
            is_fake: false,
        };

        self.passed = valid.is_safe_publication() && !scam.is_safe_publication();
        self.passed
    }
}
