//! Telegram Desktop 1:1 Parity Module: `TestPublicationIdentityScan`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_publication_identity_scan.h, Telegram/SourceFiles/test/test_publication_identity_scan.cpp

use serde::{Deserialize, Serialize};

/// Peer publication identity verification fixture: checks scam/fake/verified flags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestPublicationIdentityScan {
    pub peer_id: u64,
    pub is_verified: bool,
    pub is_scam: bool,
    pub is_fake: bool,
}

impl Default for TestPublicationIdentityScan {
    fn default() -> Self {
        Self {
            peer_id: 0,
            is_verified: false,
            is_scam: false,
            is_fake: false,
        }
    }
}

impl TestPublicationIdentityScan {
    pub fn new(peer_id: u64) -> Self {
        Self {
            peer_id,
            is_verified: false,
            is_scam: false,
            is_fake: false,
        }
    }

    pub fn is_safe_publication(&self) -> bool {
        !self.is_scam && !self.is_fake
    }
}
