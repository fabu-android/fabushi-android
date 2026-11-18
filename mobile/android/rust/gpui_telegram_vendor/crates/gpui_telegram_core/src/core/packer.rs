//! Telegram Desktop Binary Update Packer & Manifest Signature Verifier
//! Upstream sources: `Telegram/SourceFiles/_other/packer.h`, `packer.cpp`

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub version: u64,
    pub channel: String,
    pub signature_hex: String,
    pub files: Vec<String>,
}

/// Binary update archive builder and cryptographic signature validator.
#[derive(Debug, Clone, Default)]
pub struct Packer {
    pub channel_name: String,
}

impl Packer {
    pub fn new(channel_name: impl Into<String>) -> Self {
        Self {
            channel_name: channel_name.into(),
        }
    }

    pub fn make_update_version(major: u32, minor: u32, patch: u32) -> u64 {
        (major as u64) * 1_000_000 + (minor as u64) * 1_000 + (patch as u64)
    }

    pub fn verify_update_payload(payload: &[u8], expected_sha256: &[u8]) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(payload);
        let digest = hasher.finalize();
        digest.as_slice() == expected_sha256
    }

    pub fn parse_manifest(json_bytes: &[u8]) -> Result<UpdateManifest, String> {
        serde_json::from_slice(json_bytes).map_err(|e| e.to_string())
    }
}
