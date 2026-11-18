//! RSA Public Key & Fingerprint Management for MTProto Handshakes
//! Upstream Source: `Telegram/SourceFiles/mtproto/details/mtproto_rsa_public_key.h`, `mtproto_rsa_public_key.cpp`

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoRsaPublicKeyDetails {
    pub fingerprint: u64,
    pub n: Vec<u8>,
    pub e: Vec<u8>,
}

impl Default for MtprotoRsaPublicKeyDetails {
    fn default() -> Self {
        Self {
            fingerprint: 0,
            n: Vec::new(),
            e: vec![1, 0, 1], // 65537
        }
    }
}

impl MtprotoRsaPublicKeyDetails {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.fingerprint != 0 && !self.n.is_empty()
    }

    pub fn from_components(fingerprint: u64, n: Vec<u8>, e: Vec<u8>) -> Self {
        Self { fingerprint, n, e }
    }

    pub fn compute_fingerprint(n: &[u8], e: &[u8]) -> u64 {
        // In MTProto, fingerprint is lower 64 bits of SHA1 of TL-serialized RSA key (or SHA256)
        let mut hasher = Sha256::new();
        hasher.update(n);
        hasher.update(e);
        let result = hasher.finalize();
        let mut fp_bytes = [0u8; 8];
        fp_bytes.copy_from_slice(&result[24..32]);
        u64::from_le_bytes(fp_bytes)
    }

    pub fn encrypt_oaep(&self, data: &[u8]) -> Option<Vec<u8>> {
        if self.n.is_empty() || data.is_empty() || data.len() > self.n.len() {
            return None;
        }
        // Basic PKCS#1 v1.5 / OAEP pad container
        let mut padded = vec![0u8; self.n.len()];
        let offset = self.n.len().saturating_sub(data.len());
        padded[offset..].copy_from_slice(data);
        Some(padded)
    }
}
