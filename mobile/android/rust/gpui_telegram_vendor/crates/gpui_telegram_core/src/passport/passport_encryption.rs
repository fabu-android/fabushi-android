//! Telegram Passport Cryptographic Scheme Implementation
//! Upstream: `Telegram/SourceFiles/passport/passport_encryption.h`, `passport_encryption.cpp`
//! Subsystem: `passport`, Feature ID: `TCG-N02`

use std::collections::BTreeMap;
use aes::Aes256;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};

pub const AES_KEY_LENGTH: usize = 32;
pub const AES_IV_LENGTH: usize = 16;
pub const SECRET_SIZE: usize = 32;
pub const AES_PARAMS_HASH_SIZE: usize = 64;
pub const MIN_PADDING: usize = 32;
pub const MAX_PADDING: usize = 255;
pub const ALIGN_TO: usize = 16;

type Aes256CbcEnc = cbc::Encryptor<Aes256>;
type Aes256CbcDec = cbc::Decryptor<Aes256>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AesParams {
    pub key: [u8; AES_KEY_LENGTH],
    pub iv: [u8; AES_IV_LENGTH],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedData {
    pub secret: Vec<u8>,
    pub hash: Vec<u8>,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PassportEncryption {
    pub secret: Option<Vec<u8>>,
    pub secret_id: u64,
    pub is_unlocked: bool,
}

impl PassportEncryption {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_secret(secret: Vec<u8>) -> Result<Self, String> {
        if !Self::check_secret_bytes(&secret) {
            return Err("Invalid secret bytes: checksum modulo 255 != 239".to_string());
        }
        let secret_id = Self::count_secure_secret_id(&secret);
        Ok(Self {
            secret: Some(secret),
            secret_id,
            is_unlocked: true,
        })
    }

    pub fn check_bytes_mod_255(bytes: &[u8]) -> bool {
        let sum: u64 = bytes.iter().map(|&b| b as u64).sum();
        (sum % 255) == 239
    }

    pub fn check_secret_bytes(secret: &[u8]) -> bool {
        secret.len() == SECRET_SIZE && Self::check_bytes_mod_255(secret)
    }

    pub fn generate_secret_bytes() -> Vec<u8> {
        let mut result = vec![0u8; SECRET_SIZE];
        let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let u1 = uuid::Uuid::new_v4();
        let u2 = uuid::Uuid::new_v4();

        let mut hasher = Sha256::new();
        hasher.update(now.to_le_bytes());
        hasher.update(u1.as_bytes());
        hasher.update(u2.as_bytes());
        result.copy_from_slice(&hasher.finalize());

        let sum: u64 = result.iter().map(|&b| b as u64).sum();
        let current_mod = sum % 255;
        let add = (255 + 239 - current_mod) % 255;
        let first = ((result[0] as u64) + add) % 255;
        result[0] = first as u8;

        debug_assert!(Self::check_bytes_mod_255(&result));
        result
    }

    pub fn prepare_aes_params_with_hash(hash: &[u8]) -> Result<AesParams, String> {
        if hash.len() < AES_KEY_LENGTH + AES_IV_LENGTH {
            return Err(format!("Hash too short for AES params: {} bytes", hash.len()));
        }
        let mut key = [0u8; AES_KEY_LENGTH];
        let mut iv = [0u8; AES_IV_LENGTH];
        key.copy_from_slice(&hash[..AES_KEY_LENGTH]);
        iv.copy_from_slice(&hash[AES_KEY_LENGTH..AES_KEY_LENGTH + AES_IV_LENGTH]);
        Ok(AesParams { key, iv })
    }

    pub fn prepare_aes_params(bytes_for_key: &[u8]) -> AesParams {
        let mut hasher = Sha512::new();
        hasher.update(bytes_for_key);
        let hash = hasher.finalize();
        Self::prepare_aes_params_with_hash(&hash).expect("Sha512 produces 64 bytes")
    }

    pub fn aes_encrypt(plaintext: &[u8], params: &AesParams) -> Result<Vec<u8>, String> {
        if plaintext.len() % ALIGN_TO != 0 {
            return Err(format!("Plaintext length {} is not aligned to 16", plaintext.len()));
        }
        let encryptor = Aes256CbcEnc::new((&params.key).into(), (&params.iv).into());
        let mut buf = plaintext.to_vec();
        let ct = encryptor
            .encrypt_padded_mut::<cbc::cipher::block_padding::NoPadding>(&mut buf, plaintext.len())
            .map_err(|e| format!("AES CBC encryption failed: {e:?}"))?;
        Ok(ct.to_vec())
    }

    pub fn aes_decrypt(ciphertext: &[u8], params: &AesParams) -> Result<Vec<u8>, String> {
        if ciphertext.len() % ALIGN_TO != 0 || ciphertext.is_empty() {
            return Err(format!("Ciphertext length {} is not valid block size", ciphertext.len()));
        }
        let decryptor = Aes256CbcDec::new((&params.key).into(), (&params.iv).into());
        let mut buf = ciphertext.to_vec();
        let pt = decryptor
            .decrypt_padded_mut::<cbc::cipher::block_padding::NoPadding>(&mut buf)
            .map_err(|e| format!("AES CBC decryption failed: {e:?}"))?;
        Ok(pt.to_vec())
    }

    pub fn encrypt_data(bytes: &[u8], data_secret: Option<&[u8]>) -> Result<EncryptedData, String> {
        let secret = match data_secret {
            Some(s) => {
                if !Self::check_secret_bytes(s) {
                    return Err("Provided secret does not pass mod-255 check".to_string());
                }
                s.to_vec()
            }
            None => Self::generate_secret_bytes(),
        };

        let from_padding = MIN_PADDING + ALIGN_TO - 1;
        let padding_delta = MAX_PADDING - from_padding;
        let pseudo_random = (bytes.len() as u32 ^ 0x5A5A5A5A).wrapping_mul(1103515245).wrapping_add(12345);
        let random_padding = from_padding + ((pseudo_random as usize) % padding_delta);
        let padding = random_padding - ((bytes.len() + random_padding) % ALIGN_TO);

        let mut unencrypted = vec![0u8; padding + bytes.len()];
        unencrypted[0] = padding as u8;
        for i in 1..padding {
            unencrypted[i] = ((i * 37 + (pseudo_random as usize)) & 0xFF) as u8;
        }
        unencrypted[padding..].copy_from_slice(bytes);

        let data_hash = Sha256::digest(&unencrypted).to_vec();
        let mut key_seed = Vec::with_capacity(secret.len() + data_hash.len());
        key_seed.extend_from_slice(&secret);
        key_seed.extend_from_slice(&data_hash);

        let params = Self::prepare_aes_params(&key_seed);
        let encrypted = Self::aes_encrypt(&unencrypted, &params)?;

        Ok(EncryptedData {
            secret,
            hash: data_hash,
            bytes: encrypted,
        })
    }

    pub fn decrypt_data(encrypted: &[u8], data_hash: &[u8], data_secret: &[u8]) -> Result<Vec<u8>, String> {
        if data_hash.len() != 32 {
            return Err(format!("Bad data hash length: {}", data_hash.len()));
        }
        if data_secret.len() != SECRET_SIZE {
            return Err(format!("Bad data secret length: {}", data_secret.len()));
        }
        let mut key_seed = Vec::with_capacity(data_secret.len() + data_hash.len());
        key_seed.extend_from_slice(data_secret);
        key_seed.extend_from_slice(data_hash);

        let params = Self::prepare_aes_params(&key_seed);
        let decrypted = Self::aes_decrypt(encrypted, &params)?;

        let computed_hash = Sha256::digest(&decrypted);
        if computed_hash.as_slice() != data_hash {
            return Err("Data hash mismatch in decrypted payload".to_string());
        }

        let padding = decrypted[0] as usize;
        if padding < MIN_PADDING || padding > MAX_PADDING || padding > decrypted.len() {
            return Err(format!("Invalid padding size: {padding}"));
        }
        Ok(decrypted[padding..].to_vec())
    }

    pub fn encrypt_secure_secret(secret: &[u8], password_hash_for_secret: &[u8]) -> Result<Vec<u8>, String> {
        if !Self::check_secret_bytes(secret) {
            return Err("Invalid secret bytes".to_string());
        }
        let params = Self::prepare_aes_params_with_hash(password_hash_for_secret)?;
        Self::aes_encrypt(secret, &params)
    }

    pub fn decrypt_secure_secret(encrypted_secret: &[u8], password_hash_for_secret: &[u8]) -> Result<Vec<u8>, String> {
        let params = Self::prepare_aes_params_with_hash(password_hash_for_secret)?;
        let decrypted = Self::aes_decrypt(encrypted_secret, &params)?;
        if !Self::check_secret_bytes(&decrypted) {
            return Err("Decrypted secret bytes failed mod-255 check".to_string());
        }
        Ok(decrypted)
    }

    pub fn encrypt_value_secret(value_secret: &[u8], secret: &[u8], value_hash: &[u8]) -> Result<Vec<u8>, String> {
        let mut key_seed = Vec::with_capacity(secret.len() + value_hash.len());
        key_seed.extend_from_slice(secret);
        key_seed.extend_from_slice(value_hash);
        let params = Self::prepare_aes_params(&key_seed);
        Self::aes_encrypt(value_secret, &params)
    }

    pub fn decrypt_value_secret(encrypted: &[u8], secret: &[u8], value_hash: &[u8]) -> Result<Vec<u8>, String> {
        let mut key_seed = Vec::with_capacity(secret.len() + value_hash.len());
        key_seed.extend_from_slice(secret);
        key_seed.extend_from_slice(value_hash);
        let params = Self::prepare_aes_params(&key_seed);
        Self::aes_decrypt(encrypted, &params)
    }

    pub fn count_secure_secret_id(secret: &[u8]) -> u64 {
        let hash = Sha256::digest(secret);
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&hash[..8]);
        u64::from_le_bytes(bytes)
    }

    pub fn serialize_data(data: &BTreeMap<String, String>) -> Result<Vec<u8>, String> {
        serde_json::to_vec(data).map_err(|e| format!("Serialization error: {e}"))
    }

    pub fn deserialize_data(bytes: &[u8]) -> Result<BTreeMap<String, String>, String> {
        serde_json::from_slice(bytes).map_err(|e| format!("Deserialization error: {e}"))
    }
}
