//! Telegram Desktop 1:1 Parity Module: `TestSessionGate`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_session_gate.h, Telegram/SourceFiles/test/test_session_gate.cpp

use serde::{Deserialize, Serialize};

/// Session authorization gate fixture verifying auth requirements for routes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestSessionGate {
    pub is_authenticated: bool,
    pub user_id: Option<u64>,
}

impl TestSessionGate {
    pub fn new(auth: bool) -> Self {
        Self {
            is_authenticated: auth,
            user_id: if auth { Some(1) } else { None },
        }
    }

    pub fn check_access(&self, requires_auth: bool) -> bool {
        !requires_auth || self.is_authenticated
    }
}
