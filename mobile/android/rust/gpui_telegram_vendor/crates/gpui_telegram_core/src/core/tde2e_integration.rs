//! Telegram Desktop 1:1 Parity Module: `Tde2eIntegration`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/tde2e/tde2e_integration.h, Telegram/SourceFiles/tde2e/tde2e_integration.cpp

use super::tde2e_api::{Tde2ePublicKey, Tde2eUserId};
use serde::{Deserialize, Serialize};

/// Integration bridge translating domain User IDs and public keys into MTProto format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Tde2eIntegration;

impl Tde2eIntegration {
    pub fn make_user_id(raw_id: u64) -> Tde2eUserId {
        Tde2eUserId { v: raw_id }
    }

    pub fn public_key_to_bytes(key: &Tde2ePublicKey) -> [u8; 32] {
        key.to_bytes()
    }

    pub fn public_key_from_bytes(bytes: &[u8; 32]) -> Tde2ePublicKey {
        Tde2ePublicKey::from_bytes(bytes)
    }
}
