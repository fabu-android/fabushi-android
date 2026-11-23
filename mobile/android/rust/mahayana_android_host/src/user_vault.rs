//! Zero-Knowledge User Vault & Per-User AI Isolation (`userVault.*`)
//! Aligned with `bhrumom/gpui-telegram` v1.4.0–v1.6.2 (`abf2b25`, `f7269cf`):
//! - Split-Key V2 Argon2id KDF parameters + client-side auth verifier derivation (zero raw password on wire)
//! - Per-user ChaCha20-Poly1305 encrypted vault items (`password`, `browser_cookie`, `ai_oauth`)
//! - X25519 Device Trust Ring sync (`requestDeviceSync`, `approveDeviceSync`, `denyDeviceSync`, `pollDeviceSyncStatus`)
//!   with atomic ciphertext wipe (`status = "consumed"`) on first approved poll.

use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use gpui_telegram_proto::user_vault::{
    Argon2KdfParamsDto, UserVaultItemDto, UserVaultKeyEnvelopeDto, DEVICE_SYNC_CODE_DOMAIN_V1,
    USER_VAULT_CATEGORY_AI_OAUTH, USER_VAULT_CATEGORY_BROWSER_COOKIE, USER_VAULT_CATEGORY_PASSWORD,
    USER_VAULT_KDF_ALG_SPLIT_KEY_V2,
};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};

use zeroize::Zeroizing;

type HmacSha256 = Hmac<Sha256>;

/// Canonical Desktop (`bhrumom/gpui-telegram` `bot-browser/src/e2ee_sync.rs`) HKDF info strings.
pub const SPLIT_KEY_AUTH_INFO: &[u8] = b"gpui-telegram/auth-verifier/v2";
pub const SPLIT_KEY_VAULT_INFO: &[u8] = b"gpui-telegram/vault-kek/v2";
const DEVICE_SYNC_TTL_SECS: i64 = 300;

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS user_vault_envelopes (
            user_id TEXT PRIMARY KEY,
            key_version INTEGER NOT NULL,
            kdf_alg TEXT NOT NULL,
            kdf_salt_b64 TEXT NOT NULL,
            kdf_m_cost_kib INTEGER NOT NULL,
            kdf_t_cost INTEGER NOT NULL,
            kdf_p_cost INTEGER NOT NULL,
            wrapped_root_key_b64 TEXT NOT NULL,
            recovery_wrapped_root_key_b64 TEXT,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS user_vault_items (
            user_id TEXT NOT NULL,
            category TEXT NOT NULL,
            item_key TEXT NOT NULL,
            revision INTEGER NOT NULL,
            nonce_b64 TEXT NOT NULL,
            ciphertext_b64 TEXT NOT NULL,
            deleted INTEGER NOT NULL DEFAULT 0,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY (user_id, category, item_key)
        );
        CREATE TABLE IF NOT EXISTS user_vault_device_sync (
            request_id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            device_name TEXT NOT NULL,
            device_public_key_b64 TEXT NOT NULL,
            verification_code TEXT NOT NULL,
            status TEXT NOT NULL,
            approver_ephemeral_pub_b64 TEXT,
            encrypted_root_key_b64 TEXT,
            nonce_b64 TEXT,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS user_vault_local_kdf (
            username_lc TEXT PRIMARY KEY,
            auth_version INTEGER NOT NULL,
            auth_salt_b64 TEXT NOT NULL,
            m_cost_kib INTEGER NOT NULL,
            t_cost INTEGER NOT NULL,
            p_cost INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("Failed initializing user_vault schema: {e}"))
}

fn resolve_user_id(params: &Value) -> String {
    params
        .get("userId")
        .or_else(|| params.get("accountId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("active")
        .to_string()
}

fn validate_category(raw: &str) -> Result<&str, String> {
    match raw.trim() {
        USER_VAULT_CATEGORY_PASSWORD => Ok(USER_VAULT_CATEGORY_PASSWORD),
        USER_VAULT_CATEGORY_BROWSER_COOKIE => Ok(USER_VAULT_CATEGORY_BROWSER_COOKIE),
        USER_VAULT_CATEGORY_AI_OAUTH => Ok(USER_VAULT_CATEGORY_AI_OAUTH),
        other => Err(format!(
            "Unsupported vault category '{other}' (expected password | browser_cookie | ai_oauth)"
        )),
    }
}

/// Generates a fresh random 16-byte standard base64 salt for Split-Key V2 authentication.
pub fn generate_split_key_salt_b64() -> String {
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    B64.encode(salt)
}

/// Derives Split-Key V2 `(auth_verifier_32, vault_kek_32)` from `(password, salt_bytes, kdf_params)`
/// via Argon2id + HKDF-SHA256 (`salt = None`) matching Desktop `bot-browser/src/e2ee_sync.rs`
/// bit-for-bit so accounts are 100% interoperable between Desktop and Android.
pub fn derive_split_key_v2(
    password: &str,
    salt: &[u8],
    kdf: Argon2KdfParamsDto,
) -> Result<([u8; 32], [u8; 32]), String> {
    if password.is_empty() {
        return Err("Password must not be empty".to_string());
    }
    if salt.len() < 8 {
        return Err("Salt must be at least 8 bytes".to_string());
    }
    if !(1_024..=1_048_576).contains(&kdf.m_cost_kib)
        || !(1..=64).contains(&kdf.t_cost)
        || !(1..=16).contains(&kdf.p_cost)
    {
        return Err(format!(
            "Argon2id KDF params out of safe bounds (m={}, t={}, p={})",
            kdf.m_cost_kib, kdf.t_cost, kdf.p_cost
        ));
    }
    let argon_params = Params::new(kdf.m_cost_kib, kdf.t_cost, kdf.p_cost, Some(32))
        .map_err(|e| format!("Argon2 params: {e}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut master_ikm = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut *master_ikm)
        .map_err(|e| format!("Argon2id derive failed: {e}"))?;

    // CRITICAL: Match Desktop `Hkdf::<Sha256>::new(None, &*master_key)` (salt = None)
    let hk = Hkdf::<Sha256>::new(None, &*master_ikm);
    let mut auth_verifier = [0u8; 32];
    let mut vault_kek = [0u8; 32];
    hk.expand(SPLIT_KEY_AUTH_INFO, &mut auth_verifier)
        .map_err(|_| "HKDF expand auth_verifier failed".to_string())?;
    hk.expand(SPLIT_KEY_VAULT_INFO, &mut vault_kek)
        .map_err(|_| "HKDF expand vault_kek failed".to_string())?;
    Ok((auth_verifier, vault_kek))
}

/// Convenience helper decoding a standard base64 16-byte salt and returning
/// `(auth_verifier_b64, vault_kek_32)` matching Desktop `derive_split_key`.
pub fn derive_split_key_v2_b64(
    password: &str,
    auth_salt_b64: &str,
    kdf: Argon2KdfParamsDto,
) -> Result<(String, [u8; 32]), String> {
    let salt_vec = B64
        .decode(auth_salt_b64.trim())
        .map_err(|e| format!("Invalid split-key auth_salt_b64: {e}"))?;
    if salt_vec.len() != 16 {
        return Err(format!(
            "Invalid split-key salt length: {} bytes (expected 16)",
            salt_vec.len()
        ));
    }
    let (verifier, kek) = derive_split_key_v2(password, &salt_vec, kdf)?;
    Ok((B64.encode(verifier), kek))
}

/// Deterministic 6-digit verification code bound to `(request_id, device_public_key_b64)`.
pub fn compute_device_sync_code(request_id: &str, device_public_key_b64: &str) -> String {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(DEVICE_SYNC_CODE_DOMAIN_V1)
        .expect("HMAC accepts any key size");
    mac.update(request_id.as_bytes());
    mac.update(b":");
    mac.update(device_public_key_b64.as_bytes());
    let tag = mac.finalize().into_bytes();
    let num = u32::from_be_bytes([tag[0], tag[1], tag[2], tag[3]]) % 1_000_000;
    format!("{num:06}")
}

fn chacha_encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<(String, String), String> {
    let cipher = ChaCha20Poly1305::new(key.into());
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|_| "ChaCha20-Poly1305 encryption failed".to_string())?;
    Ok((B64.encode(nonce_bytes), B64.encode(ct)))
}

fn chacha_decrypt(key: &[u8; 32], nonce_b64: &str, ciphertext_b64: &str) -> Result<Vec<u8>, String> {
    let nonce_bytes = B64
        .decode(nonce_b64.trim())
        .map_err(|e| format!("Invalid nonce_b64: {e}"))?;
    if nonce_bytes.len() != 12 {
        return Err("Nonce must be 12 bytes".to_string());
    }
    let ct = B64
        .decode(ciphertext_b64.trim())
        .map_err(|e| format!("Invalid ciphertext_b64: {e}"))?;
    let cipher = ChaCha20Poly1305::new(key.into());
    cipher
        .decrypt(Nonce::from_slice(&nonce_bytes), ct.as_ref())
        .map_err(|_| "ChaCha20-Poly1305 decryption failed (wrong key or tampered ciphertext)".to_string())
}

fn derive_deterministic_fake_salt(username_lc: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"gpui-telegram/split-key-v2/fake-salt/v1:");
    hasher.update(username_lc.as_bytes());
    let digest = hasher.finalize();
    B64.encode(&digest[..16])
}

pub fn handle_rpc(conn: &Connection, submethod: &str, params: &Value) -> Result<Value, String> {
    init_schema(conn)?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let now_secs = now_ms / 1000;
    let user_id = resolve_user_id(params);

    match submethod {
        "getKdfParams" => {
            let username = params
                .get("username")
                .or_else(|| params.get("usernameOrEmail"))
                .and_then(Value::as_str)
                .unwrap_or("anonymous")
                .trim()
                .to_ascii_lowercase();
            let existing: Option<(i32, String, u32, u32, u32)> = conn
                .query_row(
                    "SELECT auth_version, auth_salt_b64, m_cost_kib, t_cost, p_cost
                     FROM user_vault_local_kdf WHERE username_lc = ?1",
                    params![username],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;

            let (auth_version, salt_b64, kdf_params) = match existing {
                Some((ver, salt, m, t, p)) => (
                    ver,
                    salt,
                    Argon2KdfParamsDto {
                        m_cost_kib: m,
                        t_cost: t,
                        p_cost: p,
                    },
                ),
                None => (
                    2,
                    derive_deterministic_fake_salt(&username),
                    Argon2KdfParamsDto::default(),
                ),
            };
            Ok(json!({
                "authVersion": auth_version,
                "authSaltB64": salt_b64,
                "kdfParams": kdf_params,
            }))
        }

        "deriveSplitKeyVerifier" => {
            let password = params
                .get("password")
                .and_then(Value::as_str)
                .ok_or_else(|| "userVault.deriveSplitKeyVerifier requires 'password'".to_string())?;
            let salt_bytes = if let Some(s) = params.get("authSaltB64").and_then(Value::as_str) {
                B64.decode(s.trim())
                    .map_err(|e| format!("Invalid authSaltB64: {e}"))?
            } else {
                let mut salt = [0u8; 16];
                OsRng.fill_bytes(&mut salt);
                salt.to_vec()
            };
            let kdf = Argon2KdfParamsDto {
                m_cost_kib: params
                    .get("mCostKib")
                    .and_then(Value::as_u64)
                    .map(|v| v as u32)
                    .unwrap_or(16_384),
                t_cost: params
                    .get("tCost")
                    .and_then(Value::as_u64)
                    .map(|v| v as u32)
                    .unwrap_or(2),
                p_cost: params
                    .get("pCost")
                    .and_then(Value::as_u64)
                    .map(|v| v as u32)
                    .unwrap_or(1),
            };
            let (verifier, kek) = derive_split_key_v2(password, &salt_bytes, kdf)?;
            if let Some(username) = params.get("username").and_then(Value::as_str) {
                let u = username.trim().to_ascii_lowercase();
                if !u.is_empty() {
                    let _ = conn.execute(
                        "INSERT OR REPLACE INTO user_vault_local_kdf
                         (username_lc, auth_version, auth_salt_b64, m_cost_kib, t_cost, p_cost)
                         VALUES (?1, 2, ?2, ?3, ?4, ?5)",
                        params![u, B64.encode(&salt_bytes), kdf.m_cost_kib, kdf.t_cost, kdf.p_cost],
                    );
                }
            }
            Ok(json!({
                "authVersion": 2,
                "kdfAlg": USER_VAULT_KDF_ALG_SPLIT_KEY_V2,
                "authSaltB64": B64.encode(&salt_bytes),
                "authVerifierB64": B64.encode(verifier),
                "vaultKekB64": B64.encode(kek),
                "kdfParams": kdf,
            }))
        }

        "initEnvelope" => {
            let password = params
                .get("password")
                .and_then(Value::as_str)
                .ok_or_else(|| "userVault.initEnvelope requires 'password'".to_string())?;
            let kdf = Argon2KdfParamsDto {
                m_cost_kib: params
                    .get("mCostKib")
                    .and_then(Value::as_u64)
                    .map(|v| v as u32)
                    .unwrap_or(16_384),
                t_cost: params
                    .get("tCost")
                    .and_then(Value::as_u64)
                    .map(|v| v as u32)
                    .unwrap_or(2),
                p_cost: params
                    .get("pCost")
                    .and_then(Value::as_u64)
                    .map(|v| v as u32)
                    .unwrap_or(1),
            };
            let mut salt = [0u8; 16];
            OsRng.fill_bytes(&mut salt);
            let mut root_key = [0u8; 32];
            OsRng.fill_bytes(&mut root_key);
            let (_, kek) = derive_split_key_v2(password, &salt, kdf)?;
            let (nonce_b64, ct_b64) = chacha_encrypt(&kek, &root_key)?;
            let wrapped = format!("{nonce_b64}.{ct_b64}");

            let mut recovery_key = [0u8; 32];
            OsRng.fill_bytes(&mut recovery_key);
            let (r_nonce, r_ct) = chacha_encrypt(&recovery_key, &root_key)?;
            let recovery_wrapped = format!("{r_nonce}.{r_ct}");

            let env_dto = UserVaultKeyEnvelopeDto {
                key_version: 2,
                kdf_alg: USER_VAULT_KDF_ALG_SPLIT_KEY_V2.to_string(),
                kdf_salt_b64: B64.encode(salt),
                kdf_m_cost_kib: kdf.m_cost_kib,
                kdf_t_cost: kdf.t_cost,
                kdf_p_cost: kdf.p_cost,
                wrapped_root_key_b64: wrapped,
                recovery_wrapped_root_key_b64: Some(recovery_wrapped),
                updated_at_ms: now_ms,
            };

            conn.execute(
                "INSERT OR REPLACE INTO user_vault_envelopes
                 (user_id, key_version, kdf_alg, kdf_salt_b64, kdf_m_cost_kib, kdf_t_cost, kdf_p_cost,
                  wrapped_root_key_b64, recovery_wrapped_root_key_b64, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    user_id,
                    env_dto.key_version,
                    env_dto.kdf_alg,
                    env_dto.kdf_salt_b64,
                    env_dto.kdf_m_cost_kib,
                    env_dto.kdf_t_cost,
                    env_dto.kdf_p_cost,
                    env_dto.wrapped_root_key_b64,
                    env_dto.recovery_wrapped_root_key_b64,
                    env_dto.updated_at_ms,
                ],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "userId": user_id,
                "envelope": env_dto,
                "rootKeyB64": B64.encode(root_key),
                "recoveryKeyB64": B64.encode(recovery_key),
            }))
        }

        "getEnvelope" => {
            let env: Option<UserVaultKeyEnvelopeDto> = conn
                .query_row(
                    "SELECT key_version, kdf_alg, kdf_salt_b64, kdf_m_cost_kib, kdf_t_cost, kdf_p_cost,
                            wrapped_root_key_b64, recovery_wrapped_root_key_b64, updated_at_ms
                     FROM user_vault_envelopes WHERE user_id = ?1",
                    params![user_id],
                    |r| {
                        Ok(UserVaultKeyEnvelopeDto {
                            key_version: r.get(0)?,
                            kdf_alg: r.get(1)?,
                            kdf_salt_b64: r.get(2)?,
                            kdf_m_cost_kib: r.get(3)?,
                            kdf_t_cost: r.get(4)?,
                            kdf_p_cost: r.get(5)?,
                            wrapped_root_key_b64: r.get(6)?,
                            recovery_wrapped_root_key_b64: r.get(7)?,
                            updated_at_ms: r.get(8)?,
                        })
                    },
                )
                .optional()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "userId": user_id, "envelope": env }))
        }

        "upsertItem" => {
            let category = validate_category(
                params
                    .get("category")
                    .and_then(Value::as_str)
                    .unwrap_or(USER_VAULT_CATEGORY_AI_OAUTH),
            )?;
            let item_key = params
                .get("itemKey")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "userVault.upsertItem requires non-empty 'itemKey'".to_string())?;
            let deleted = params.get("deleted").and_then(Value::as_bool).unwrap_or(false);

            let (nonce_b64, ciphertext_b64) = if let (Some(n), Some(c)) = (
                params.get("nonceB64").and_then(Value::as_str),
                params.get("ciphertextB64").and_then(Value::as_str),
            ) {
                (n.to_string(), c.to_string())
            } else if let (Some(root_key_b64), Some(plaintext)) = (
                params.get("rootKeyB64").and_then(Value::as_str),
                params.get("plaintext").and_then(Value::as_str),
            ) {
                let raw_key = B64
                    .decode(root_key_b64.trim())
                    .map_err(|e| format!("Invalid rootKeyB64: {e}"))?;
                let key_arr: [u8; 32] = raw_key
                    .try_into()
                    .map_err(|_| "rootKeyB64 must decode to 32 bytes".to_string())?;
                chacha_encrypt(&key_arr, plaintext.as_bytes())?
            } else {
                return Err(
                    "userVault.upsertItem requires ('nonceB64' + 'ciphertextB64') or ('rootKeyB64' + 'plaintext')"
                        .to_string(),
                );
            };

            let current_rev: i64 = conn
                .query_row(
                    "SELECT revision FROM user_vault_items
                     WHERE user_id = ?1 AND category = ?2 AND item_key = ?3",
                    params![user_id, category, item_key],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .unwrap_or(0);

            if let Some(expected) = params.get("expectedRevision").and_then(Value::as_i64) {
                if expected != current_rev {
                    return Err(format!(
                        "Vault revision conflict for {category}/{item_key}: expected {expected}, current {current_rev}"
                    ));
                }
            }

            let next_rev = current_rev + 1;
            conn.execute(
                "INSERT OR REPLACE INTO user_vault_items
                 (user_id, category, item_key, revision, nonce_b64, ciphertext_b64, deleted, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    user_id,
                    category,
                    item_key,
                    next_rev,
                    nonce_b64,
                    ciphertext_b64,
                    if deleted { 1 } else { 0 },
                    now_ms,
                ],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "ok": true,
                "userId": user_id,
                "category": category,
                "itemKey": item_key,
                "revision": next_rev,
                "updatedAtMs": now_ms,
            }))
        }

        "syncPull" => {
            let since_rev = params
                .get("sinceRevision")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let category_filter = params
                .get("category")
                .and_then(Value::as_str)
                .map(validate_category)
                .transpose()?;
            let decrypt_key: Option<[u8; 32]> = params
                .get("rootKeyB64")
                .and_then(Value::as_str)
                .map(|b| {
                    let v = B64
                        .decode(b.trim())
                        .map_err(|e| format!("Invalid rootKeyB64: {e}"))?;
                    v.try_into()
                        .map_err(|_| "rootKeyB64 must decode to 32 bytes".to_string())
                })
                .transpose()?;

            let mut stmt = conn
                .prepare(
                    "SELECT category, item_key, revision, nonce_b64, ciphertext_b64, deleted, updated_at_ms
                     FROM user_vault_items
                     WHERE user_id = ?1 AND revision > ?2
                     ORDER BY revision ASC",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![user_id, since_rev], |r| {
                    Ok(UserVaultItemDto {
                        category: r.get(0)?,
                        item_key: r.get(1)?,
                        revision: r.get(2)?,
                        nonce_b64: r.get(3)?,
                        ciphertext_b64: r.get(4)?,
                        deleted: r.get::<_, i64>(5)? != 0,
                        updated_at_ms: r.get(6)?,
                    })
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;

            let mut latest_rev = since_rev;
            let mut out = Vec::new();
            for item in rows {
                if let Some(cat) = category_filter {
                    if item.category != cat {
                        continue;
                    }
                }
                if item.revision > latest_rev {
                    latest_rev = item.revision;
                }
                let mut entry = serde_json::to_value(&item).unwrap_or_else(|_| json!({}));
                if let Some(ref key) = decrypt_key {
                    if !item.deleted {
                        let plain_bytes =
                            chacha_decrypt(key, &item.nonce_b64, &item.ciphertext_b64)?;
                        let plain_text = String::from_utf8(plain_bytes)
                            .map_err(|e| format!("Decrypted vault item is not UTF-8: {e}"))?;
                        if let Some(obj) = entry.as_object_mut() {
                            obj.insert("plaintext".to_string(), Value::String(plain_text));
                        }
                    }
                }
                out.push(entry);
            }

            Ok(json!({
                "userId": user_id,
                "latestRevision": latest_rev,
                "items": out,
            }))
        }

        "requestDeviceSync" => {
            let device_name = params
                .get("deviceName")
                .and_then(Value::as_str)
                .unwrap_or("Fabushi Android")
                .trim();
            let (device_pub_b64, ephemeral_priv_b64) =
                if let Some(pub_b64) = params.get("devicePublicKeyB64").and_then(Value::as_str) {
                    (pub_b64.trim().to_string(), None)
                } else {
                    let secret = StaticSecret::random_from_rng(OsRng);
                    let public = PublicKey::from(&secret);
                    (
                        B64.encode(public.as_bytes()),
                        Some(B64.encode(secret.to_bytes())),
                    )
                };
            let request_id = format!("dsync_{}", uuid::Uuid::new_v4().simple());
            let verification_code = compute_device_sync_code(&request_id, &device_pub_b64);
            let expires_at = now_secs + DEVICE_SYNC_TTL_SECS;

            conn.execute(
                "INSERT INTO user_vault_device_sync
                 (request_id, user_id, device_name, device_public_key_b64, verification_code, status, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7)",
                params![
                    request_id,
                    user_id,
                    device_name,
                    device_pub_b64,
                    verification_code,
                    now_secs,
                    expires_at,
                ],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "requestId": request_id,
                "deviceName": device_name,
                "devicePublicKeyB64": device_pub_b64,
                "devicePrivateKeyB64": ephemeral_priv_b64,
                "verificationCode": verification_code,
                "expiresAt": expires_at,
            }))
        }

        "listPendingDeviceSync" => {
            let mut stmt = conn
                .prepare(
                    "SELECT request_id, device_name, device_public_key_b64, verification_code, status, created_at, expires_at
                     FROM user_vault_device_sync
                     WHERE user_id = ?1 AND status = 'pending' AND expires_at >= ?2
                     ORDER BY created_at DESC",
                )
                .map_err(|e| e.to_string())?;
            let items = stmt
                .query_map(params![user_id, now_secs], |r| {
                    Ok(json!({
                        "requestId": r.get::<_, String>(0)?,
                        "deviceName": r.get::<_, String>(1)?,
                        "devicePublicKeyB64": r.get::<_, String>(2)?,
                        "verificationCode": r.get::<_, String>(3)?,
                        "status": r.get::<_, String>(4)?,
                        "createdAt": r.get::<_, i64>(5)?,
                        "expiresAt": r.get::<_, i64>(6)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "items": items }))
        }

        "approveDeviceSync" => {
            let request_id = params
                .get("requestId")
                .and_then(Value::as_str)
                .ok_or_else(|| "userVault.approveDeviceSync requires 'requestId'".to_string())?;
            let (dev_pub_b64, status, expires_at): (String, String, i64) = conn
                .query_row(
                    "SELECT device_public_key_b64, status, expires_at
                     FROM user_vault_device_sync WHERE request_id = ?1 AND user_id = ?2",
                    params![request_id, user_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Device sync request '{request_id}' not found"))?;
            if status != "pending" {
                return Err(format!("Device sync request '{request_id}' is already '{status}'"));
            }
            if expires_at < now_secs {
                return Err(format!("Device sync request '{request_id}' expired"));
            }

            let (approver_pub_b64, enc_root_b64, nonce_b64) = if let (Some(ap), Some(er), Some(nb)) = (
                params.get("approverEphemeralPubB64").and_then(Value::as_str),
                params.get("encryptedRootKeyB64").and_then(Value::as_str),
                params.get("nonceB64").and_then(Value::as_str),
            ) {
                (ap.to_string(), er.to_string(), nb.to_string())
            } else if let Some(root_key_b64) = params.get("rootKeyB64").and_then(Value::as_str) {
                let root_bytes = B64
                    .decode(root_key_b64.trim())
                    .map_err(|e| format!("Invalid rootKeyB64: {e}"))?;
                let peer_pub_bytes: [u8; 32] = B64
                    .decode(dev_pub_b64.trim())
                    .map_err(|e| format!("Invalid stored device_public_key_b64: {e}"))?
                    .try_into()
                    .map_err(|_| "device_public_key_b64 must be 32 bytes".to_string())?;
                let peer_pub = PublicKey::from(peer_pub_bytes);
                let approver_secret = StaticSecret::random_from_rng(OsRng);
                let approver_pub = PublicKey::from(&approver_secret);
                let shared = approver_secret.diffie_hellman(&peer_pub);
                let hk = Hkdf::<Sha256>::new(Some(request_id.as_bytes()), shared.as_bytes());
                let mut wrap_key = [0u8; 32];
                hk.expand(b"gpui-telegram/device-sync-wrap/v1", &mut wrap_key)
                    .map_err(|_| "HKDF expand failed".to_string())?;
                let (nb, ct) = chacha_encrypt(&wrap_key, &root_bytes)?;
                (B64.encode(approver_pub.as_bytes()), ct, nb)
            } else {
                return Err(
                    "userVault.approveDeviceSync requires 'rootKeyB64' or pre-encrypted payload"
                        .to_string(),
                );
            };

            conn.execute(
                "UPDATE user_vault_device_sync
                 SET status = 'approved',
                     approver_ephemeral_pub_b64 = ?1,
                     encrypted_root_key_b64 = ?2,
                     nonce_b64 = ?3
                 WHERE request_id = ?4 AND user_id = ?5",
                params![approver_pub_b64, enc_root_b64, nonce_b64, request_id, user_id],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "ok": true,
                "requestId": request_id,
                "status": "approved",
            }))
        }

        "denyDeviceSync" => {
            let request_id = params
                .get("requestId")
                .and_then(Value::as_str)
                .ok_or_else(|| "userVault.denyDeviceSync requires 'requestId'".to_string())?;
            let changed = conn
                .execute(
                    "UPDATE user_vault_device_sync
                     SET status = 'denied',
                         approver_ephemeral_pub_b64 = NULL,
                         encrypted_root_key_b64 = NULL,
                         nonce_b64 = NULL
                     WHERE request_id = ?1 AND user_id = ?2",
                    params![request_id, user_id],
                )
                .map_err(|e| e.to_string())?;
            if changed == 0 {
                return Err(format!("Device sync request '{request_id}' not found"));
            }
            Ok(json!({
                "ok": true,
                "requestId": request_id,
                "status": "denied",
            }))
        }

        "pollDeviceSyncStatus" => {
            let request_id = params
                .get("requestId")
                .and_then(Value::as_str)
                .ok_or_else(|| "userVault.pollDeviceSyncStatus requires 'requestId'".to_string())?;
            let row: Option<(String, Option<String>, Option<String>, Option<String>, i64)> = conn
                .query_row(
                    "SELECT status, approver_ephemeral_pub_b64, encrypted_root_key_b64, nonce_b64, expires_at
                     FROM user_vault_device_sync WHERE request_id = ?1 AND user_id = ?2",
                    params![request_id, user_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;

            let Some((status, approver_pub, enc_root, nonce, expires_at)) = row else {
                return Err(format!("Device sync request '{request_id}' not found"));
            };

            // Atomic one-time wipe on first approved read (matching desktop v1.4.0 user_vault.rs contract)
            if status == "approved" {
                conn.execute(
                    "UPDATE user_vault_device_sync
                     SET status = 'consumed',
                         approver_ephemeral_pub_b64 = NULL,
                         encrypted_root_key_b64 = NULL,
                         nonce_b64 = NULL
                     WHERE request_id = ?1 AND user_id = ?2 AND status = 'approved'",
                    params![request_id, user_id],
                )
                .map_err(|e| e.to_string())?;

                let mut decrypted_root_b64: Option<String> = None;
                if let (Some(priv_b64), Some(ref ap_b64), Some(ref ct_b64), Some(ref nb_b64)) = (
                    params.get("devicePrivateKeyB64").and_then(Value::as_str),
                    &approver_pub,
                    &enc_root,
                    &nonce,
                ) {
                    let priv_arr: [u8; 32] = B64
                        .decode(priv_b64.trim())
                        .map_err(|e| format!("Invalid devicePrivateKeyB64: {e}"))?
                        .try_into()
                        .map_err(|_| "devicePrivateKeyB64 must be 32 bytes".to_string())?;
                    let ap_arr: [u8; 32] = B64
                        .decode(ap_b64.trim())
                        .map_err(|e| format!("Invalid approverEphemeralPubB64: {e}"))?
                        .try_into()
                        .map_err(|_| "approverEphemeralPubB64 must be 32 bytes".to_string())?;
                    let secret = StaticSecret::from(priv_arr);
                    let shared = secret.diffie_hellman(&PublicKey::from(ap_arr));
                    let hk = Hkdf::<Sha256>::new(Some(request_id.as_bytes()), shared.as_bytes());
                    let mut wrap_key = [0u8; 32];
                    hk.expand(b"gpui-telegram/device-sync-wrap/v1", &mut wrap_key)
                        .map_err(|_| "HKDF expand failed".to_string())?;
                    let root_bytes = chacha_decrypt(&wrap_key, nb_b64, ct_b64)?;
                    decrypted_root_b64 = Some(B64.encode(root_bytes));
                }

                return Ok(json!({
                    "requestId": request_id,
                    "status": "approved",
                    "approverEphemeralPubB64": approver_pub,
                    "encryptedRootKeyB64": enc_root,
                    "nonceB64": nonce,
                    "decryptedRootKeyB64": decrypted_root_b64,
                    "expiresAt": expires_at,
                }));
            }

            Ok(json!({
                "requestId": request_id,
                "status": status,
                "approverEphemeralPubB64": Value::Null,
                "encryptedRootKeyB64": Value::Null,
                "nonceB64": Value::Null,
                "expiresAt": expires_at,
            }))
        }

        other => Err(format!("Unsupported userVault method: {other}")),
    }
}
