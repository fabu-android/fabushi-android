//! MTProto 2.0 AuthKey & AES-256-IGE Key Derivation
//! Upstream Source: `Telegram/SourceFiles/mtproto/mtproto_auth_key.h`, `mtproto_auth_key.cpp`

use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};

pub const AUTH_KEY_SIZE: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AuthKeyType {
    #[default]
    Generated,
    Temporary,
    ReadFromFile,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoAuthKey {
    pub key: Vec<u8>,
    pub key_id: u64,
    pub dc_id: i32,
    pub key_type: AuthKeyType,
    pub creation_time: i64,
    pub expires_at: i32,
}

impl Default for MtprotoAuthKey {
    fn default() -> Self {
        Self {
            key: vec![0u8; AUTH_KEY_SIZE],
            key_id: 0,
            dc_id: 2,
            key_type: AuthKeyType::Generated,
            creation_time: chrono::Utc::now().timestamp(),
            expires_at: 0,
        }
    }
}

impl MtprotoAuthKey {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        true
    }

    pub fn from_key_bytes(key_bytes: &[u8], dc_id: i32, key_type: AuthKeyType) -> Self {
        let mut key = vec![0u8; AUTH_KEY_SIZE];
        let copy_len = key_bytes.len().min(AUTH_KEY_SIZE);
        key[AUTH_KEY_SIZE - copy_len..].copy_from_slice(&key_bytes[..copy_len]);

        let key_id = Self::compute_key_id(&key);
        Self {
            key,
            key_id,
            dc_id,
            key_type,
            creation_time: chrono::Utc::now().timestamp(),
            expires_at: 0,
        }
    }

    pub fn compute_key_id(key_bytes: &[u8]) -> u64 {
        // Lower 64 bits = 8 bytes of 20-byte SHA-1 hash (tdesktop mtproto_auth_key.cpp:146)
        let mut hasher = Sha1::new();
        hasher.update(key_bytes);
        let hash = hasher.finalize();
        let mut id_bytes = [0u8; 8];
        id_bytes.copy_from_slice(&hash[12..20]);
        u64::from_le_bytes(id_bytes)
    }

    /// Derives AES-256 key (32 bytes) and IV (32 bytes) for MTProto 2.0
    /// send == true for client-to-server; send == false for server-to-client
    pub fn prepare_aes(&self, msg_key: &[u8; 16], send: bool) -> ([u8; 32], [u8; 32]) {
        let x = if send { 0 } else { 8 };

        // sha256_a = SHA256(msg_key + key[x .. x+36])
        let mut hasher_a = Sha256::new();
        hasher_a.update(msg_key);
        hasher_a.update(&self.key[x..x + 36]);
        let sha256_a = hasher_a.finalize();

        // sha256_b = SHA256(key[40+x .. 40+x+36] + msg_key)
        let mut hasher_b = Sha256::new();
        hasher_b.update(&self.key[40 + x..40 + x + 36]);
        hasher_b.update(msg_key);
        let sha256_b = hasher_b.finalize();

        // aes_key = sha256_a[0..8] + sha256_b[8..24] + sha256_a[24..32]
        let mut aes_key = [0u8; 32];
        aes_key[0..8].copy_from_slice(&sha256_a[0..8]);
        aes_key[8..24].copy_from_slice(&sha256_b[8..24]);
        aes_key[24..32].copy_from_slice(&sha256_a[24..32]);

        // aes_iv = sha256_b[0..8] + sha256_a[8..24] + sha256_b[24..32]
        let mut aes_iv = [0u8; 32];
        aes_iv[0..8].copy_from_slice(&sha256_b[0..8]);
        aes_iv[8..24].copy_from_slice(&sha256_a[8..24]);
        aes_iv[24..32].copy_from_slice(&sha256_b[24..32]);

        (aes_key, aes_iv)
    }

    /// Computes MTProto 2.0 msg_key (16 bytes = middle 128-bits of SHA-256)
    pub fn compute_msg_key(&self, plaintext: &[u8], send: bool) -> [u8; 16] {
        let x = if send { 0 } else { 8 };
        let mut hasher = Sha256::new();
        hasher.update(&self.key[88 + x..88 + x + 32]);
        hasher.update(plaintext);
        let hash = hasher.finalize();
        let mut msg_key = [0u8; 16];
        msg_key.copy_from_slice(&hash[8..24]);
        msg_key
    }
}
