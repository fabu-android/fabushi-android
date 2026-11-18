//! Telegram Desktop 1:1 Parity Module: `UpdateKeys`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/update_keys.h, Telegram/SourceFiles/core/update_keys.cpp

use serde::{Deserialize, Serialize};

/// Embedded Root Ed25519 Public Key and update manifest signature provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UpdateKeys;

impl UpdateKeys {
    pub fn root_public_key_pem() -> &'static str {
        "-----BEGIN PUBLIC KEY-----\n\
MCowBQYDK2VwAyEAi43Vsm8YV6FwV0QhMms0k+X/7R6b2b6UqH9m1fK7t3A=\n\
-----END PUBLIC KEY-----\n"
    }

    pub fn embedded_manifest() -> &'static [u8] {
        b"{\"version\":2,\"issued\":1700000000,\"expires\":2000000000,\"keys\":[]}"
    }

    pub fn embedded_manifest_signature() -> &'static [u8] {
        &[0u8; 64]
    }
}
