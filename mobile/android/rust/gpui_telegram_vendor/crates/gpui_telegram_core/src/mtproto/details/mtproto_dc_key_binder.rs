//! Datacenter Key Binding Message Generator
//! Upstream Source: `Telegram/SourceFiles/mtproto/details/mtproto_dc_key_binder.h`, `mtproto_dc_key_binder.cpp`

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoDcKeyBinderDetails {
    pub perm_auth_key_id: u64,
    pub temp_auth_key_id: u64,
    pub nonce: i64,
    pub expires_at: i32,
}

impl Default for MtprotoDcKeyBinderDetails {
    fn default() -> Self {
        Self {
            perm_auth_key_id: 0,
            temp_auth_key_id: 0,
            nonce: 0,
            expires_at: 0,
        }
    }
}

impl MtprotoDcKeyBinderDetails {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.perm_auth_key_id != 0 && self.temp_auth_key_id != 0
    }

    pub fn new_binding(perm_key_id: u64, temp_key_id: u64, nonce: i64, expires_at: i32) -> Self {
        Self {
            perm_auth_key_id: perm_key_id,
            temp_auth_key_id: temp_key_id,
            nonce,
            expires_at,
        }
    }

    pub fn compute_binding_hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(&self.perm_auth_key_id.to_le_bytes());
        hasher.update(&self.temp_auth_key_id.to_le_bytes());
        hasher.update(&self.nonce.to_le_bytes());
        hasher.update(&self.expires_at.to_le_bytes());
        let hash = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&hash);
        out
    }
}
