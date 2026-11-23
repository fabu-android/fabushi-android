//! Extended Telegram Capabilities (`telegram.auth.*`, `telegram.messaging.*`,
//! `telegram.calls.*`, `telegram.server.*`):
//! - Multi-account switcher with `GenerationFence` & Argon2id local Passcode lock
//! - Phone/2FA/Passkey authentication state machine
//! - Forum Topics, 24h Stories, Built-in Sticker Sets, Channel Stats, JSON/HTML Chat Export
//! - <60ms VAD barge-in DSP (`vadFeedPcm`) and fail-closed VoIP call methods
//! - Lock-free `gpui_telegram_server` `/api/v1/*` HTTP client + SQLite write-through caches & offline outbox

use crate::waveform::{
    pcm_bytes_to_i16, rpc_decode_waveform, rpc_encode_waveform, VadBargeInDetector,
};
use argon2::Argon2;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::sync::Mutex;

pub const DEFAULT_DESKTOP_SERVER_URL: &str = "http://34.46.77.133:8080";
const MAX_MEDIA_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;

/// Hashes a passcode or 2FA secret with Argon2id + 16-byte random salt.
pub fn hash_secret_argon2id(secret: &str) -> Result<(String, String), String> {
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    let mut digest = [0u8; 32];
    Argon2::default()
        .hash_password_into(secret.as_bytes(), &salt, &mut digest)
        .map_err(|e| format!("Argon2id hashing failed: {e}"))?;
    Ok((hex_encode(&salt), hex_encode(&digest)))
}

/// Verifies a passcode or 2FA secret against stored Argon2id salt + digest in constant time.
pub fn verify_secret_argon2id(
    secret: &str,
    salt_hex: &str,
    expected_digest_hex: &str,
) -> Result<bool, String> {
    let salt = hex_decode(salt_hex)?;
    let expected = hex_decode(expected_digest_hex)?;
    if expected.len() != 32 {
        return Ok(false);
    }
    let mut actual = [0u8; 32];
    Argon2::default()
        .hash_password_into(secret.as_bytes(), &salt, &mut actual)
        .map_err(|e| format!("Argon2id verify failed: {e}"))?;
    let mut diff = 0u8;
    for (a, b) in actual.iter().zip(expected.iter()) {
        diff |= a ^ b;
    }
    Ok(diff == 0)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
    let clean = hex.trim();
    if !clean.is_ascii() {
        return Err("Hex string must be ASCII".into());
    }
    if clean.len() % 2 != 0 {
        return Err("Odd hex length".into());
    }
    let mut out = Vec::with_capacity(clean.len() / 2);
    for i in (0..clean.len()).step_by(2) {
        out.push(
            u8::from_str_radix(&clean[i..i + 2], 16)
                .map_err(|e| format!("Invalid hex byte: {e}"))?,
        );
    }
    Ok(out)
}

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS tg_accounts (
            account_id TEXT PRIMARY KEY,
            phone TEXT NOT NULL,
            username TEXT NOT NULL,
            display_name TEXT NOT NULL,
            is_active INTEGER NOT NULL DEFAULT 0,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tg_fence (
            singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
            generation INTEGER NOT NULL DEFAULT 1,
            passcode_salt_hex TEXT,
            passcode_hash_hex TEXT,
            two_factor_salt_hex TEXT,
            two_factor_hash_hex TEXT,
            two_factor_hint TEXT,
            server_endpoint TEXT NOT NULL DEFAULT 'http://34.46.77.133:8080',
            sync_pts INTEGER NOT NULL DEFAULT 1
        );
        INSERT OR IGNORE INTO tg_fence (singleton_id, generation) VALUES (1, 1);
        CREATE TABLE IF NOT EXISTS tg_passkeys (
            credential_id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            public_key_hex TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tg_topics (
            topic_id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL,
            title TEXT NOT NULL,
            icon_emoji TEXT NOT NULL,
            closed INTEGER NOT NULL DEFAULT 0,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tg_stories (
            story_id TEXT PRIMARY KEY,
            author_id TEXT NOT NULL,
            caption TEXT NOT NULL,
            media_kind TEXT NOT NULL,
            privacy TEXT NOT NULL,
            views_count INTEGER NOT NULL DEFAULT 0,
            expires_at_ms INTEGER NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tg_sticker_sets (
            set_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            short_name TEXT NOT NULL,
            emojis_json TEXT NOT NULL,
            installed INTEGER NOT NULL DEFAULT 1
        );
        CREATE TABLE IF NOT EXISTS tg_calls (
            call_id TEXT PRIMARY KEY,
            peer_id TEXT NOT NULL,
            is_group INTEGER NOT NULL DEFAULT 0,
            state TEXT NOT NULL,
            muted INTEGER NOT NULL DEFAULT 0,
            video_enabled INTEGER NOT NULL DEFAULT 0,
            screen_sharing INTEGER NOT NULL DEFAULT 0,
            emoji_fingerprint TEXT NOT NULL,
            started_at_ms INTEGER NOT NULL,
            ended_at_ms INTEGER
        );
        CREATE TABLE IF NOT EXISTS tg_server_outbox (
            seq INTEGER PRIMARY KEY AUTOINCREMENT,
            payload_json TEXT NOT NULL,
            flushed INTEGER NOT NULL DEFAULT 0,
            retry_count INTEGER NOT NULL DEFAULT 0,
            remote_id TEXT,
            flushed_at_ms INTEGER,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tg_auth_challenges (
            phone_code_hash TEXT PRIMARY KEY,
            phone TEXT NOT NULL,
            code_sha256_hex TEXT NOT NULL,
            expires_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tg_server_chats (
            chat_id INTEGER PRIMARY KEY,
            is_pinned INTEGER NOT NULL DEFAULT 0,
            is_muted INTEGER NOT NULL DEFAULT 0,
            mute_until_ms INTEGER,
            is_archived INTEGER NOT NULL DEFAULT 0,
            last_message_date_ms INTEGER NOT NULL DEFAULT 0,
            pts INTEGER NOT NULL DEFAULT 0,
            chat_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tg_server_pinned_messages (
            chat_id INTEGER NOT NULL,
            message_id INTEGER NOT NULL,
            pin_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY (chat_id, message_id)
        );
        CREATE TABLE IF NOT EXISTS tg_server_drafts (
            chat_id INTEGER NOT NULL,
            topic_id INTEGER NOT NULL DEFAULT 0,
            draft_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY (chat_id, topic_id)
        );
        CREATE TABLE IF NOT EXISTS tg_server_folders (
            folder_id INTEGER PRIMARY KEY,
            sort_order INTEGER NOT NULL DEFAULT 0,
            folder_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tg_server_bots (
            bot_key TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            bot_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tg_server_computers (
            node_id TEXT PRIMARY KEY,
            node_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tg_server_remote_sessions (
            session_id TEXT PRIMARY KEY,
            session_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tg_server_mcp_grants (
            family_id TEXT PRIMARY KEY,
            grant_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("Failed to initialize telegram_ext schema: {e}"))?;

    let _ = conn.execute("ALTER TABLE tg_fence ADD COLUMN auth_token TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE tg_server_outbox ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute("ALTER TABLE tg_fence ADD COLUMN session_user_id INTEGER", []);
    let _ = conn.execute("ALTER TABLE tg_fence ADD COLUMN session_username TEXT", []);
    let _ = conn.execute("ALTER TABLE tg_fence ADD COLUMN session_first_name TEXT", []);
    let _ = conn.execute("ALTER TABLE tg_fence ADD COLUMN session_id TEXT", []);
    let _ = conn.execute("ALTER TABLE tg_server_outbox ADD COLUMN remote_id TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE tg_server_outbox ADD COLUMN flushed_at_ms INTEGER",
        [],
    );

    // Idempotent migration off retired legacy server endpoints to canonical desktop v1.6.6 default
    let _ = conn.execute(
        "UPDATE tg_fence
         SET server_endpoint = ?1
         WHERE server_endpoint IN ('https://telegram.fabushi.ombhrum.com', 'https://telegram-sync.ombhrum.com', '')",
        params![DEFAULT_DESKTOP_SERVER_URL],
    );

    let sets_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM tg_sticker_sets", [], |r| r.get(0))
        .unwrap_or(0);
    if sets_count == 0 {
        conn.execute_batch(
            "INSERT INTO tg_sticker_sets (set_id, title, short_name, emojis_json, installed)
             VALUES ('set_lotus', 'Zen Lotus Blessings (禅意莲花)', 'zen_lotus', '[\"🪷\",\"🙏\",\"☸️\",\"✨\",\"🕯️\"]', 1),
                    ('set_mika', 'Mika Cyber Cat (手机助手)', 'mika_cat', '[\"🐱\",\"🤖\",\"📱\",\"⚡\",\"🎯\"]', 1);",
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub(crate) fn has_authenticated_server_session(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT auth_token FROM tg_fence WHERE singleton_id = 1",
        [],
        |r| r.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
    .map(|t| !t.trim().is_empty())
    .unwrap_or(false)
}

pub(crate) fn clear_server_session_and_caches(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "UPDATE tg_fence
         SET auth_token = NULL, session_user_id = NULL, session_username = NULL,
             session_first_name = NULL, session_id = NULL, sync_pts = 1
         WHERE singleton_id = 1",
        [],
    )
    .map_err(|e| e.to_string())?;
    conn.execute_batch(
        "DELETE FROM tg_server_chats;
         DELETE FROM tg_server_pinned_messages;
         DELETE FROM tg_server_drafts;
         DELETE FROM tg_server_folders;
         DELETE FROM tg_server_bots;
         DELETE FROM tg_server_computers;
         DELETE FROM tg_server_remote_sessions;
         DELETE FROM tg_server_mcp_grants;
         DELETE FROM tg_server_outbox;",
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn clear_server_session_and_caches_if_token_matches(
    conn: &Connection,
    failed_token: &str,
) -> Result<bool, String> {
    let current: Option<String> = conn
        .query_row(
            "SELECT auth_token FROM tg_fence WHERE singleton_id = 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .flatten();
    if current.as_deref() == Some(failed_token) {
        clear_server_session_and_caches(conn)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

// ============================================================================
// Parameter & URL Helpers (Dual camelCase + snake_case)
// ============================================================================

fn param_str<'a>(params: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| {
        params
            .get(*k)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    })
}

fn param_str_raw<'a>(params: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|k| params.get(*k).and_then(Value::as_str))
}

fn param_id_string(params: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| {
        params.get(*k).and_then(|v| {
            v.as_i64()
                .map(|n| n.to_string())
                .or_else(|| v.as_u64().map(|n| n.to_string()))
                .or_else(|| {
                    v.as_str()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToString::to_string)
                })
        })
    })
}

fn param_i64(params: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|k| {
        params.get(*k).and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_u64().and_then(|u| i64::try_from(u).ok()))
                .or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
        })
    })
}

fn param_u64(params: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|k| {
        params.get(*k).and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_i64().and_then(|i| u64::try_from(i).ok()))
                .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
        })
    })
}

fn param_bool(params: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|k| params.get(*k).and_then(Value::as_bool))
}

fn param_val<'a>(params: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .find_map(|k| params.get(*k).filter(|v| !v.is_null()))
}

fn insert_opt_str(map: &mut Map<String, Value>, target: &str, params: &Value, keys: &[&str]) {
    if let Some(s) = param_str(params, keys) {
        map.insert(target.to_string(), Value::String(s.to_string()));
    }
}

fn insert_opt_i64(map: &mut Map<String, Value>, target: &str, params: &Value, keys: &[&str]) {
    if let Some(n) = param_i64(params, keys) {
        map.insert(target.to_string(), json!(n));
    }
}

fn insert_opt_u64(map: &mut Map<String, Value>, target: &str, params: &Value, keys: &[&str]) {
    if let Some(n) = param_u64(params, keys) {
        map.insert(target.to_string(), json!(n));
    }
}

fn insert_opt_bool(map: &mut Map<String, Value>, target: &str, params: &Value, keys: &[&str]) {
    if let Some(b) = param_bool(params, keys) {
        map.insert(target.to_string(), Value::Bool(b));
    }
}

fn insert_opt_val(map: &mut Map<String, Value>, target: &str, params: &Value, keys: &[&str]) {
    if let Some(v) = param_val(params, keys) {
        map.insert(target.to_string(), v.clone());
    }
}

fn is_http_401(err: &str) -> bool {
    err.starts_with("HTTP 401:")
}

// ============================================================================
// telegram.auth.* (Tier 1 — Lock-Free on Live Network Calls)
// ============================================================================

pub fn handle_auth_rpc(
    db: &Mutex<Connection>,
    submethod: &str,
    params: &Value,
) -> Result<Value, String> {
    match submethod {
        "phoneSendCode" => {
            let phone = params
                .get("phone")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "phoneSendCode requires 'phone'".to_string())?;
            let live = params
                .get("liveNetwork")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if live {
                let ep = crate::with_db(db, |conn| {
                    init_schema(conn)?;
                    Ok(resolve_server_endpoint(conn, params))
                })?;
                let val = desktop_http_json_ext(
                    "POST",
                    &format!("{ep}/api/v1/auth/phone/send-code"),
                    &HttpOptions::default(),
                    Some(json!({ "phone": phone })),
                )?;
                let pch = val
                    .get("phoneCodeHash")
                    .or_else(|| val.get("phone_code_hash"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        "SERVER_UNREACHABLE: missing phoneCodeHash in server response".to_string()
                    })?;
                return Ok(json!({
                    "phone": phone,
                    "phoneCodeHash": pch,
                    "codeLength": val.get("codeLength").or_else(|| val.get("code_length")).and_then(Value::as_u64).unwrap_or(5),
                    "deliveryType": val.get("deliveryType").or_else(|| val.get("delivery_type")).and_then(Value::as_str).unwrap_or("server_sms"),
                    "liveNetwork": true,
                }));
            }

            let now = chrono::Utc::now().timestamp_millis();
            let phone_code_hash = format!("pch_{}", uuid::Uuid::new_v4().simple());
            let expected_code = params
                .get("expectedCode")
                .and_then(Value::as_str)
                .unwrap_or("10808");
            let code_digest = hex_encode(&Sha256::digest(expected_code.as_bytes()));
            crate::with_db(db, |conn| {
                init_schema(conn)?;
                conn.execute(
                    "INSERT OR REPLACE INTO tg_auth_challenges (phone_code_hash, phone, code_sha256_hex, expires_at_ms)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![phone_code_hash, phone, code_digest, now + 300_000],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })?;
            Ok(json!({
                "phone": phone,
                "phoneCodeHash": phone_code_hash,
                "codeLength": 5,
                "deliveryType": "telegram_app_or_sms",
            }))
        }

        "phoneVerify" => {
            let phone = params
                .get("phone")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "phoneVerify requires 'phone'".to_string())?;
            let code = params
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if code.len() < 4 {
                return Err("Invalid verification code length".into());
            }
            let live = params
                .get("liveNetwork")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut issued_token: Option<String> = None;

            if live {
                let pch = params
                    .get("phoneCodeHash")
                    .or_else(|| params.get("phone_code_hash"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Live phoneVerify requires 'phoneCodeHash'".to_string())?;
                let ep = crate::with_db(db, |conn| {
                    init_schema(conn)?;
                    Ok(resolve_server_endpoint(conn, params))
                })?;
                let val = desktop_http_json_ext(
                    "POST",
                    &format!("{ep}/api/v1/auth/phone/verify"),
                    &HttpOptions::default(),
                    Some(json!({
                        "phone": phone,
                        "phone_code_hash": pch,
                        "phoneCodeHash": pch,
                        "code": code,
                    })),
                )?;
                issued_token = val
                    .get("token")
                    .or_else(|| val.get("accessToken"))
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
            }

            let now = chrono::Utc::now().timestamp_millis();
            crate::with_db(db, |conn| {
                init_schema(conn)?;
                if !live {
                    if let Some(pch) = params
                        .get("phoneCodeHash")
                        .or_else(|| params.get("phone_code_hash"))
                        .and_then(Value::as_str)
                    {
                        if let Some((stored_digest, exp_ms)) = conn
                            .query_row(
                                "SELECT code_sha256_hex, expires_at_ms FROM tg_auth_challenges WHERE phone_code_hash = ?1",
                                params![pch],
                                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
                            )
                            .optional()
                            .map_err(|e| e.to_string())?
                        {
                            if now > exp_ms {
                                return Err("Verification challenge expired".into());
                            }
                            let actual_digest = hex_encode(&Sha256::digest(code.as_bytes()));
                            if actual_digest != stored_digest {
                                return Err("INVALID_CODE: verification code mismatch".into());
                            }
                            let _ = conn.execute(
                                "DELETE FROM tg_auth_challenges WHERE phone_code_hash = ?1",
                                params![pch],
                            );
                        }
                    }
                }

                let acc_id = format!("acc_{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
                let username = params
                    .get("username")
                    .and_then(Value::as_str)
                    .unwrap_or("telegram_user");
                conn.execute("UPDATE tg_accounts SET is_active = 0", [])
                    .map_err(|e| e.to_string())?;
                conn.execute(
                    "INSERT OR REPLACE INTO tg_accounts (account_id, phone, username, display_name, is_active, created_at_ms)
                     VALUES (?1, ?2, ?3, ?4, 1, ?5)",
                    params![acc_id, phone, username, username, now],
                )
                .map_err(|e| e.to_string())?;
                conn.execute(
                    "UPDATE tg_fence SET generation = generation + 1, auth_token = COALESCE(?1, auth_token) WHERE singleton_id = 1",
                    params![issued_token],
                )
                .map_err(|e| e.to_string())?;
                let gen: i64 = conn
                    .query_row(
                        "SELECT generation FROM tg_fence WHERE singleton_id = 1",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or(1);
                Ok(json!({
                    "verified": true,
                    "accountId": acc_id,
                    "phone": phone,
                    "generation": gen,
                    "hasSessionToken": issued_token.is_some(),
                }))
            })
        }

        _ => crate::with_db(db, |conn| handle_auth_rpc_local(conn, submethod, params)),
    }
}

fn handle_auth_rpc_local(
    conn: &Connection,
    submethod: &str,
    params: &Value,
) -> Result<Value, String> {
    init_schema(conn)?;
    let now = chrono::Utc::now().timestamp_millis();

    match submethod {
        "qrPairInit" => {
            let token = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(uuid::Uuid::new_v4().as_bytes());
            Ok(json!({
                "qrToken": token,
                "qrUri": format!("tg://login?token={token}"),
                "expiresInSeconds": 30,
            }))
        }

        "qrPairPoll" => Err("QR_PAIR_UNAVAILABLE: QR login requires server session".into()),

        "twoFactorStatus" => {
            let (hash_opt, hint): (Option<String>, Option<String>) = conn
                .query_row(
                    "SELECT two_factor_hash_hex, two_factor_hint FROM tg_fence WHERE singleton_id = 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap_or((None, None));
            Ok(json!({
                "enabled": hash_opt.is_some(),
                "hint": hint.unwrap_or_default(),
            }))
        }

        "twoFactorSet" => {
            let password = params
                .get("password")
                .and_then(Value::as_str)
                .ok_or_else(|| "twoFactorSet requires 'password'".to_string())?;
            let hint = params.get("hint").and_then(Value::as_str).unwrap_or("");
            let (salt, hash) = hash_secret_argon2id(password)?;
            conn.execute(
                "UPDATE tg_fence SET two_factor_salt_hex = ?1, two_factor_hash_hex = ?2, two_factor_hint = ?3 WHERE singleton_id = 1",
                params![salt, hash, hint],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "enabled": true, "hint": hint, "kdf": "Argon2id" }))
        }

        "twoFactorVerify" => {
            let password = params
                .get("password")
                .and_then(Value::as_str)
                .unwrap_or("");
            let (salt, hash): (Option<String>, Option<String>) = conn
                .query_row(
                    "SELECT two_factor_salt_hex, two_factor_hash_hex FROM tg_fence WHERE singleton_id = 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(|e| e.to_string())?;
            let verified = match (salt, hash) {
                (Some(s), Some(h)) => verify_secret_argon2id(password, &s, &h)?,
                _ => false,
            };
            Ok(json!({ "verified": verified }))
        }

        "passkeyList" => {
            let mut stmt = conn
                .prepare("SELECT credential_id, label, public_key_hex, created_at_ms FROM tg_passkeys ORDER BY created_at_ms DESC")
                .map_err(|e| e.to_string())?;
            let list = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "credentialId": r.get::<_, String>(0)?,
                        "label": r.get::<_, String>(1)?,
                        "publicKeyHex": r.get::<_, String>(2)?,
                        "createdAtMs": r.get::<_, i64>(3)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "passkeys": list }))
        }

        "passkeyRegister" => {
            let label = params
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("Android Biometric Passkey");
            let cred_id = format!("pk_{}", uuid::Uuid::new_v4().simple());
            let mut pk = [0u8; 32];
            OsRng.fill_bytes(&mut pk);
            let pk_hex = hex_encode(&pk);
            conn.execute(
                "INSERT INTO tg_passkeys (credential_id, label, public_key_hex, created_at_ms) VALUES (?1, ?2, ?3, ?4)",
                params![cred_id, label, pk_hex, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "credentialId": cred_id, "label": label, "publicKeyHex": pk_hex }))
        }

        "accountsList" => {
            let gen: i64 = conn
                .query_row(
                    "SELECT generation FROM tg_fence WHERE singleton_id = 1",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(1);
            let mut stmt = conn
                .prepare("SELECT account_id, phone, username, display_name, is_active FROM tg_accounts ORDER BY created_at_ms ASC")
                .map_err(|e| e.to_string())?;
            let accounts = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "accountId": r.get::<_, String>(0)?,
                        "phone": r.get::<_, String>(1)?,
                        "username": r.get::<_, String>(2)?,
                        "displayName": r.get::<_, String>(3)?,
                        "active": r.get::<_, i64>(4)? != 0,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "generation": gen,
                "accounts": accounts,
            }))
        }

        "accountAdd" => {
            let phone = params
                .get("phone")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            let username = params
                .get("username")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("account");
            let display = params
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or(username);
            let acc_id = params
                .get("accountId")
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .unwrap_or_else(|| {
                    format!("acc_{}", &uuid::Uuid::new_v4().simple().to_string()[..10])
                });
            conn.execute(
                "INSERT OR REPLACE INTO tg_accounts (account_id, phone, username, display_name, is_active, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, 0, ?5)",
                params![acc_id, phone, username, display, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "accountId": acc_id, "added": true }))
        }

        "accountSwitch" => {
            let target_id = params
                .get("accountId")
                .and_then(Value::as_str)
                .ok_or_else(|| "accountSwitch requires 'accountId'".to_string())?;
            if let Some(expected_gen) = params.get("expectedGeneration").and_then(Value::as_i64) {
                let cur_gen: i64 = conn
                    .query_row(
                        "SELECT generation FROM tg_fence WHERE singleton_id = 1",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or(1);
                if expected_gen != cur_gen {
                    return Err(format!(
                        "GenerationFence mismatch: expected {expected_gen}, active {cur_gen}"
                    ));
                }
            }
            let exists: Option<String> = conn
                .query_row(
                    "SELECT account_id FROM tg_accounts WHERE account_id = ?1",
                    params![target_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if exists.is_none() {
                return Err(format!("Account '{target_id}' not found"));
            }

            conn.execute("UPDATE tg_accounts SET is_active = 0", [])
                .map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE tg_accounts SET is_active = 1 WHERE account_id = ?1",
                params![target_id],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE tg_fence SET generation = generation + 1 WHERE singleton_id = 1",
                [],
            )
            .map_err(|e| e.to_string())?;
            let new_gen: i64 = conn
                .query_row(
                    "SELECT generation FROM tg_fence WHERE singleton_id = 1",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(2);
            Ok(json!({
                "activeAccountId": target_id,
                "generation": new_gen,
            }))
        }

        "passcodeSet" => {
            let passcode = params
                .get("passcode")
                .and_then(Value::as_str)
                .ok_or_else(|| "passcodeSet requires 'passcode'".to_string())?;
            if passcode.chars().count() < 4 {
                return Err("Passcode must be at least 4 characters".into());
            }
            let (salt, hash) = hash_secret_argon2id(passcode)?;
            conn.execute(
                "UPDATE tg_fence SET passcode_salt_hex = ?1, passcode_hash_hex = ?2 WHERE singleton_id = 1",
                params![salt, hash],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "passcodeEnabled": true, "kdf": "Argon2id" }))
        }

        "passcodeVerify" => {
            let passcode = params
                .get("passcode")
                .and_then(Value::as_str)
                .unwrap_or("");
            let (salt, hash): (Option<String>, Option<String>) = conn
                .query_row(
                    "SELECT passcode_salt_hex, passcode_hash_hex FROM tg_fence WHERE singleton_id = 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(|e| e.to_string())?;
            let valid = match (salt, hash) {
                (Some(s), Some(h)) => verify_secret_argon2id(passcode, &s, &h)?,
                _ => false,
            };
            Ok(json!({ "valid": valid }))
        }

        other => Err(format!("Unsupported telegram.auth method: {other}")),
    }
}

// ============================================================================
// telegram.messaging.* (Tier 2 — Pure Local SQLite)
// ============================================================================

pub fn handle_messaging_ext_rpc(
    conn: &Connection,
    submethod: &str,
    params: &Value,
) -> Result<Value, String> {
    init_schema(conn)?;
    let now = chrono::Utc::now().timestamp_millis();

    match submethod {
        "encodeWaveform" => rpc_encode_waveform(params),
        "decodeWaveform" => rpc_decode_waveform(params),

        "searchMessages" | "searchGlobal" | "searchInChat" => {
            crate::messaging::search_messages(conn, params)
        }

        "listTopics" => {
            let conv_id = param_id_string(params, &["conversationId", "chatId", "chat_id"])
                .unwrap_or_default();
            let mut stmt = conn
                .prepare(
                    "SELECT topic_id, conversation_id, title, icon_emoji, closed, created_at_ms
                     FROM tg_topics WHERE conversation_id = ?1 ORDER BY created_at_ms ASC",
                )
                .map_err(|e| e.to_string())?;
            let topics = stmt
                .query_map(params![conv_id], |r| {
                    Ok(json!({
                        "topicId": r.get::<_, String>(0)?,
                        "conversationId": r.get::<_, String>(1)?,
                        "title": r.get::<_, String>(2)?,
                        "iconEmoji": r.get::<_, String>(3)?,
                        "closed": r.get::<_, i64>(4)? != 0,
                        "createdAtMs": r.get::<_, i64>(5)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "conversationId": conv_id, "topics": topics }))
        }

        "createTopic" => {
            let conv_id = param_id_string(params, &["conversationId", "chatId", "chat_id"])
                .ok_or_else(|| "createTopic requires 'conversationId'".to_string())?;
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .ok_or_else(|| "createTopic requires 'title'".to_string())?;
            let emoji = params
                .get("iconEmoji")
                .and_then(Value::as_str)
                .unwrap_or("💬");
            let topic_id = format!("topic_{}", uuid::Uuid::new_v4().simple());
            conn.execute(
                "INSERT INTO tg_topics (topic_id, conversation_id, title, icon_emoji, closed, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, 0, ?5)",
                params![topic_id, conv_id, title, emoji, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "topicId": topic_id,
                "conversationId": conv_id,
                "title": title,
                "iconEmoji": emoji,
            }))
        }

        "listStories" => {
            let mut stmt = conn
                .prepare(
                    "SELECT story_id, author_id, caption, media_kind, privacy, views_count, expires_at_ms, created_at_ms
                     FROM tg_stories WHERE expires_at_ms > ?1 ORDER BY created_at_ms DESC",
                )
                .map_err(|e| e.to_string())?;
            let stories = stmt
                .query_map(params![now], |r| {
                    Ok(json!({
                        "storyId": r.get::<_, String>(0)?,
                        "authorId": r.get::<_, String>(1)?,
                        "caption": r.get::<_, String>(2)?,
                        "mediaKind": r.get::<_, String>(3)?,
                        "privacy": r.get::<_, String>(4)?,
                        "viewsCount": r.get::<_, i64>(5)?,
                        "expiresAtMs": r.get::<_, i64>(6)?,
                        "createdAtMs": r.get::<_, i64>(7)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "stories": stories }))
        }

        "createStory" => {
            let author_id = params
                .get("authorId")
                .and_then(Value::as_str)
                .unwrap_or("human:account:local-default");
            let caption = params
                .get("caption")
                .and_then(Value::as_str)
                .unwrap_or("");
            let media_kind = params
                .get("mediaKind")
                .and_then(Value::as_str)
                .unwrap_or("photo");
            let privacy = params
                .get("privacy")
                .and_then(Value::as_str)
                .unwrap_or("contacts");
            let story_id = format!("story_{}", uuid::Uuid::new_v4().simple());
            let expires_at = now + 86_400_000;
            conn.execute(
                "INSERT INTO tg_stories (story_id, author_id, caption, media_kind, privacy, views_count, expires_at_ms, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7)",
                params![story_id, author_id, caption, media_kind, privacy, expires_at, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "storyId": story_id,
                "expiresAtMs": expires_at,
                "privacy": privacy,
            }))
        }

        "viewStory" => {
            let story_id = params
                .get("storyId")
                .and_then(Value::as_str)
                .ok_or_else(|| "viewStory requires 'storyId'".to_string())?;
            conn.execute(
                "UPDATE tg_stories SET views_count = views_count + 1 WHERE story_id = ?1",
                params![story_id],
            )
            .map_err(|e| e.to_string())?;
            let views: i64 = conn
                .query_row(
                    "SELECT views_count FROM tg_stories WHERE story_id = ?1",
                    params![story_id],
                    |r| r.get(0),
                )
                .unwrap_or(1);
            Ok(json!({ "storyId": story_id, "viewsCount": views }))
        }

        "listStickerSets" => {
            let mut stmt = conn
                .prepare("SELECT set_id, title, short_name, emojis_json, installed FROM tg_sticker_sets")
                .map_err(|e| e.to_string())?;
            let sets = stmt
                .query_map([], |r| {
                    let em_raw: String = r.get(3)?;
                    let emojis: Value = serde_json::from_str(&em_raw).unwrap_or(json!([]));
                    Ok(json!({
                        "setId": r.get::<_, String>(0)?,
                        "title": r.get::<_, String>(1)?,
                        "shortName": r.get::<_, String>(2)?,
                        "emojis": emojis,
                        "installed": r.get::<_, i64>(4)? != 0,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "stickerSets": sets }))
        }

        "installStickerSet" => {
            let set_id = params
                .get("setId")
                .and_then(Value::as_str)
                .unwrap_or("set_custom");
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Custom Sticker Pack");
            let short_name = params
                .get("shortName")
                .and_then(Value::as_str)
                .unwrap_or(set_id);
            conn.execute(
                "INSERT OR REPLACE INTO tg_sticker_sets (set_id, title, short_name, emojis_json, installed)
                 VALUES (?1, ?2, ?3, '[\"🔥\",\"💎\",\"🚀\"]', 1)",
                params![set_id, title, short_name],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "setId": set_id, "installed": true }))
        }

        "searchGifs" => {
            let q = params.get("query").and_then(Value::as_str).unwrap_or("");
            Ok(json!({
                "query": q,
                "gifs": [],
                "unavailable": true,
            }))
        }

        "channelStats" => {
            let conv_id = param_id_string(params, &["conversationId", "chatId", "chat_id"])
                .ok_or_else(|| "channelStats requires 'conversationId' or 'chatId'".to_string())?;
            let msg_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM msg_messages WHERE conversation_id = ?1",
                    params![&conv_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let pinned_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM msg_messages
                     WHERE conversation_id = ?1
                       AND json_extract(message_json, '$.pinned') = 1",
                    params![&conv_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let topic_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM tg_topics WHERE conversation_id = ?1",
                    params![&conv_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let participants_count: usize = conn
                .query_row(
                    "SELECT conversation_json FROM msg_conversations WHERE conversation_id = ?1",
                    params![&conv_id],
                    |r| r.get::<_, String>(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                .and_then(|v| v.get("participants").and_then(Value::as_array).map(Vec::len))
                .unwrap_or(0);
            Ok(json!({
                "conversationId": conv_id,
                "subscribers": participants_count,
                "totalMessages": msg_count,
                "pinnedMessages": pinned_count,
                "topicsCount": topic_count,
                "source": "sqlite_local",
            }))
        }

        "exportChat" => crate::messaging::export_chat_history(conn, params),

        "inlineQuery" => {
            let bot = params
                .get("botUsername")
                .and_then(Value::as_str)
                .unwrap_or("");
            Ok(json!({
                "botUsername": bot,
                "results": [],
                "unavailable": true,
            }))
        }

        "callbackQuery" => Err(
            "CALLBACK_QUERY_REQUIRES_SERVER: use telegram.server.sendCallbackQuery".to_string(),
        ),

        other => Err(format!("Unsupported telegram.messaging method: {other}")),
    }
}

// ============================================================================
// telegram.calls.* (Tier 2 — VAD Kept, Unbacked VoIP Calls Fail Closed)
// ============================================================================

pub fn handle_calls_rpc(
    conn: &Connection,
    vad: &Mutex<VadBargeInDetector>,
    submethod: &str,
    params: &Value,
) -> Result<Value, String> {
    init_schema(conn)?;

    match submethod {
        "vadFeedPcm" => {
            let tts_playing = params
                .get("ttsPlaying")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let samples: Vec<i16> =
                if let Some(arr) = params.get("pcmSamples").and_then(Value::as_array) {
                    arr.iter()
                        .map(|v| {
                            v.as_i64()
                                .unwrap_or(0)
                                .clamp(i16::MIN as i64, i16::MAX as i64)
                                as i16
                        })
                        .collect()
                } else if let Some(b64) = params.get("pcmBase64").and_then(Value::as_str) {
                    let raw = base64::engine::general_purpose::STANDARD
                        .decode(b64.trim())
                        .map_err(|e| format!("Invalid pcmBase64: {e}"))?;
                    pcm_bytes_to_i16(&raw)
                } else {
                    Vec::new()
                };

            let mut guard = vad.lock().unwrap_or_else(|e| e.into_inner());
            if params
                .get("reset")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                guard.reset();
            }
            let report = guard.feed_pcm(&samples, tts_playing);
            Ok(json!({
                "framesEvaluated": report.frames_evaluated,
                "speechDetected": report.speech_detected,
                "bargeInTriggered": report.barge_in_triggered,
                "ttsInterrupted": report.tts_interrupted,
                "consecutiveSpeechFrames": report.consecutive_speech_frames,
                "detectionLatencyMs": report.detection_latency_ms,
                "peakRms": report.peak_rms,
            }))
        }

        "startCall" | "joinGroupCall" | "signal" | "toggleMute" | "toggleVideo"
        | "toggleScreenShare" | "endCall" => Err(format!(
            "CALL_TRANSPORT_UNAVAILABLE: Native WebRTC VoIP call transport is not available for '{submethod}'"
        )),

        "history" => {
            let mut stmt = conn
                .prepare(
                    "SELECT call_id, peer_id, is_group, state, emoji_fingerprint, started_at_ms, ended_at_ms
                     FROM tg_calls ORDER BY started_at_ms DESC LIMIT 50",
                )
                .map_err(|e| e.to_string())?;
            let calls = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "callId": r.get::<_, String>(0)?,
                        "peerId": r.get::<_, String>(1)?,
                        "isGroup": r.get::<_, i64>(2)? != 0,
                        "state": r.get::<_, String>(3)?,
                        "emojiFingerprint": r.get::<_, String>(4)?,
                        "startedAtMs": r.get::<_, i64>(5)?,
                        "endedAtMs": r.get::<_, Option<i64>>(6)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "calls": calls }))
        }

        other => Err(format!("Unsupported telegram.calls method: {other}")),
    }
}

// ============================================================================
// telegram.server.* (Tier 1 — Lock-Free Full Desktop v1.6.6 /api/v1/* Client)
// ============================================================================

pub fn handle_server_rpc(
    db: &Mutex<Connection>,
    submethod: &str,
    params: &Value,
) -> Result<Value, String> {
    match submethod {
        // ------------------------------------------------------------------
        // 1–17: Endpoint, Auth, Sessions & Profile
        // ------------------------------------------------------------------
        "configureEndpoint" | "setEndpoint" => crate::with_db(db, |conn| {
            init_schema(conn)?;
            let url = param_str(params, &["endpoint", "url", "serverUrl"])
                .ok_or_else(|| "configureEndpoint requires 'endpoint'".to_string())?;
            let token = param_str(params, &["authToken", "token"]);
            let clean = url.trim_end_matches('/');
            conn.execute(
                "UPDATE tg_fence SET server_endpoint = ?1, auth_token = COALESCE(?2, auth_token) WHERE singleton_id = 1",
                params![clean, token],
            )
            .map_err(|e| e.to_string())?;
            let has_token = has_authenticated_server_session(conn);
            Ok(json!({
                "endpoint": clean,
                "hasAuthToken": has_token,
                "saved": true,
            }))
        }),

        "status" => crate::with_db(db, |conn| {
            init_schema(conn)?;
            let (ep, pts, tok): (String, i64, Option<String>) = conn
                .query_row(
                    "SELECT server_endpoint, sync_pts, auth_token FROM tg_fence WHERE singleton_id = 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap_or_else(|_| (DEFAULT_DESKTOP_SERVER_URL.into(), 1, None));
            let pending_outbox: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM tg_server_outbox WHERE flushed = 0",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            Ok(json!({
                "endpoint": ep,
                "pts": pts,
                "hasAuthToken": tok.as_deref().map(|s| !s.is_empty()).unwrap_or(false),
                "pendingOutboxCount": pending_outbox,
                "transport": "ureq-rustls-https",
            }))
        }),

        "session" => crate::with_db(db, |conn| {
            init_schema(conn)?;
            let row: (
                String,
                Option<String>,
                Option<i64>,
                Option<String>,
                Option<String>,
                Option<String>,
            ) = conn
                .query_row(
                    "SELECT server_endpoint, auth_token, session_user_id, session_username, session_first_name, session_id
                     FROM tg_fence WHERE singleton_id = 1",
                    [],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                        ))
                    },
                )
                .unwrap_or_else(|_| {
                    (
                        DEFAULT_DESKTOP_SERVER_URL.into(),
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                });
            let has_token = row.1.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
            Ok(json!({
                "authenticated": has_token,
                "endpoint": row.0,
                "token": row.1.unwrap_or_default(),
                "userId": row.2,
                "username": row.3,
                "firstName": row.4,
                "sessionId": row.5,
            }))
        }),

        "login" | "register" | "registerEmail" => {
            let ep = crate::with_db(db, |conn| {
                init_schema(conn)?;
                Ok(resolve_server_endpoint(conn, params))
            })?;
            let username = param_str(params, &["username"])
                .ok_or_else(|| format!("{submethod} requires 'username'"))?;
            let password = params
                .get("password")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("{submethod} requires 'password'"))?;
            let device_name =
                param_str(params, &["deviceName", "device_name"]).unwrap_or("Fabushi Android");
            let first_name = param_str(params, &["firstName", "first_name"]).unwrap_or(username);

            let (path, body) = match submethod {
                "login" => (
                    "/api/v1/auth/login",
                    json!({
                        "username": username,
                        "password": password,
                        "device_name": device_name,
                    }),
                ),
                "register" => (
                    "/api/v1/auth/register",
                    json!({
                        "username": username,
                        "password": password,
                        "first_name": first_name,
                        "device_name": device_name,
                    }),
                ),
                _ => {
                    let email = param_str(params, &["email"])
                        .ok_or_else(|| "registerEmail requires 'email'".to_string())?;
                    let code = param_str(params, &["code"])
                        .ok_or_else(|| "registerEmail requires 'code'".to_string())?;
                    (
                        "/api/v1/auth/email/register",
                        json!({
                            "email": email,
                            "code": code,
                            "username": username,
                            "password": password,
                            "first_name": first_name,
                        }),
                    )
                }
            };

            let val = desktop_http_json_ext(
                "POST",
                &format!("{ep}{path}"),
                &HttpOptions::default(),
                Some(body),
            )?;
            crate::with_db(db, |conn| persist_server_session(conn, &ep, &val))?;
            Ok(val)
        }

        "sendEmailCode" => {
            let ep = crate::with_db(db, |conn| {
                init_schema(conn)?;
                Ok(resolve_server_endpoint(conn, params))
            })?;
            let email = param_str(params, &["email"])
                .ok_or_else(|| "sendEmailCode requires 'email'".to_string())?;
            desktop_http_json_ext(
                "POST",
                &format!("{ep}/api/v1/auth/email/send-code"),
                &HttpOptions::default(),
                Some(json!({ "email": email })),
            )
        }

        "alipayStart" => {
            let ep = crate::with_db(db, |conn| {
                init_schema(conn)?;
                Ok(resolve_server_endpoint(conn, params))
            })?;
            desktop_http_json_ext(
                "POST",
                &format!("{ep}/api/v1/auth/alipay/start"),
                &HttpOptions::default(),
                Some(json!({})),
            )
        }

        "alipayPoll" => {
            let ep = crate::with_db(db, |conn| {
                init_schema(conn)?;
                Ok(resolve_server_endpoint(conn, params))
            })?;
            let attempt_id = param_str(params, &["attemptId", "attempt_id"])
                .ok_or_else(|| "alipayPoll requires 'attemptId'".to_string())?;
            let encoded = url_encode_path_segment(attempt_id);
            let val = desktop_http_json_ext(
                "GET",
                &format!("{ep}/api/v1/auth/alipay/poll/{encoded}"),
                &HttpOptions::default(),
                None,
            )?;
            let status = val.get("status").and_then(Value::as_str).unwrap_or("");
            if status == "complete" || status == "completed" {
                let session_val = val.get("session").unwrap_or(&val);
                crate::with_db(db, |conn| persist_server_session(conn, &ep, session_val))?;
            }
            Ok(val)
        }

        "me" => {
            let val = authed_http_json(db, params, "GET", "/api/v1/auth/me", None, None, 15)?;
            crate::with_db(db, |conn| {
                if let Some(uid) = val.get("user_id").and_then(Value::as_i64) {
                    let uname = val.get("username").and_then(Value::as_str);
                    let fname = val.get("first_name").and_then(Value::as_str);
                    let sid = val.get("session_id").and_then(Value::as_str);
                    let _ = conn.execute(
                        "UPDATE tg_fence
                         SET session_user_id = ?1,
                             session_username = COALESCE(?2, session_username),
                             session_first_name = COALESCE(?3, session_first_name),
                             session_id = COALESCE(?4, session_id)
                         WHERE singleton_id = 1",
                        params![uid, uname, fname, sid],
                    );
                }
                Ok(())
            })?;
            Ok(val)
        }

        "logout" => crate::with_db(db, |conn| {
            init_schema(conn)?;
            clear_server_session_and_caches(conn)?;
            Ok(json!({ "loggedOut": true }))
        }),

        "listSessions" => {
            let val = authed_http_json(db, params, "GET", "/api/v1/auth/sessions", None, None, 15)?;
            if val.is_array() {
                Ok(json!({ "sessions": val }))
            } else {
                Ok(val)
            }
        }

        "revokeSession" => {
            let sid = param_id_string(params, &["sessionId", "session_id", "id"])
                .ok_or_else(|| "revokeSession requires 'sessionId'".to_string())?;
            let encoded = url_encode_path_segment(&sid);
            authed_http_json(
                db,
                params,
                "DELETE",
                &format!("/api/v1/auth/sessions/{encoded}"),
                None,
                None,
                15,
            )
        }

        "revokeOtherSessions" => authed_http_json(
            db,
            params,
            "DELETE",
            "/api/v1/auth/sessions/others",
            None,
            None,
            15,
        ),

        "updateProfile" => {
            let mut body = Map::new();
            insert_opt_str(&mut body, "first_name", params, &["firstName", "first_name"]);
            insert_opt_str(&mut body, "last_name", params, &["lastName", "last_name"]);
            if let Some(bio) = param_str_raw(params, &["bio"]) {
                body.insert("bio".into(), Value::String(bio.to_string()));
            }
            insert_opt_str(&mut body, "username", params, &["username"]);
            insert_opt_str(&mut body, "phone", params, &["phone"]);
            insert_opt_str(
                &mut body,
                "avatar_media_id",
                params,
                &[
                    "avatarMediaId",
                    "avatar_media_id",
                    "photoFileId",
                    "photo_file_id",
                ],
            );
            insert_opt_str(
                &mut body,
                "emoji_status",
                params,
                &["emojiStatus", "emoji_status"],
            );

            let val = authed_http_json(
                db,
                params,
                "PUT",
                "/api/v1/auth/profile",
                Some(Value::Object(body)),
                None,
                15,
            )?;
            crate::with_db(db, |conn| {
                let uname = val
                    .get("username")
                    .or_else(|| val.pointer("/user/username"))
                    .and_then(Value::as_str);
                let fname = val
                    .get("first_name")
                    .or_else(|| val.pointer("/user/first_name"))
                    .and_then(Value::as_str);
                let _ = conn.execute(
                    "UPDATE tg_fence
                     SET session_username = COALESCE(?1, session_username),
                         session_first_name = COALESCE(?2, session_first_name)
                     WHERE singleton_id = 1",
                    params![uname, fname],
                );
                Ok(())
            })?;
            Ok(val)
        }

        "getUserSettings" => authed_http_json(
            db,
            params,
            "GET",
            "/api/v1/users/me/settings",
            None,
            None,
            15,
        ),

        "updateUserSettings" => {
            let verb = if param_bool(params, &["partial"]).unwrap_or(false) {
                "PATCH"
            } else {
                "PUT"
            };
            let mut body = params
                .get("settings")
                .and_then(Value::as_object)
                .cloned()
                .or_else(|| params.as_object().cloned())
                .unwrap_or_default();
            body.remove("partial");
            body.remove("endpoint");
            body.remove("serverUrl");
            body.remove("token");
            body.remove("authToken");
            authed_http_json(
                db,
                params,
                verb,
                "/api/v1/users/me/settings",
                Some(Value::Object(body)),
                None,
                15,
            )
        }

        // ------------------------------------------------------------------
        // 18–54: Chats, Pinning, Rich Messaging, Drafts, Folders, Polls,
        //        Contacts, Media, Search, ChannelStats, Outbox & Sync
        // ------------------------------------------------------------------
        "listChats" => {
            let val = authed_http_json(db, params, "GET", "/api/v1/chats", None, None, 15)?;
            let chats_arr: Vec<Value> = if let Some(arr) = val.as_array() {
                arr.clone()
            } else if let Some(arr) = val.get("chats").and_then(Value::as_array) {
                arr.clone()
            } else {
                Vec::new()
            };
            crate::with_db(db, |conn| {
                let now = chrono::Utc::now().timestamp_millis();
                conn.execute("DELETE FROM tg_server_chats", [])
                    .map_err(|e| e.to_string())?;
                for chat in &chats_arr {
                    upsert_server_chat_cache(conn, chat, now)?;
                }
                Ok(())
            })?;
            if val.is_array() {
                let total = chats_arr.len();
                Ok(json!({ "chats": chats_arr, "total": total }))
            } else {
                Ok(val)
            }
        }

        "createChat" => {
            let title = param_str(params, &["title"])
                .ok_or_else(|| "createChat requires 'title'".to_string())?;
            let mut body = Map::new();
            body.insert("title".into(), Value::String(title.to_string()));
            if let Some(kind) = param_str(params, &["kind"]) {
                body.insert("kind".into(), Value::String(kind.to_string()));
            } else if params.get("chatType").is_none() && params.get("chat_type").is_none() {
                body.insert("kind".into(), Value::String("group".to_string()));
            }
            insert_opt_val(&mut body, "chat_type", params, &["chatType", "chat_type"]);
            insert_opt_i64(
                &mut body,
                "peer_user_id",
                params,
                &["peerUserId", "peer_user_id"],
            );
            insert_opt_str(&mut body, "about", params, &["about"]);
            insert_opt_str(&mut body, "username", params, &["username"]);
            insert_opt_bool(&mut body, "is_forum", params, &["isForum", "is_forum"]);
            if let Some(m) = param_val(params, &["memberIds", "member_ids"]) {
                body.insert("member_ids".into(), m.clone());
            } else {
                body.insert("member_ids".into(), json!([]));
            }
            insert_opt_val(
                &mut body,
                "member_usernames",
                params,
                &["memberUsernames", "member_usernames"],
            );

            let val = authed_http_json(
                db,
                params,
                "POST",
                "/api/v1/chats/create",
                Some(Value::Object(body)),
                None,
                15,
            )?;
            crate::with_db(db, |conn| {
                let now = chrono::Utc::now().timestamp_millis();
                let chat_obj = val.get("chat").unwrap_or(&val);
                if extract_chat_id(chat_obj).is_some() {
                    let _ = upsert_server_chat_cache(conn, chat_obj, now);
                }
                Ok(())
            })?;
            Ok(val)
        }

        "pinChat" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "pinChat requires numeric 'chatId'".to_string())?;
            let pinned = param_bool(params, &["pinned"]).unwrap_or(true);
            let val = authed_http_json(
                db,
                params,
                "POST",
                &format!("/api/v1/chats/{chat_id}/pin"),
                Some(json!({ "pinned": pinned })),
                None,
                15,
            )?;
            crate::with_db(db, |conn| {
                let eff_pinned = val
                    .get("is_pinned")
                    .and_then(Value::as_bool)
                    .unwrap_or(pinned);
                let eff_muted = val.get("is_muted").and_then(Value::as_bool);
                let eff_mute_until = val.get("mute_until_ms").map(Value::as_i64);
                let eff_archived = val.get("is_archived").and_then(Value::as_bool);
                update_server_chat_flags_cache(
                    conn,
                    chat_id,
                    Some(eff_pinned),
                    eff_muted,
                    eff_mute_until,
                    eff_archived,
                )
            })?;
            Ok(val)
        }

        "muteChat" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "muteChat requires numeric 'chatId'".to_string())?;
            let muted = param_bool(params, &["muted"]).unwrap_or(true);
            let mut body = Map::new();
            body.insert("muted".into(), Value::Bool(muted));
            insert_opt_i64(
                &mut body,
                "mute_until_ms",
                params,
                &["muteUntilMs", "mute_until_ms"],
            );
            let val = authed_http_json(
                db,
                params,
                "POST",
                &format!("/api/v1/chats/{chat_id}/mute"),
                Some(Value::Object(body)),
                None,
                15,
            )?;
            crate::with_db(db, |conn| {
                let eff_muted = val
                    .get("is_muted")
                    .and_then(Value::as_bool)
                    .unwrap_or(muted);
                let eff_mute_until = if !eff_muted {
                    Some(None)
                } else {
                    val.get("mute_until_ms").map(Value::as_i64)
                };
                let eff_pinned = val.get("is_pinned").and_then(Value::as_bool);
                let eff_archived = val.get("is_archived").and_then(Value::as_bool);
                update_server_chat_flags_cache(
                    conn,
                    chat_id,
                    eff_pinned,
                    Some(eff_muted),
                    eff_mute_until,
                    eff_archived,
                )
            })?;
            Ok(val)
        }

        "archiveChat" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "archiveChat requires numeric 'chatId'".to_string())?;
            let archived = param_bool(params, &["archived"]).unwrap_or(true);
            let val = authed_http_json(
                db,
                params,
                "POST",
                &format!("/api/v1/chats/{chat_id}/archive"),
                Some(json!({ "archived": archived })),
                None,
                15,
            )?;
            crate::with_db(db, |conn| {
                let eff_archived = val
                    .get("is_archived")
                    .and_then(Value::as_bool)
                    .unwrap_or(archived);
                let eff_pinned = val.get("is_pinned").and_then(Value::as_bool);
                let eff_muted = val.get("is_muted").and_then(Value::as_bool);
                let eff_mute_until = val.get("mute_until_ms").map(Value::as_i64);
                update_server_chat_flags_cache(
                    conn,
                    chat_id,
                    eff_pinned,
                    eff_muted,
                    eff_mute_until,
                    Some(eff_archived),
                )
            })?;
            Ok(val)
        }

        "listPinnedMessages" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "listPinnedMessages requires numeric 'chatId'".to_string())?;
            let val = authed_http_json(
                db,
                params,
                "GET",
                &format!("/api/v1/chats/{chat_id}/pins"),
                None,
                None,
                15,
            )?;
            crate::with_db(db, |conn| sync_pinned_messages_cache(conn, chat_id, &val))?;
            Ok(val)
        }

        "pinMessage" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "pinMessage requires numeric 'chatId'".to_string())?;
            let message_id = param_i64(params, &["messageId", "message_id"])
                .filter(|&id| id > 0)
                .ok_or_else(|| "pinMessage requires positive 'messageId'".to_string())?;
            let val = authed_http_json(
                db,
                params,
                "POST",
                &format!("/api/v1/chats/{chat_id}/pins"),
                Some(json!({ "message_id": message_id })),
                None,
                15,
            )?;
            crate::with_db(db, |conn| sync_pinned_messages_cache(conn, chat_id, &val))?;
            Ok(val)
        }

        "unpinMessage" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "unpinMessage requires numeric 'chatId'".to_string())?;
            let message_id = param_i64(params, &["messageId", "message_id"]);
            let path = match message_id {
                Some(mid) => format!("/api/v1/chats/{chat_id}/pins/{mid}"),
                None => format!("/api/v1/chats/{chat_id}/pins"),
            };
            let val = authed_http_json(db, params, "DELETE", &path, None, None, 15)?;
            crate::with_db(db, |conn| {
                if val.get("pinned_message_ids").is_some() || val.get("pins").is_some() {
                    sync_pinned_messages_cache(conn, chat_id, &val)?;
                } else if let Some(mid) = message_id {
                    conn.execute(
                        "DELETE FROM tg_server_pinned_messages WHERE chat_id = ?1 AND message_id = ?2",
                        params![chat_id, mid],
                    )
                    .map_err(|e| e.to_string())?;
                } else {
                    conn.execute(
                        "DELETE FROM tg_server_pinned_messages WHERE chat_id = ?1",
                        params![chat_id],
                    )
                    .map_err(|e| e.to_string())?;
                }
                Ok(())
            })?;
            Ok(val)
        }

        "getMessages" => {
            let chat_id = param_id_string(params, &["chatId", "chat_id"])
                .ok_or_else(|| "getMessages requires 'chatId'".to_string())?;
            let encoded_chat = url_encode_path_segment(&chat_id);
            let mut qs: Vec<String> = Vec::new();
            if let Some(limit) = param_u64(params, &["limit"]) {
                qs.push(format!("limit={limit}"));
            }
            if let Some(before) = param_i64(params, &["beforeId", "before_id", "offsetId", "offset_id"])
            {
                qs.push(format!("before_id={before}"));
            }
            if let Some(after) = param_i64(params, &["afterId", "after_id"]) {
                qs.push(format!("after_id={after}"));
            }
            if let Some(topic) = param_i64(params, &["topicId", "topic_id"]) {
                qs.push(format!("topic_id={topic}"));
            }
            let path = if qs.is_empty() {
                format!("/api/v1/chats/{encoded_chat}/messages")
            } else {
                format!("/api/v1/chats/{encoded_chat}/messages?{}", qs.join("&"))
            };
            let val = authed_http_json(db, params, "GET", &path, None, None, 15)?;
            if val.is_array() {
                Ok(json!({ "messages": val }))
            } else {
                Ok(val)
            }
        }

        "sendMessage" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "sendMessage requires numeric 'chatId'".to_string())?;
            let text_opt = params.get("text").and_then(Value::as_str);
            let has_media_or_forward = param_val(params, &["content"]).is_some()
                || param_str(params, &["mediaId", "media_id"]).is_some()
                || param_i64(params, &["forwardFromMsgId", "forward_from_msg_id"]).is_some();
            let text_non_empty = text_opt.map(|s| !s.trim().is_empty()).unwrap_or(false);
            if !text_non_empty && !has_media_or_forward {
                return Err(
                    "sendMessage requires non-empty 'text' or 'content'/'mediaId'/'forwardFromMsgId'"
                        .to_string(),
                );
            }
            let mut body = Map::new();
            body.insert("chat_id".into(), json!(chat_id));
            if let Some(t) = text_opt {
                body.insert("text".into(), Value::String(t.to_string()));
            }
            insert_opt_val(&mut body, "entities", params, &["entities"]);
            insert_opt_val(&mut body, "content", params, &["content"]);
            insert_opt_str(&mut body, "media_id", params, &["mediaId", "media_id"]);
            if let Some(reply_to) = param_i64(
                params,
                &[
                    "replyTo",
                    "reply_to",
                    "replyToId",
                    "reply_to_id",
                    "replyToMsgId",
                    "reply_to_msg_id",
                ],
            ) {
                body.insert("reply_to".into(), json!(reply_to));
            }
            insert_opt_str(&mut body, "quote_text", params, &["quoteText", "quote_text"]);
            insert_opt_i64(&mut body, "topic_id", params, &["topicId", "topic_id"]);
            insert_opt_str(
                &mut body,
                "client_nonce",
                params,
                &["clientNonce", "client_nonce"],
            );
            insert_opt_bool(&mut body, "silent", params, &["silent"]);
            insert_opt_i64(
                &mut body,
                "forward_from_chat_id",
                params,
                &["forwardFromChatId", "forward_from_chat_id"],
            );
            insert_opt_i64(
                &mut body,
                "forward_from_msg_id",
                params,
                &["forwardFromMsgId", "forward_from_msg_id"],
            );
            insert_opt_i64(
                &mut body,
                "send_as_peer_id",
                params,
                &["sendAsPeerId", "send_as_peer_id"],
            );
            insert_opt_val(
                &mut body,
                "inline_keyboard",
                params,
                &["inlineKeyboard", "inline_keyboard"],
            );
            insert_opt_i64(
                &mut body,
                "scheduled_at_ms",
                params,
                &["scheduledAtMs", "scheduled_at_ms"],
            );
            insert_opt_bool(
                &mut body,
                "skip_server_bot_reply",
                params,
                &["skipServerBotReply", "skip_server_bot_reply"],
            );

            authed_http_json(
                db,
                params,
                "POST",
                "/api/v1/messages/send",
                Some(Value::Object(body)),
                None,
                15,
            )
        }

        "editMessage" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "editMessage requires numeric 'chatId'".to_string())?;
            let message_id = param_i64(params, &["messageId", "message_id"])
                .ok_or_else(|| "editMessage requires numeric 'messageId'".to_string())?;
            let text = params
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| "editMessage requires 'text'".to_string())?;
            let mut body = Map::new();
            body.insert("text".into(), Value::String(text.to_string()));
            insert_opt_val(&mut body, "entities", params, &["entities"]);
            authed_http_json(
                db,
                params,
                "PUT",
                &format!("/api/v1/chats/{chat_id}/messages/{message_id}"),
                Some(Value::Object(body)),
                None,
                15,
            )
        }

        "deleteMessage" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "deleteMessage requires numeric 'chatId'".to_string())?;
            let message_id = param_i64(params, &["messageId", "message_id"])
                .ok_or_else(|| "deleteMessage requires numeric 'messageId'".to_string())?;
            let path = match param_bool(params, &["revoke"]) {
                Some(rev) => format!("/api/v1/chats/{chat_id}/messages/{message_id}?revoke={rev}"),
                None => format!("/api/v1/chats/{chat_id}/messages/{message_id}"),
            };
            authed_http_json(db, params, "DELETE", &path, None, None, 15)
        }

        "sendReaction" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "sendReaction requires numeric 'chatId'".to_string())?;
            let message_id = param_i64(params, &["messageId", "message_id"])
                .ok_or_else(|| "sendReaction requires numeric 'messageId'".to_string())?;
            let emoji = param_str(params, &["emoji"])
                .ok_or_else(|| "sendReaction requires 'emoji'".to_string())?;
            let mut body = Map::new();
            body.insert("emoji".into(), Value::String(emoji.to_string()));
            insert_opt_str(&mut body, "action", params, &["action"]);
            authed_http_json(
                db,
                params,
                "POST",
                &format!("/api/v1/chats/{chat_id}/messages/{message_id}/reactions"),
                Some(Value::Object(body)),
                None,
                15,
            )
        }

        "markRead" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "markRead requires numeric 'chatId'".to_string())?;
            let max_id = param_i64(params, &["maxId", "max_id"])
                .ok_or_else(|| "markRead requires numeric 'maxId'".to_string())?;
            let mut body = Map::new();
            body.insert("max_id".into(), json!(max_id));
            insert_opt_i64(&mut body, "topic_id", params, &["topicId", "topic_id"]);
            authed_http_json(
                db,
                params,
                "POST",
                &format!("/api/v1/chats/{chat_id}/read"),
                Some(Value::Object(body)),
                None,
                15,
            )
        }

        "clearChat" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "clearChat requires numeric 'chatId'".to_string())?;
            authed_http_json(
                db,
                params,
                "POST",
                &format!("/api/v1/chats/{chat_id}/clear"),
                Some(json!({})),
                None,
                15,
            )
        }

        "leaveChat" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "leaveChat requires numeric 'chatId'".to_string())?;
            let val = authed_http_json(
                db,
                params,
                "POST",
                &format!("/api/v1/chats/{chat_id}/leave"),
                Some(json!({})),
                None,
                15,
            )?;
            crate::with_db(db, |conn| {
                let _ = conn.execute(
                    "DELETE FROM tg_server_chats WHERE chat_id = ?1",
                    params![chat_id],
                );
                let _ = conn.execute(
                    "DELETE FROM tg_server_pinned_messages WHERE chat_id = ?1",
                    params![chat_id],
                );
                Ok(())
            })?;
            Ok(val)
        }

        "setChatTtl" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "setChatTtl requires numeric 'chatId'".to_string())?;
            let ttl_seconds = param_u64(params, &["ttlSeconds", "ttl_seconds"])
                .ok_or_else(|| "setChatTtl requires 'ttlSeconds'".to_string())?;
            authed_http_json(
                db,
                params,
                "POST",
                &format!("/api/v1/chats/{chat_id}/ttl"),
                Some(json!({ "ttl_seconds": ttl_seconds })),
                None,
                15,
            )
        }

        "getDraft" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "getDraft requires numeric 'chatId'".to_string())?;
            let topic_id = param_i64(params, &["topicId", "topic_id"]);
            let path = match topic_id {
                Some(tid) => format!("/api/v1/chats/{chat_id}/draft?topic_id={tid}"),
                None => format!("/api/v1/chats/{chat_id}/draft"),
            };
            let val = authed_http_json(db, params, "GET", &path, None, None, 15)?;
            crate::with_db(db, |conn| {
                let now = chrono::Utc::now().timestamp_millis();
                let tid = topic_id.unwrap_or(0);
                if val.get("draft").map(|d| d.is_null()).unwrap_or(false) {
                    let _ = conn.execute(
                        "DELETE FROM tg_server_drafts WHERE chat_id = ?1 AND topic_id = ?2",
                        params![chat_id, tid],
                    );
                } else {
                    conn.execute(
                        "INSERT OR REPLACE INTO tg_server_drafts (chat_id, topic_id, draft_json, updated_at_ms)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![chat_id, tid, val.to_string(), now],
                    )
                    .map_err(|e| e.to_string())?;
                }
                Ok(())
            })?;
            Ok(val)
        }

        "setDraft" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "setDraft requires numeric 'chatId'".to_string())?;
            let text = params.get("text").and_then(Value::as_str).unwrap_or("");
            let topic_id = param_i64(params, &["topicId", "topic_id"]);
            let mut body = Map::new();
            body.insert("text".into(), Value::String(text.to_string()));
            if let Some(tid) = topic_id {
                body.insert("topic_id".into(), json!(tid));
            }
            insert_opt_i64(
                &mut body,
                "reply_to_msg_id",
                params,
                &[
                    "replyToMsgId",
                    "reply_to_msg_id",
                    "replyToId",
                    "reply_to_id",
                    "replyTo",
                    "reply_to",
                ],
            );
            insert_opt_bool(&mut body, "no_webpage", params, &["noWebpage", "no_webpage"]);
            insert_opt_val(&mut body, "entities", params, &["entities"]);

            let val = authed_http_json(
                db,
                params,
                "PUT",
                &format!("/api/v1/chats/{chat_id}/draft"),
                Some(Value::Object(body)),
                None,
                15,
            )?;
            crate::with_db(db, |conn| {
                let now = chrono::Utc::now().timestamp_millis();
                conn.execute(
                    "INSERT OR REPLACE INTO tg_server_drafts (chat_id, topic_id, draft_json, updated_at_ms)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![chat_id, topic_id.unwrap_or(0), val.to_string(), now],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })?;
            Ok(val)
        }

        "clearDraft" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "clearDraft requires numeric 'chatId'".to_string())?;
            let topic_id = param_i64(params, &["topicId", "topic_id"]);
            let path = match topic_id {
                Some(tid) => format!("/api/v1/chats/{chat_id}/draft?topic_id={tid}"),
                None => format!("/api/v1/chats/{chat_id}/draft"),
            };
            let val = authed_http_json(db, params, "DELETE", &path, None, None, 15)?;
            crate::with_db(db, |conn| {
                conn.execute(
                    "DELETE FROM tg_server_drafts WHERE chat_id = ?1 AND topic_id = ?2",
                    params![chat_id, topic_id.unwrap_or(0)],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })?;
            Ok(val)
        }

        "listFolders" => {
            let val = authed_http_json(db, params, "GET", "/api/v1/folders", None, None, 15)?;
            let arr_opt = val
                .as_array()
                .or_else(|| val.get("folders").and_then(Value::as_array));
            if let Some(arr) = arr_opt {
                crate::with_db(db, |conn| {
                    let now = chrono::Utc::now().timestamp_millis();
                    conn.execute("DELETE FROM tg_server_folders", [])
                        .map_err(|e| e.to_string())?;
                    for (idx, folder) in arr.iter().enumerate() {
                        upsert_server_folder_cache(conn, folder, idx as i64, now)?;
                    }
                    Ok(())
                })?;
            }
            if val.is_array() {
                Ok(json!({ "folders": val }))
            } else {
                Ok(val)
            }
        }

        "saveFolder" => {
            let folder_id = param_i64(params, &["id", "folderId", "folder_id"]).filter(|&i| i > 0);
            let title = param_str(params, &["title"])
                .ok_or_else(|| "saveFolder requires 'title'".to_string())?;
            let mut body = Map::new();
            if let Some(id) = folder_id {
                body.insert("id".into(), json!(id));
            }
            body.insert("title".into(), Value::String(title.to_string()));
            insert_opt_str(&mut body, "icon", params, &["icon", "emoticon"]);
            insert_opt_val(
                &mut body,
                "included_chat_ids",
                params,
                &["includedChatIds", "included_chat_ids", "include_chats"],
            );
            insert_opt_val(
                &mut body,
                "excluded_chat_ids",
                params,
                &["excludedChatIds", "excluded_chat_ids", "exclude_chats"],
            );
            insert_opt_val(
                &mut body,
                "pinned_chat_ids",
                params,
                &["pinnedChatIds", "pinned_chat_ids", "pinned_chats"],
            );
            for (camel, snake) in [
                ("includeContacts", "include_contacts"),
                ("includeNonContacts", "include_non_contacts"),
                ("includeGroups", "include_groups"),
                ("includeBroadcasts", "include_broadcasts"),
                ("includeChannels", "include_channels"),
                ("includeBots", "include_bots"),
                ("excludeMuted", "exclude_muted"),
                ("excludeRead", "exclude_read"),
                ("excludeArchived", "exclude_archived"),
            ] {
                insert_opt_bool(&mut body, snake, params, &[camel, snake]);
            }
            insert_opt_i64(&mut body, "sort_order", params, &["sortOrder", "sort_order"]);

            let (method, path) = match folder_id {
                Some(id) => ("PUT", format!("/api/v1/folders/{id}")),
                None => ("POST", "/api/v1/folders".to_string()),
            };
            let val = authed_http_json(db, params, method, &path, Some(Value::Object(body)), None, 15)?;
            crate::with_db(db, |conn| {
                let now = chrono::Utc::now().timestamp_millis();
                let folder_obj = val.get("folder").unwrap_or(&val);
                let _ = upsert_server_folder_cache(conn, folder_obj, 0, now);
                Ok(())
            })?;
            Ok(val)
        }

        "deleteFolder" => {
            let folder_id = param_i64(params, &["folderId", "folder_id", "id"])
                .ok_or_else(|| "deleteFolder requires numeric 'folderId'".to_string())?;
            let val = authed_http_json(
                db,
                params,
                "DELETE",
                &format!("/api/v1/folders/{folder_id}"),
                None,
                None,
                15,
            )?;
            crate::with_db(db, |conn| {
                let _ = conn.execute(
                    "DELETE FROM tg_server_folders WHERE folder_id = ?1",
                    params![folder_id],
                );
                Ok(())
            })?;
            Ok(val)
        }

        "createPoll" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "createPoll requires numeric 'chatId'".to_string())?;
            let question = param_str(params, &["question"])
                .ok_or_else(|| "createPoll requires 'question'".to_string())?;
            let options = params
                .get("options")
                .and_then(Value::as_array)
                .ok_or_else(|| "createPoll requires 'options' array".to_string())?;
            let mut body = Map::new();
            body.insert("question".into(), Value::String(question.to_string()));
            body.insert("options".into(), Value::Array(options.clone()));
            body.insert(
                "is_anonymous".into(),
                Value::Bool(param_bool(params, &["isAnonymous", "is_anonymous"]).unwrap_or(true)),
            );
            body.insert(
                "multiple_choice".into(),
                Value::Bool(
                    param_bool(params, &["multipleChoice", "multiple_choice"]).unwrap_or(false),
                ),
            );
            body.insert(
                "is_quiz".into(),
                Value::Bool(param_bool(params, &["isQuiz", "is_quiz"]).unwrap_or(false)),
            );
            insert_opt_u64(
                &mut body,
                "correct_option_id",
                params,
                &["correctOptionId", "correct_option_id"],
            );
            insert_opt_str(&mut body, "solution", params, &["solution"]);
            insert_opt_i64(&mut body, "topic_id", params, &["topicId", "topic_id"]);
            insert_opt_str(
                &mut body,
                "client_nonce",
                params,
                &["clientNonce", "client_nonce"],
            );

            authed_http_json(
                db,
                params,
                "POST",
                &format!("/api/v1/chats/{chat_id}/polls"),
                Some(Value::Object(body)),
                None,
                15,
            )
        }

        "votePoll" => {
            let poll_id = param_id_string(params, &["pollId", "poll_id"])
                .ok_or_else(|| "votePoll requires 'pollId'".to_string())?;
            let option_ids = param_val(params, &["optionIds", "option_ids"])
                .cloned()
                .ok_or_else(|| "votePoll requires 'optionIds'".to_string())?;
            let encoded = url_encode_path_segment(&poll_id);
            authed_http_json(
                db,
                params,
                "POST",
                &format!("/api/v1/polls/{encoded}/vote"),
                Some(json!({ "option_ids": option_ids })),
                None,
                15,
            )
        }

        "closePoll" => {
            let poll_id = param_id_string(params, &["pollId", "poll_id"])
                .ok_or_else(|| "closePoll requires 'pollId'".to_string())?;
            let encoded = url_encode_path_segment(&poll_id);
            authed_http_json(
                db,
                params,
                "POST",
                &format!("/api/v1/polls/{encoded}/close"),
                Some(json!({})),
                None,
                15,
            )
        }

        "listContacts" => {
            let val = authed_http_json(db, params, "GET", "/api/v1/contacts", None, None, 15)?;
            if val.is_array() {
                Ok(json!({ "contacts": val }))
            } else {
                Ok(val)
            }
        }

        "addContact" => {
            let username = param_str(params, &["username"]);
            let phone = param_str(params, &["phone"]);
            if username.is_none() && phone.is_none() {
                return Err("addContact requires 'username' or 'phone'".to_string());
            }
            let mut body = Map::new();
            if let Some(u) = username {
                body.insert(
                    "username".into(),
                    Value::String(u.trim_start_matches('@').to_string()),
                );
            }
            if let Some(p) = phone {
                body.insert("phone".into(), Value::String(p.to_string()));
            }
            insert_opt_str(&mut body, "first_name", params, &["firstName", "first_name"]);
            insert_opt_str(&mut body, "last_name", params, &["lastName", "last_name"]);
            insert_opt_str(&mut body, "bio", params, &["bio"]);
            if let Some(kind) = param_str(params, &["kind"]) {
                body.insert("kind".into(), Value::String(kind.to_string()));
            } else if username.is_some() {
                body.insert("kind".into(), Value::String("user".to_string()));
            }
            authed_http_json(
                db,
                params,
                "POST",
                "/api/v1/contacts/add",
                Some(Value::Object(body)),
                None,
                15,
            )
        }

        "deleteContact" => {
            let user_id = param_id_string(params, &["userId", "user_id"])
                .ok_or_else(|| "deleteContact requires 'userId'".to_string())?;
            let encoded = url_encode_path_segment(&user_id);
            authed_http_json(
                db,
                params,
                "DELETE",
                &format!("/api/v1/contacts/{encoded}"),
                None,
                None,
                15,
            )
        }

        "resolvePeer" => {
            let target = param_str(params, &["username", "query", "q"])
                .ok_or_else(|| "resolvePeer requires 'username'".to_string())?;
            let clean = target.trim_start_matches('@');
            let encoded = url_encode_query(clean);
            authed_http_json(
                db,
                params,
                "GET",
                &format!("/api/v1/peers/resolve?username={encoded}"),
                None,
                None,
                15,
            )
        }

        "uploadMedia" => {
            let data_b64 = param_str(params, &["dataBase64", "data_base64"])
                .ok_or_else(|| "uploadMedia requires 'dataBase64'".to_string())?;
            let mut body = Map::new();
            body.insert("data_base64".into(), Value::String(data_b64.to_string()));
            insert_opt_str(&mut body, "filename", params, &["filename"]);
            insert_opt_str(&mut body, "mime_type", params, &["mimeType", "mime_type"]);
            insert_opt_i64(&mut body, "chat_id", params, &["chatId", "chat_id"]);
            insert_opt_bool(&mut body, "is_voice", params, &["isVoice", "is_voice"]);
            insert_opt_bool(
                &mut body,
                "is_video_note",
                params,
                &["isVideoNote", "is_video_note"],
            );

            let mut qs: Vec<String> = Vec::new();
            if let Some(cid) = param_i64(params, &["chatId", "chat_id"]) {
                qs.push(format!("chat_id={cid}"));
            }
            if let Some(f) = param_str(params, &["filename"]) {
                qs.push(format!("filename={}", url_encode_query(f)));
            }
            if let Some(mt) = param_str(params, &["mimeType", "mime_type"]) {
                qs.push(format!("mime_type={}", url_encode_query(mt)));
            }
            if let Some(iv) = param_bool(params, &["isVoice", "is_voice"]) {
                qs.push(format!("is_voice={iv}"));
            }
            if let Some(ivn) = param_bool(params, &["isVideoNote", "is_video_note"]) {
                qs.push(format!("is_video_note={ivn}"));
            }
            let path = if qs.is_empty() {
                "/api/v1/media/upload".to_string()
            } else {
                format!("/api/v1/media/upload?{}", qs.join("&"))
            };
            authed_http_json(db, params, "POST", &path, Some(Value::Object(body)), None, 30)
        }

        "downloadMedia" => {
            let file_id = param_id_string(params, &["fileId", "file_id", "mediaId", "media_id"])
                .ok_or_else(|| "downloadMedia requires 'fileId'".to_string())?;
            let (ep, tok) = crate::with_db(db, |conn| {
                init_schema(conn)?;
                require_server_auth(conn, params)
            })?;
            let encoded = url_encode_path_segment(&file_id);
            let url = format!("{ep}/api/v1/media/{encoded}");
            let res = desktop_http_bytes(&url, Some(&tok), 30);
            match res {
                Ok((bytes, mime_type, filename)) => {
                    // If server returned a JSON payload containing data_base64, pass through cleanly
                    if mime_type.contains("application/json") {
                        if let Ok(parsed) = serde_json::from_slice::<Value>(&bytes) {
                            if parsed.get("data_base64").is_some()
                                || parsed.get("dataBase64").is_some()
                            {
                                return Ok(parsed);
                            }
                        }
                    }
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    let mut out = json!({
                        "fileId": file_id,
                        "file_id": file_id,
                        "mimeType": mime_type,
                        "mime_type": mime_type,
                        "sizeBytes": bytes.len(),
                        "size_bytes": bytes.len(),
                        "dataBase64": b64,
                        "data_base64": b64,
                    });
                    if let Some(fname) = filename {
                        out["filename"] = Value::String(fname);
                    }
                    Ok(out)
                }
                Err(e) => {
                    if is_http_401(&e) {
                        let _ = crate::with_db(db, |conn| {
                            clear_server_session_and_caches_if_token_matches(conn, &tok)
                        });
                    }
                    Err(e)
                }
            }
        }

        "searchMessages" => {
            let query = params
                .get("query")
                .or_else(|| params.get("q"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let category = params
                .get("category")
                .and_then(Value::as_str)
                .unwrap_or("all");
            let mut qs = vec![
                format!("q={}", url_encode_query(query)),
                format!("category={}", url_encode_query(category)),
            ];
            if let Some(cid) = param_i64(params, &["chatId", "chat_id"]) {
                qs.push(format!("chat_id={cid}"));
            }
            if let Some(limit) = param_u64(params, &["limit"]) {
                qs.push(format!("limit={limit}"));
            }
            if let Some(offset) = param_u64(params, &["offset"]) {
                qs.push(format!("offset={offset}"));
            }
            authed_http_json(
                db,
                params,
                "GET",
                &format!("/api/v1/search/messages?{}", qs.join("&")),
                None,
                None,
                15,
            )
        }

        "channelStats" => {
            let chat_id = param_id_string(params, &["chatId", "chat_id", "conversationId"])
                .ok_or_else(|| "channelStats requires 'chatId'".to_string())?;
            let encoded = url_encode_path_segment(&chat_id);
            authed_http_json(
                db,
                params,
                "GET",
                &format!("/api/v1/chats/{encoded}/statistics"),
                None,
                None,
                15,
            )
        }

        "enqueueOutbox" => crate::with_db(db, |conn| {
            init_schema(conn)?;
            let now = chrono::Utc::now().timestamp_millis();
            let payload = params.get("payload").cloned().unwrap_or_else(|| json!({}));
            conn.execute(
                "INSERT INTO tg_server_outbox (payload_json, flushed, retry_count, created_at_ms) VALUES (?1, 0, 0, ?2)",
                params![payload.to_string(), now],
            )
            .map_err(|e| e.to_string())?;
            let id = conn.last_insert_rowid();
            Ok(json!({ "outboxSeq": id, "queued": true }))
        }),

        "flushOutbox" => handle_flush_outbox(db),

        "syncDifference" => handle_sync_difference(db, params),

        // ------------------------------------------------------------------
        // 55–66: Real Bots & Coding Agents (Spec §8.3)
        // ------------------------------------------------------------------
        "listMyBots" => {
            let val = authed_http_json(db, params, "GET", "/api/v1/bots/my", None, None, 15)?;
            let bots_arr: Vec<Value> = if let Some(arr) = val.as_array() {
                arr.clone()
            } else if let Some(arr) = val.get("bots").and_then(Value::as_array) {
                arr.clone()
            } else {
                Vec::new()
            };
            crate::with_db(db, |conn| replace_server_bots_cache(conn, "my_bots", &bots_arr))?;
            if val.is_array() {
                let total = bots_arr.len();
                Ok(json!({ "bots": bots_arr, "total": total }))
            } else {
                Ok(val)
            }
        }

        "createBot" => {
            let display_name = param_str(params, &["displayName", "display_name"])
                .ok_or_else(|| "createBot requires 'displayName'".to_string())?;
            let username = param_str(params, &["username"])
                .ok_or_else(|| "createBot requires 'username'".to_string())?;
            let mut body = Map::new();
            body.insert("display_name".into(), Value::String(display_name.to_string()));
            body.insert(
                "username".into(),
                Value::String(username.trim_start_matches('@').to_string()),
            );
            for (camel, snake) in [
                ("description", "description"),
                ("about", "about"),
                ("aiPersona", "ai_persona"),
                ("menuButtonText", "menu_button_text"),
                ("menuButtonUrl", "menu_button_url"),
                ("agentId", "agent_id"),
                ("avatarShape", "avatar_shape"),
                ("avatarColor", "avatar_color"),
                ("preferredModel", "preferred_model"),
                ("thinkingLevel", "thinking_level"),
                ("approvalMode", "approval_mode"),
                ("toolsetPreset", "toolset_preset"),
            ] {
                insert_opt_str(&mut body, snake, params, &[camel, snake]);
            }
            insert_opt_bool(
                &mut body,
                "allow_attach_menu",
                params,
                &["allowAttachMenu", "allow_attach_menu"],
            );
            insert_opt_val(
                &mut body,
                "attach_peer_types",
                params,
                &["attachPeerTypes", "attach_peer_types"],
            );
            insert_opt_val(&mut body, "commands", params, &["commands"]);

            let val = authed_http_json(
                db,
                params,
                "POST",
                "/api/v1/bots",
                Some(Value::Object(body)),
                None,
                15,
            )?;
            crate::with_db(db, |conn| {
                let bot_obj = val.get("bot").unwrap_or(&val);
                let _ = upsert_server_bot_cache(conn, "my_bots", bot_obj);
                Ok(())
            })?;
            Ok(val)
        }

        "listFabushiBots" => {
            let val = authed_http_json(db, params, "GET", "/api/v1/fabushi-bots", None, None, 15)?;
            let bots_arr: Vec<Value> = if let Some(arr) = val.as_array() {
                arr.clone()
            } else if let Some(arr) = val.get("bots").and_then(Value::as_array) {
                arr.clone()
            } else {
                Vec::new()
            };
            crate::with_db(db, |conn| {
                replace_server_bots_cache(conn, "fabushi_bots", &bots_arr)
            })?;
            if val.is_array() {
                let total = bots_arr.len();
                Ok(json!({ "bots": bots_arr, "total": total }))
            } else {
                Ok(val)
            }
        }

        "createFabushiBot" => {
            let display_name = param_str(params, &["displayName", "display_name"])
                .ok_or_else(|| "createFabushiBot requires 'displayName'".to_string())?;
            let mut body = Map::new();
            body.insert("display_name".into(), Value::String(display_name.to_string()));
            for (camel, snake) in [
                ("username", "username"),
                ("agentId", "agent_id"),
                ("description", "description"),
                ("title", "title"),
                ("avatarShape", "avatar_shape"),
                ("avatarColor", "avatar_color"),
                ("preferredModel", "preferred_model"),
                ("thinkingLevel", "thinking_level"),
                ("approvalMode", "approval_mode"),
                ("toolsetPreset", "toolset_preset"),
            ] {
                insert_opt_str(&mut body, snake, params, &[camel, snake]);
            }
            insert_opt_val(
                &mut body,
                "coding_agent",
                params,
                &["codingAgent", "coding_agent"],
            );

            let val = authed_http_json(
                db,
                params,
                "POST",
                "/api/v1/fabushi-bots",
                Some(Value::Object(body)),
                None,
                15,
            )?;
            crate::with_db(db, |conn| {
                let bot_obj = val.get("bot").unwrap_or(&val);
                let _ = upsert_server_bot_cache(conn, "fabushi_bots", bot_obj);
                Ok(())
            })?;
            Ok(val)
        }

        "getFabushiBot" => {
            let bot_id = param_id_string(params, &["botId", "bot_id", "id"])
                .ok_or_else(|| "getFabushiBot requires 'botId'".to_string())?;
            let encoded = url_encode_path_segment(&bot_id);
            authed_http_json(
                db,
                params,
                "GET",
                &format!("/api/v1/fabushi-bots/{encoded}"),
                None,
                None,
                15,
            )
        }

        "patchFabushiBot" => {
            let bot_id = param_id_string(params, &["botId", "bot_id", "id"])
                .ok_or_else(|| "patchFabushiBot requires 'botId'".to_string())?;
            let encoded = url_encode_path_segment(&bot_id);
            let mut body = Map::new();
            for (camel, snake) in [
                ("displayName", "display_name"),
                ("description", "description"),
                ("title", "title"),
                ("avatarShape", "avatar_shape"),
                ("avatarColor", "avatar_color"),
                ("preferredModel", "preferred_model"),
                ("thinkingLevel", "thinking_level"),
                ("approvalMode", "approval_mode"),
                ("toolsetPreset", "toolset_preset"),
            ] {
                insert_opt_str(&mut body, snake, params, &[camel, snake]);
            }
            insert_opt_bool(
                &mut body,
                "notify_on_agent_updates",
                params,
                &["notifyOnAgentUpdates", "notify_on_agent_updates"],
            );
            insert_opt_bool(
                &mut body,
                "hidden_from_sidebar",
                params,
                &["hiddenFromSidebar", "hidden_from_sidebar"],
            );
            insert_opt_val(
                &mut body,
                "coding_agent",
                params,
                &["codingAgent", "coding_agent"],
            );

            let val = authed_http_json(
                db,
                params,
                "PATCH",
                &format!("/api/v1/fabushi-bots/{encoded}"),
                Some(Value::Object(body)),
                None,
                15,
            )?;
            crate::with_db(db, |conn| {
                let bot_obj = val.get("bot").unwrap_or(&val);
                let _ = upsert_server_bot_cache(conn, "fabushi_bots", bot_obj);
                Ok(())
            })?;
            Ok(val)
        }

        "getBotProfile" => {
            let target = param_id_string(
                params,
                &["target", "botUsername", "username", "botId", "bot_id"],
            )
            .ok_or_else(|| "getBotProfile requires 'target'".to_string())?;
            let clean = target.trim_start_matches('@');
            let encoded = url_encode_path_segment(clean);
            authed_http_json(
                db,
                params,
                "GET",
                &format!("/api/v1/bots/profile/{encoded}"),
                None,
                None,
                15,
            )
        }

        "botfatherNlCreate" => {
            let prompt = param_str(params, &["prompt"])
                .ok_or_else(|| "botfatherNlCreate requires 'prompt'".to_string())?;
            let mut body = Map::new();
            body.insert("prompt".into(), Value::String(prompt.to_string()));
            insert_opt_str(&mut body, "avatar_shape", params, &["avatarShape", "avatar_shape"]);
            insert_opt_str(&mut body, "avatar_color", params, &["avatarColor", "avatar_color"]);
            authed_http_json(
                db,
                params,
                "POST",
                "/api/v1/botfather/nl-create",
                Some(Value::Object(body)),
                None,
                30,
            )
        }

        "sendCallbackQuery" => {
            let chat_id = param_i64(params, &["chatId", "chat_id"])
                .ok_or_else(|| "sendCallbackQuery requires numeric 'chatId'".to_string())?;
            let message_id = param_i64(params, &["messageId", "message_id"])
                .ok_or_else(|| "sendCallbackQuery requires numeric 'messageId'".to_string())?;
            let callback_data = param_str(params, &["callbackData", "callback_data", "data"])
                .ok_or_else(|| "sendCallbackQuery requires 'callbackData'".to_string())?;
            authed_http_json(
                db,
                params,
                "POST",
                &format!("/api/v1/chats/{chat_id}/callback"),
                Some(json!({
                    "message_id": message_id,
                    "callback_data": callback_data,
                })),
                None,
                15,
            )
        }

        "probeCodingAgents" => {
            let mut body = Map::new();
            insert_opt_str(&mut body, "device_id", params, &["deviceId", "device_id"]);
            insert_opt_u64(&mut body, "timeout_ms", params, &["timeoutMs", "timeout_ms"]);
            authed_http_json(
                db,
                params,
                "POST",
                "/api/v1/coding-agents/probe",
                Some(Value::Object(body)),
                param_str(params, &["deviceId", "device_id"]),
                20,
            )
        }

        "getBotComputer" => {
            let bot_id = param_id_string(params, &["botId", "bot_id"])
                .ok_or_else(|| "getBotComputer requires 'botId'".to_string())?;
            let encoded = url_encode_path_segment(&bot_id);
            authed_http_json(
                db,
                params,
                "GET",
                &format!("/api/v1/bots/{encoded}/computer"),
                None,
                None,
                15,
            )
        }

        "setBotComputer" => {
            let bot_id = param_id_string(params, &["botId", "bot_id"])
                .ok_or_else(|| "setBotComputer requires 'botId'".to_string())?;
            let encoded = url_encode_path_segment(&bot_id);
            let mut body = Map::new();
            insert_opt_str(
                &mut body,
                "bound_node_id",
                params,
                &["boundNodeId", "bound_node_id"],
            );
            insert_opt_bool(
                &mut body,
                "clear_bound_node",
                params,
                &["clearBoundNode", "clear_bound_node"],
            );
            insert_opt_str(
                &mut body,
                "persona_prompt",
                params,
                &["personaPrompt", "persona_prompt"],
            );
            insert_opt_str(
                &mut body,
                "preferred_model",
                params,
                &["preferredModel", "preferred_model"],
            );
            insert_opt_str(
                &mut body,
                "approval_mode",
                params,
                &["approvalMode", "approval_mode"],
            );
            insert_opt_bool(
                &mut body,
                "allow_cross_device_control",
                params,
                &["allowCrossDeviceControl", "allow_cross_device_control"],
            );
            insert_opt_bool(
                &mut body,
                "allow_cloud_self_config",
                params,
                &["allowCloudSelfConfig", "allow_cloud_self_config"],
            );
            authed_http_json(
                db,
                params,
                "PUT",
                &format!("/api/v1/bots/{encoded}/computer"),
                Some(Value::Object(body)),
                None,
                15,
            )
        }

        // ------------------------------------------------------------------
        // 67–88: Real Remote Control, Computers, Assist, Sessions, WebRTC & MCP
        // ------------------------------------------------------------------
        "listComputers" => {
            let device_id = param_str(params, &["deviceId", "device_id"]);
            let val = authed_http_json(db, params, "GET", "/api/v1/computers", None, device_id, 15)?;
            let nodes_arr: Vec<Value> = if let Some(arr) = val.as_array() {
                arr.clone()
            } else if let Some(arr) = val.get("computers").and_then(Value::as_array) {
                arr.clone()
            } else {
                Vec::new()
            };
            crate::with_db(db, |conn| {
                let now = chrono::Utc::now().timestamp_millis();
                conn.execute("DELETE FROM tg_server_computers", [])
                    .map_err(|e| e.to_string())?;
                for node in &nodes_arr {
                    upsert_server_computer_cache(conn, node, now)?;
                }
                Ok(())
            })?;
            if val.is_array() {
                Ok(json!({ "computers": nodes_arr }))
            } else {
                Ok(val)
            }
        }

        "registerLocalComputer" | "localRegisterComputer" => {
            let device_id = param_str(params, &["deviceId", "device_id"])
                .ok_or_else(|| "registerLocalComputer requires 'deviceId'".to_string())?;
            let display_name =
                param_str(params, &["displayName", "display_name"]).unwrap_or("Fabushi Android");
            let ip_address =
                param_str(params, &["ipAddress", "ip_address"]).unwrap_or("127.0.0.1");
            let os_name = param_str(params, &["osName", "os_name"]).unwrap_or("Android");
            let resolution = param_str(params, &["resolution"]).unwrap_or("1080x2400");
            let val = authed_http_json(
                db,
                params,
                "POST",
                "/api/v1/computers/local/register",
                Some(json!({
                    "device_id": device_id,
                    "display_name": display_name,
                    "ip_address": ip_address,
                    "os_name": os_name,
                    "resolution": resolution,
                })),
                Some(device_id),
                15,
            )?;
            crate::with_db(db, |conn| {
                let now = chrono::Utc::now().timestamp_millis();
                let _ = upsert_server_computer_cache(conn, &val, now);
                Ok(())
            })?;
            Ok(val)
        }

        "pollLocalCommands" | "localPollCommands" => {
            let device_id = param_str(params, &["deviceId", "device_id"])
                .ok_or_else(|| "pollLocalCommands requires 'deviceId'".to_string())?;
            let val = authed_http_json(
                db,
                params,
                "GET",
                "/api/v1/computers/local/poll",
                None,
                Some(device_id),
                15,
            )?;
            if val.is_array() {
                Ok(json!({ "commands": val }))
            } else {
                Ok(val)
            }
        }

        "approveComputerCommand" | "approveCommand" => {
            let command_id = param_id_string(params, &["commandId", "command_id"])
                .ok_or_else(|| "approveComputerCommand requires 'commandId'".to_string())?;
            let approved = param_bool(params, &["approved"]).unwrap_or(true);
            let encoded = url_encode_path_segment(&command_id);
            authed_http_json(
                db,
                params,
                "POST",
                &format!("/api/v1/computers/commands/{encoded}/approve"),
                Some(json!({ "approved": approved })),
                None,
                15,
            )
        }

        "completeComputerCommand" | "completeCommand" => {
            let command_id = param_id_string(params, &["commandId", "command_id"])
                .ok_or_else(|| "completeComputerCommand requires 'commandId'".to_string())?;
            let encoded = url_encode_path_segment(&command_id);
            let body = if let Some(res_obj) = params.get("result").filter(|v| v.is_object()) {
                res_obj.clone()
            } else {
                let mut m = Map::new();
                m.insert("command_id".into(), Value::String(command_id.clone()));
                insert_opt_str(&mut m, "node_id", params, &["nodeId", "node_id"]);
                m.insert(
                    "ok".into(),
                    Value::Bool(param_bool(params, &["ok"]).unwrap_or(true)),
                );
                insert_opt_i64(&mut m, "exit_code", params, &["exitCode", "exit_code"]);
                m.insert(
                    "stdout".into(),
                    Value::String(
                        params
                            .get("stdout")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    ),
                );
                m.insert(
                    "stderr".into(),
                    Value::String(
                        params
                            .get("stderr")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    ),
                );
                m.insert(
                    "entries".into(),
                    params
                        .get("entries")
                        .cloned()
                        .unwrap_or_else(|| json!([])),
                );
                insert_opt_str(
                    &mut m,
                    "frame_data_url",
                    params,
                    &["frameDataUrl", "frame_data_url"],
                );
                m.insert(
                    "executed_by".into(),
                    Value::String(
                        param_str(params, &["executedBy", "executed_by"])
                            .unwrap_or("Fabushi Android")
                            .to_string(),
                    ),
                );
                m.insert(
                    "duration_ms".into(),
                    json!(param_u64(params, &["durationMs", "duration_ms"]).unwrap_or(0)),
                );
                Value::Object(m)
            };
            authed_http_json(
                db,
                params,
                "POST",
                &format!("/api/v1/computers/commands/{encoded}/complete"),
                Some(body),
                None,
                30,
            )
        }

        "upsertCloudComputer" => {
            let display_name = param_str(params, &["displayName", "display_name"])
                .ok_or_else(|| "upsertCloudComputer requires 'displayName'".to_string())?;
            let host = param_str(params, &["host"])
                .ok_or_else(|| "upsertCloudComputer requires non-empty 'host'".to_string())?;
            let port = param_u64(params, &["port"]).unwrap_or(22);
            let username = param_str(params, &["username"]).unwrap_or("root");
            let auth_method =
                param_str(params, &["authMethod", "auth_method"]).unwrap_or("ssh_password");
            let verify_connection =
                param_bool(params, &["verifyConnection", "verify_connection"]).unwrap_or(false);

            let mut body = Map::new();
            insert_opt_str(&mut body, "node_id", params, &["nodeId", "node_id"]);
            body.insert("display_name".into(), Value::String(display_name.to_string()));
            body.insert("host".into(), Value::String(host.to_string()));
            body.insert("port".into(), json!(port));
            body.insert("username".into(), Value::String(username.to_string()));
            body.insert("auth_method".into(), Value::String(auth_method.to_string()));
            insert_opt_str(&mut body, "secret", params, &["secret"]);
            insert_opt_str(
                &mut body,
                "agent_endpoint_url",
                params,
                &["agentEndpointUrl", "agent_endpoint_url"],
            );
            insert_opt_str(&mut body, "workdir", params, &["workdir"]);
            insert_opt_i64(
                &mut body,
                "bind_bot_id",
                params,
                &["bindBotId", "bind_bot_id"],
            );
            body.insert(
                "verify_connection".into(),
                Value::Bool(verify_connection),
            );

            let val = authed_http_json(
                db,
                params,
                "POST",
                "/api/v1/computers/cloud",
                Some(Value::Object(body)),
                None,
                30,
            )?;
            crate::with_db(db, |conn| {
                let now = chrono::Utc::now().timestamp_millis();
                // Sanitize out any raw secret before caching
                let mut cached = val.clone();
                if let Some(obj) = cached.as_object_mut() {
                    obj.remove("secret");
                }
                let _ = upsert_server_computer_cache(conn, &cached, now);
                Ok(())
            })?;
            Ok(val)
        }

        "patchComputerPolicy" => {
            let node_id = param_str(params, &["nodeId", "node_id"])
                .ok_or_else(|| "patchComputerPolicy requires 'nodeId'".to_string())?;
            let encoded = url_encode_path_segment(node_id);
            let mut body = Map::new();
            insert_opt_str(&mut body, "run_policy", params, &["runPolicy", "run_policy"]);
            insert_opt_val(
                &mut body,
                "add_controller",
                params,
                &["addController", "add_controller"],
            );
            insert_opt_str(
                &mut body,
                "remove_controller_id",
                params,
                &["removeControllerId", "remove_controller_id"],
            );
            insert_opt_str(
                &mut body,
                "display_name",
                params,
                &["displayName", "display_name"],
            );
            let val = authed_http_json(
                db,
                params,
                "PATCH",
                &format!("/api/v1/computers/{encoded}/policy"),
                Some(Value::Object(body)),
                None,
                15,
            )?;
            crate::with_db(db, |conn| {
                let now = chrono::Utc::now().timestamp_millis();
                let _ = upsert_server_computer_cache(conn, &val, now);
                Ok(())
            })?;
            Ok(val)
        }

        "deleteComputer" => {
            let node_id = param_str(params, &["nodeId", "node_id"])
                .ok_or_else(|| "deleteComputer requires 'nodeId'".to_string())?;
            let encoded = url_encode_path_segment(node_id);
            let val = authed_http_json(
                db,
                params,
                "DELETE",
                &format!("/api/v1/computers/{encoded}"),
                None,
                None,
                15,
            )?;
            crate::with_db(db, |conn| {
                let _ = conn.execute(
                    "DELETE FROM tg_server_computers WHERE node_id = ?1",
                    params![node_id],
                );
                Ok(())
            })?;
            Ok(val)
        }

        "testComputer" => {
            let node_id = param_str(params, &["nodeId", "node_id"])
                .ok_or_else(|| "testComputer requires 'nodeId'".to_string())?;
            let encoded = url_encode_path_segment(node_id);
            authed_http_json(
                db,
                params,
                "POST",
                &format!("/api/v1/computers/{encoded}/test"),
                Some(json!({})),
                None,
                30,
            )
        }

        "execComputer" => {
            let node_id = param_str(params, &["nodeId", "node_id"])
                .ok_or_else(|| "execComputer requires 'nodeId'".to_string())?;
            let op_val = params
                .get("operation")
                .or_else(|| params.get("op"))
                .ok_or_else(|| "execComputer requires 'operation'".to_string())?;
            // Validate strictly against canonical tagged RemoteComputerOp (all 19 variants!)
            let parsed_op: gpui_telegram_proto::computer_contracts::RemoteComputerOp =
                serde_json::from_value(op_val.clone())
                    .map_err(|e| format!("Invalid RemoteComputerOp payload: {e}"))?;
            let op_json = serde_json::to_value(&parsed_op).map_err(|e| e.to_string())?;

            let mut body = Map::new();
            body.insert("node_id".into(), Value::String(node_id.to_string()));
            insert_opt_str(
                &mut body,
                "caller_device_id",
                params,
                &["callerDeviceId", "caller_device_id"],
            );
            insert_opt_i64(
                &mut body,
                "caller_bot_id",
                params,
                &["callerBotId", "caller_bot_id"],
            );
            body.insert(
                "caller_label".into(),
                Value::String(
                    param_str(params, &["callerLabel", "caller_label"])
                        .unwrap_or("Fabushi Android")
                        .to_string(),
                ),
            );
            body.insert("operation".into(), op_json);

            authed_http_json(
                db,
                params,
                "POST",
                "/api/v1/computers/exec",
                Some(Value::Object(body)),
                param_str(params, &["callerDeviceId", "caller_device_id"]),
                30,
            )
        }

        "assistInfo" => {
            let device_id = param_str(params, &["deviceId", "device_id"]);
            let path = match device_id {
                Some(did) => format!("/api/v1/remote/assist/info?device_id={}", url_encode_query(did)),
                None => "/api/v1/remote/assist/info".to_string(),
            };
            authed_http_json(db, params, "GET", &path, None, device_id, 15)
        }

        "assistResetCode" => {
            let device_id = param_str(params, &["deviceId", "device_id"]);
            let body = match device_id {
                Some(did) => json!({ "device_id": did }),
                None => json!({}),
            };
            authed_http_json(
                db,
                params,
                "POST",
                "/api/v1/remote/assist/reset-code",
                Some(body),
                device_id,
                15,
            )
        }

        "assistSetCode" => {
            let code = params
                .get("code")
                .and_then(Value::as_str)
                .ok_or_else(|| "assistSetCode requires 'code'".to_string())?;
            let device_id = param_str(params, &["deviceId", "device_id"]);
            let mut body = Map::new();
            body.insert("code".into(), Value::String(code.to_string()));
            if let Some(did) = device_id {
                body.insert("device_id".into(), Value::String(did.to_string()));
            }
            authed_http_json(
                db,
                params,
                "POST",
                "/api/v1/remote/assist/set-code",
                Some(Value::Object(body)),
                device_id,
                15,
            )
        }

        "assistAllow" => {
            let allow = param_bool(params, &["allowAssist", "allow_assist", "allow"])
                .ok_or_else(|| "assistAllow requires boolean 'allowAssist'".to_string())?;
            let device_id = param_str(params, &["deviceId", "device_id"]);
            let mut body = Map::new();
            body.insert("allow_assist".into(), Value::Bool(allow));
            if let Some(did) = device_id {
                body.insert("device_id".into(), Value::String(did.to_string()));
            }
            authed_http_json(
                db,
                params,
                "POST",
                "/api/v1/remote/assist/allow",
                Some(Value::Object(body)),
                device_id,
                15,
            )
        }

        "assistStart" => {
            let assist_id = param_str(params, &["assistId", "assist_id"])
                .ok_or_else(|| "assistStart requires 'assistId'".to_string())?;
            let access_code = param_str(params, &["accessCode", "access_code"])
                .ok_or_else(|| "assistStart requires 'accessCode'".to_string())?;
            let mut body = Map::new();
            body.insert("assist_id".into(), Value::String(assist_id.to_string()));
            body.insert("access_code".into(), Value::String(access_code.to_string()));
            insert_opt_str(
                &mut body,
                "requester_device_id",
                params,
                &["requesterDeviceId", "requester_device_id"],
            );
            insert_opt_str(
                &mut body,
                "requester_name",
                params,
                &["requesterName", "requester_name"],
            );
            body.insert(
                "requester_platform".into(),
                Value::String(
                    param_str(params, &["requesterPlatform", "requester_platform"])
                        .unwrap_or("android")
                        .to_string(),
                ),
            );
            body.insert(
                "mode".into(),
                Value::String(param_str(params, &["mode"]).unwrap_or("desktop").to_string()),
            );
            authed_http_json(
                db,
                params,
                "POST",
                "/api/v1/remote/assist/start",
                Some(Value::Object(body)),
                None,
                15,
            )
        }

        "listRemoteSessions" => {
            let val =
                authed_http_json(db, params, "GET", "/api/v1/remote/sessions", None, None, 15)?;
            let sessions_arr: Vec<Value> = if let Some(arr) = val.as_array() {
                arr.clone()
            } else if let Some(arr) = val.get("sessions").and_then(Value::as_array) {
                arr.clone()
            } else {
                Vec::new()
            };
            crate::with_db(db, |conn| {
                let now = chrono::Utc::now().timestamp_millis();
                conn.execute("DELETE FROM tg_server_remote_sessions", [])
                    .map_err(|e| e.to_string())?;
                for sess in &sessions_arr {
                    if let Some(sid) = sess
                        .get("session_id")
                        .or_else(|| sess.get("sessionId"))
                        .and_then(Value::as_str)
                    {
                        conn.execute(
                            "INSERT OR REPLACE INTO tg_server_remote_sessions (session_id, session_json, updated_at_ms)
                             VALUES (?1, ?2, ?3)",
                            params![sid, sess.to_string(), now],
                        )
                        .map_err(|e| e.to_string())?;
                    }
                }
                Ok(())
            })?;
            if val.is_array() {
                Ok(json!({ "sessions": sessions_arr }))
            } else {
                Ok(val)
            }
        }

        "disconnectRemoteSession" => {
            let session_id = param_str(params, &["sessionId", "session_id"])
                .ok_or_else(|| "disconnectRemoteSession requires 'sessionId'".to_string())?;
            let val = authed_http_json(
                db,
                params,
                "POST",
                "/api/v1/remote/sessions/disconnect",
                Some(json!({ "session_id": session_id })),
                None,
                15,
            )?;
            crate::with_db(db, |conn| {
                let _ = conn.execute(
                    "DELETE FROM tg_server_remote_sessions WHERE session_id = ?1",
                    params![session_id],
                );
                Ok(())
            })?;
            Ok(val)
        }

        "disconnectAllRemoteSessions" => {
            let val = authed_http_json(
                db,
                params,
                "POST",
                "/api/v1/remote/sessions/disconnect-all",
                Some(json!({})),
                None,
                15,
            )?;
            crate::with_db(db, |conn| {
                let _ = conn.execute("DELETE FROM tg_server_remote_sessions", []);
                Ok(())
            })?;
            Ok(val)
        }

        "webrtcSignal" => {
            let relay_token = param_str(
                params,
                &[
                    "relayToken",
                    "relay_token",
                    "tempRelayToken",
                    "temp_relay_token",
                ],
            )
            .map(ToString::to_string);
            let (ep, bearer) = crate::with_db(db, |conn| {
                init_schema(conn)?;
                let ep = resolve_server_endpoint(conn, params);
                let tok = if let Some(rt) = relay_token {
                    rt
                } else {
                    require_server_auth(conn, params)?.1
                };
                Ok((ep, tok))
            })?;
            let body = if let Some(p) = params.get("payload").filter(|v| v.is_object()) {
                p.clone()
            } else {
                let mut m = params.as_object().cloned().unwrap_or_default();
                for k in [
                    "relayToken",
                    "relay_token",
                    "tempRelayToken",
                    "temp_relay_token",
                    "endpoint",
                    "serverUrl",
                    "token",
                    "authToken",
                ] {
                    m.remove(k);
                }
                Value::Object(m)
            };
            let opts = HttpOptions {
                bearer_token: Some(&bearer),
                device_id: param_str(params, &["deviceId", "device_id"]),
                extra_headers: &[],
                read_timeout_secs: 15,
            };
            desktop_http_json_ext(
                "POST",
                &format!("{ep}/api/v1/remote/webrtc/signal"),
                &opts,
                Some(body),
            )
        }

        "listMcpGrants" => {
            let val = authed_http_json(db, params, "GET", "/api/v1/mcp/grants", None, None, 15)?;
            let grants_arr: Vec<Value> = if let Some(arr) = val.as_array() {
                arr.clone()
            } else if let Some(arr) = val.get("grants").and_then(Value::as_array) {
                arr.clone()
            } else {
                Vec::new()
            };
            crate::with_db(db, |conn| {
                let now = chrono::Utc::now().timestamp_millis();
                conn.execute("DELETE FROM tg_server_mcp_grants", [])
                    .map_err(|e| e.to_string())?;
                for g in &grants_arr {
                    if let Some(fid) = g
                        .get("family_id")
                        .or_else(|| g.get("familyId"))
                        .and_then(Value::as_str)
                    {
                        conn.execute(
                            "INSERT OR REPLACE INTO tg_server_mcp_grants (family_id, grant_json, updated_at_ms)
                             VALUES (?1, ?2, ?3)",
                            params![fid, g.to_string(), now],
                        )
                        .map_err(|e| e.to_string())?;
                    }
                }
                Ok(())
            })?;
            if val.is_array() {
                Ok(json!({ "grants": grants_arr }))
            } else {
                Ok(val)
            }
        }

        "revokeMcpGrant" => {
            let family_id = param_str(params, &["familyId", "family_id"])
                .ok_or_else(|| "revokeMcpGrant requires 'familyId'".to_string())?;
            let encoded = url_encode_path_segment(family_id);
            let val = authed_http_json(
                db,
                params,
                "DELETE",
                &format!("/api/v1/mcp/grants/{encoded}"),
                None,
                None,
                15,
            )?;
            crate::with_db(db, |conn| {
                let _ = conn.execute(
                    "DELETE FROM tg_server_mcp_grants WHERE family_id = ?1",
                    params![family_id],
                );
                Ok(())
            })?;
            Ok(val)
        }

        "listMcpAudit" => {
            let limit = param_u64(params, &["limit"]).unwrap_or(50);
            authed_http_json(
                db,
                params,
                "GET",
                &format!("/api/v1/mcp/audit?limit={limit}"),
                None,
                None,
                15,
            )
        }

        other => Err(format!("Unsupported telegram.server method: {other}")),
    }
}

// ============================================================================
// Lock-Free Outbox & PTS Sync Helpers
// ============================================================================

fn handle_flush_outbox(db: &Mutex<Connection>) -> Result<Value, String> {
    let (ep, tok, pending) = crate::with_db(db, |conn| {
        init_schema(conn)?;
        let (ep, tok): (String, Option<String>) = conn
            .query_row(
                "SELECT server_endpoint, auth_token FROM tg_fence WHERE singleton_id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or_else(|_| (DEFAULT_DESKTOP_SERVER_URL.into(), None));

        let mut stmt = conn
            .prepare(
                "SELECT seq, payload_json FROM tg_server_outbox WHERE flushed = 0 ORDER BY seq ASC LIMIT 100",
            )
            .map_err(|e| e.to_string())?;
        let pending: Vec<(i64, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok((ep, tok, pending))
    })?;

    if pending.is_empty() {
        return Ok(json!({ "flushedCount": 0, "failedCount": 0, "pendingCount": 0 }));
    }

    // Phase 2: blocking HTTP loop completely unlocked!
    let post_url = format!("{}/api/v1/messages/send", ep.trim_end_matches('/'));
    let mut results: Vec<(i64, Result<String, String>)> = Vec::with_capacity(pending.len());
    let mut last_err = String::new();
    let mut saw_401 = false;

    for (seq, raw_payload) in &pending {
        let payload_val: Value =
            serde_json::from_str(raw_payload).unwrap_or_else(|_| json!({ "raw": raw_payload }));
        let body = if payload_val.get("chat_id").is_some() {
            payload_val
        } else {
            json!({ "seq": seq, "payload": payload_val })
        };
        let opts = HttpOptions {
            bearer_token: tok.as_deref().filter(|s| !s.is_empty()),
            device_id: None,
            extra_headers: &[],
            read_timeout_secs: 15,
        };
        match desktop_http_json_ext("POST", &post_url, &opts, Some(body)) {
            Ok(ack) => {
                let remote_id = ack
                    .get("message_id")
                    .or_else(|| ack.get("messageId"))
                    .or_else(|| ack.get("id"))
                    .and_then(|v| {
                        v.as_i64()
                            .map(|n| n.to_string())
                            .or_else(|| v.as_u64().map(|n| n.to_string()))
                            .or_else(|| v.as_str().map(ToString::to_string))
                    })
                    .unwrap_or_else(|| "ack_ok".to_string());
                results.push((*seq, Ok(remote_id)));
            }
            Err(e) => {
                if is_http_401(&e) {
                    saw_401 = true;
                }
                last_err = e.clone();
                results.push((*seq, Err(e)));
            }
        }
    }

    // Phase 3: write-through SQLite updates under brief lock
    let (flushed_count, failed_count, remaining) = crate::with_db(db, |conn| {
        if saw_401 {
            if let Some(ref failed_tok) = tok {
                let _ = clear_server_session_and_caches_if_token_matches(conn, failed_tok);
            }
        }
        let mut flushed_count = 0usize;
        let mut failed_count = 0usize;
        for (seq, res) in results {
            match res {
                Ok(remote_id) => {
                    conn.execute(
                        "UPDATE tg_server_outbox SET flushed = 1, remote_id = ?1, flushed_at_ms = ?2 WHERE seq = ?3",
                        params![remote_id, chrono::Utc::now().timestamp_millis(), seq],
                    )
                    .map_err(|e| e.to_string())?;
                    flushed_count += 1;
                }
                Err(_) => {
                    let _ = conn.execute(
                        "UPDATE tg_server_outbox SET retry_count = retry_count + 1 WHERE seq = ?1",
                        params![seq],
                    );
                    failed_count += 1;
                }
            }
        }
        let remaining: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tg_server_outbox WHERE flushed = 0",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        Ok((flushed_count, failed_count, remaining))
    })?;

    if flushed_count == 0 && failed_count > 0 {
        return Err(format!(
            "SERVER_UNREACHABLE: Outbox flush to {post_url} failed ({last_err})"
        ));
    }

    Ok(json!({
        "flushedCount": flushed_count,
        "failedCount": failed_count,
        "pendingCount": remaining,
    }))
}

fn handle_sync_difference(db: &Mutex<Connection>, params: &Value) -> Result<Value, String> {
    let (ep, stored_pts, tok) = crate::with_db(db, |conn| {
        init_schema(conn)?;
        let (ep, pts, tok): (String, i64, Option<String>) = conn
            .query_row(
                "SELECT server_endpoint, sync_pts, auth_token FROM tg_fence WHERE singleton_id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap_or_else(|_| (DEFAULT_DESKTOP_SERVER_URL.into(), 1, None));
        Ok((ep, pts, tok))
    })?;

    let live = params
        .get("liveNetwork")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let req_pts = param_i64(params, &["pts"]).unwrap_or(stored_pts);

    if !live {
        return Ok(json!({
            "endpoint": ep,
            "previousPts": stored_pts,
            "pts": stored_pts,
            "newPts": stored_pts,
            "new_pts": stored_pts,
            "current_pts": stored_pts,
            "has_more": false,
            "snapshot_required": false,
            "onlineReachable": false,
            "ingestedMessages": 0,
            "messages": [],
            "updates": [],
        }));
    }

    let mut body = Map::new();
    body.insert("pts".into(), json!(req_pts));
    insert_opt_i64(&mut body, "chat_id", params, &["chatId", "chat_id"]);
    insert_opt_val(&mut body, "chat_pts", params, &["chatPts", "chat_pts"]);
    insert_opt_u64(&mut body, "limit", params, &["limit"]);

    let url = format!("{}/api/v1/sync/difference", ep.trim_end_matches('/'));
    let opts = HttpOptions {
        bearer_token: tok.as_deref().filter(|s| !s.is_empty()),
        device_id: None,
        extra_headers: &[],
        read_timeout_secs: 15,
    };
    let val = match desktop_http_json_ext("POST", &url, &opts, Some(Value::Object(body))) {
        Ok(v) => v,
        Err(e) => {
            if is_http_401(&e) {
                if let Some(ref failed_tok) = tok {
                    let _ = crate::with_db(db, |conn| {
                        clear_server_session_and_caches_if_token_matches(conn, failed_tok)
                    });
                }
            }
            return Err(if e.starts_with("SERVER_UNREACHABLE") {
                e
            } else {
                format!("SERVER_UNREACHABLE: syncDifference {e}")
            });
        }
    };

    let remote_updates = val
        .get("updates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let remote_messages = val
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let now = chrono::Utc::now().timestamp_millis();

    let (ingested_messages, next_pts) = crate::with_db(db, |conn| {
        let mut ingested = 0usize;
        for upd in &remote_updates {
            let utype = upd
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("messageAdded");
            if utype == "messageAdded"
                || utype == "NewMessage"
                || upd.get("text").is_some()
                || upd.get("message").is_some()
            {
                let m_obj = upd.get("message").unwrap_or(upd);
                if ingest_sync_item(conn, m_obj, now)? {
                    ingested += 1;
                }
            }
        }
        for msg in &remote_messages {
            if ingest_sync_item(conn, msg, now)? {
                ingested += 1;
            }
        }

        let next_pts = val
            .get("new_pts")
            .or_else(|| val.get("newPts"))
            .or_else(|| val.get("current_pts"))
            .or_else(|| val.get("pts"))
            .and_then(Value::as_i64)
            .unwrap_or(req_pts + (remote_updates.len() + remote_messages.len()) as i64);
        conn.execute(
            "UPDATE tg_fence SET sync_pts = ?1 WHERE singleton_id = 1",
            params![next_pts],
        )
        .map_err(|e| e.to_string())?;
        Ok((ingested, next_pts))
    })?;

    let current_pts = val
        .get("current_pts")
        .and_then(Value::as_i64)
        .unwrap_or(next_pts);
    let has_more = val
        .get("has_more")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let snapshot_required = val
        .get("snapshot_required")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    Ok(json!({
        "endpoint": ep,
        "previousPts": stored_pts,
        "pts": req_pts,
        "newPts": next_pts,
        "new_pts": next_pts,
        "current_pts": current_pts,
        "has_more": has_more,
        "snapshot_required": snapshot_required,
        "onlineReachable": true,
        "ingestedMessages": ingested_messages,
        "messages": remote_messages,
        "updates": remote_updates,
    }))
}

fn ingest_sync_item(conn: &Connection, m_obj: &Value, now: i64) -> Result<bool, String> {
    let conv_id = param_id_string(m_obj, &["conversationId", "chatId", "chat_id"])
        .unwrap_or_else(|| "0".to_string());
    let msg_id = param_id_string(m_obj, &["id", "messageId", "message_id"])
        .unwrap_or_else(|| format!("sync_msg_{}", uuid::Uuid::new_v4().simple()));
    let sender_id = param_id_string(m_obj, &["senderId", "sender_id", "fromId", "from_id"])
        .unwrap_or_else(|| "remote:peer".to_string());
    let text = m_obj
        .get("text")
        .and_then(Value::as_str)
        .or_else(|| m_obj.pointer("/content/data/text/text").and_then(Value::as_str))
        .unwrap_or("");
    let ts = param_i64(m_obj, &["createdAtMs", "date_ms", "created_at_ms"]).unwrap_or(now);
    if !text.is_empty() {
        crate::messaging::ingest_synced_message(conn, &conv_id, &msg_id, &sender_id, text, ts)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

// ============================================================================
// SQLite Write-Through Cache Helpers
// ============================================================================

fn extract_chat_id(chat: &Value) -> Option<i64> {
    param_i64(chat, &["chat_id", "id", "chatId"])
}

fn upsert_server_chat_cache(conn: &Connection, chat: &Value, now: i64) -> Result<(), String> {
    let Some(chat_id) = extract_chat_id(chat) else {
        return Ok(());
    };
    let is_pinned = chat
        .get("is_pinned")
        .or_else(|| chat.get("pinned"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_muted = chat
        .get("is_muted")
        .or_else(|| chat.get("muted"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mute_until_ms = chat
        .get("mute_until_ms")
        .or_else(|| chat.get("muteUntilMs"))
        .and_then(Value::as_i64);
    let is_archived = chat
        .get("is_archived")
        .or_else(|| chat.get("archived"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let last_msg_ms = chat
        .get("last_message_date_ms")
        .or_else(|| chat.get("lastMessageDateMs"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let pts = chat.get("pts").and_then(Value::as_i64).unwrap_or(0);

    conn.execute(
        "INSERT OR REPLACE INTO tg_server_chats
         (chat_id, is_pinned, is_muted, mute_until_ms, is_archived, last_message_date_ms, pts, chat_json, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            chat_id,
            if is_pinned { 1 } else { 0 },
            if is_muted { 1 } else { 0 },
            mute_until_ms,
            if is_archived { 1 } else { 0 },
            last_msg_ms,
            pts,
            chat.to_string(),
            now,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn update_server_chat_flags_cache(
    conn: &Connection,
    chat_id: i64,
    is_pinned: Option<bool>,
    is_muted: Option<bool>,
    mute_until_ms: Option<Option<i64>>,
    is_archived: Option<bool>,
) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp_millis();
    let existing: Option<String> = conn
        .query_row(
            "SELECT chat_json FROM tg_server_chats WHERE chat_id = ?1",
            params![chat_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    let mut obj = existing
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or_else(|| json!({ "chat_id": chat_id, "id": chat_id }));
    if let Some(m) = obj.as_object_mut() {
        if let Some(p) = is_pinned {
            m.insert("is_pinned".into(), Value::Bool(p));
        }
        if let Some(mu) = is_muted {
            m.insert("is_muted".into(), Value::Bool(mu));
            if !mu {
                m.insert("mute_until_ms".into(), Value::Null);
                m.remove("muteUntilMs");
            }
        }
        if let Some(until_opt) = mute_until_ms {
            match until_opt {
                Some(until) => {
                    m.insert("mute_until_ms".into(), json!(until));
                }
                None => {
                    m.insert("mute_until_ms".into(), Value::Null);
                    m.remove("muteUntilMs");
                }
            }
        }
        if let Some(ar) = is_archived {
            m.insert("is_archived".into(), Value::Bool(ar));
        }
    }
    upsert_server_chat_cache(conn, &obj, now)
}

fn sync_pinned_messages_cache(
    conn: &Connection,
    chat_id: i64,
    val: &Value,
) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "DELETE FROM tg_server_pinned_messages WHERE chat_id = ?1",
        params![chat_id],
    )
    .map_err(|e| e.to_string())?;

    if let Some(pins) = val.get("pins").and_then(Value::as_array) {
        for pin in pins {
            if let Some(mid) = param_i64(pin, &["message_id", "messageId", "id"]) {
                conn.execute(
                    "INSERT OR REPLACE INTO tg_server_pinned_messages (chat_id, message_id, pin_json, updated_at_ms)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![chat_id, mid, pin.to_string(), now],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    } else if let Some(ids) = val.get("pinned_message_ids").and_then(Value::as_array) {
        for id_v in ids {
            if let Some(mid) = id_v.as_i64() {
                conn.execute(
                    "INSERT OR REPLACE INTO tg_server_pinned_messages (chat_id, message_id, pin_json, updated_at_ms)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![chat_id, mid, json!({ "message_id": mid }).to_string(), now],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

fn upsert_server_folder_cache(
    conn: &Connection,
    folder: &Value,
    default_order: i64,
    now: i64,
) -> Result<(), String> {
    let Some(fid) = param_i64(folder, &["id", "folder_id", "folderId"]) else {
        return Ok(());
    };
    let sort_order =
        param_i64(folder, &["sort_order", "sortOrder"]).unwrap_or(default_order);
    conn.execute(
        "INSERT OR REPLACE INTO tg_server_folders (folder_id, sort_order, folder_json, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4)",
        params![fid, sort_order, folder.to_string(), now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn bot_cache_key(source: &str, bot: &Value) -> Option<String> {
    param_id_string(
        bot,
        &[
            "bot_user_id",
            "agent_id",
            "bot_id",
            "id",
            "username",
            "botUsername",
        ],
    )
    .map(|k| format!("{source}:{k}"))
}

fn replace_server_bots_cache(
    conn: &Connection,
    source: &str,
    bots: &[Value],
) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp_millis();
    conn.execute("DELETE FROM tg_server_bots WHERE source = ?1", params![source])
        .map_err(|e| e.to_string())?;
    for bot in bots {
        if let Some(key) = bot_cache_key(source, bot) {
            conn.execute(
                "INSERT OR REPLACE INTO tg_server_bots (bot_key, source, bot_json, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4)",
                params![key, source, bot.to_string(), now],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn upsert_server_bot_cache(conn: &Connection, source: &str, bot: &Value) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp_millis();
    if let Some(key) = bot_cache_key(source, bot) {
        conn.execute(
            "INSERT OR REPLACE INTO tg_server_bots (bot_key, source, bot_json, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4)",
            params![key, source, bot.to_string(), now],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn upsert_server_computer_cache(conn: &Connection, node: &Value, now: i64) -> Result<(), String> {
    let Some(node_id) = param_str(node, &["node_id", "nodeId", "id"]) else {
        return Ok(());
    };
    conn.execute(
        "INSERT OR REPLACE INTO tg_server_computers (node_id, node_json, updated_at_ms)
         VALUES (?1, ?2, ?3)",
        params![node_id, node.to_string(), now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ============================================================================
// Endpoint / Auth Resolution & Upgraded HTTP Transport
// ============================================================================

fn resolve_server_endpoint(conn: &Connection, params: &Value) -> String {
    if let Some(ep) = param_str(params, &["endpoint", "serverUrl"]) {
        let clean = ep.trim_end_matches('/').to_string();
        let _ = conn.execute(
            "UPDATE tg_fence SET server_endpoint = ?1 WHERE singleton_id = 1",
            params![&clean],
        );
        return clean;
    }
    conn.query_row(
        "SELECT server_endpoint FROM tg_fence WHERE singleton_id = 1",
        [],
        |r| r.get::<_, String>(0),
    )
    .unwrap_or_else(|_| DEFAULT_DESKTOP_SERVER_URL.to_string())
    .trim_end_matches('/')
    .to_string()
}

fn require_server_auth(conn: &Connection, params: &Value) -> Result<(String, String), String> {
    let ep = resolve_server_endpoint(conn, params);
    let token = param_str(params, &["token", "authToken"])
        .map(ToString::to_string)
        .or_else(|| {
            conn.query_row(
                "SELECT auth_token FROM tg_fence WHERE singleton_id = 1",
                [],
                |r| r.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten()
            .filter(|s| !s.trim().is_empty())
        })
        .ok_or_else(|| "AUTH_REQUIRED: Not logged in to gpui_telegram_server".to_string())?;
    Ok((ep, token))
}

fn persist_server_session(conn: &Connection, ep: &str, val: &Value) -> Result<(), String> {
    let token = val
        .get("token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Server auth response missing 'token'".to_string())?;
    let user_id = val
        .get("user_id")
        .or_else(|| val.pointer("/user/user_id"))
        .and_then(Value::as_i64);
    let username = val
        .get("username")
        .or_else(|| val.pointer("/user/username"))
        .and_then(Value::as_str);
    let first_name = val
        .get("first_name")
        .or_else(|| val.pointer("/user/first_name"))
        .and_then(Value::as_str);
    let session_id = val.get("session_id").and_then(Value::as_str);
    conn.execute(
        "UPDATE tg_fence
         SET server_endpoint = ?1, auth_token = ?2, session_user_id = ?3,
             session_username = ?4, session_first_name = ?5, session_id = ?6
         WHERE singleton_id = 1",
        params![
            ep.trim_end_matches('/'),
            token,
            user_id,
            username,
            first_name,
            session_id
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

struct HttpOptions<'a> {
    bearer_token: Option<&'a str>,
    device_id: Option<&'a str>,
    extra_headers: &'a [(&'a str, &'a str)],
    read_timeout_secs: u64,
}

impl Default for HttpOptions<'_> {
    fn default() -> Self {
        Self {
            bearer_token: None,
            device_id: None,
            extra_headers: &[],
            read_timeout_secs: 15,
        }
    }
}

fn authed_http_json(
    db: &Mutex<Connection>,
    params: &Value,
    method: &str,
    path: &str,
    body: Option<Value>,
    device_id: Option<&str>,
    read_timeout_secs: u64,
) -> Result<Value, String> {
    // Phase 1: brief DB lock to resolve endpoint & token
    let (ep, tok) = crate::with_db(db, |conn| {
        init_schema(conn)?;
        require_server_auth(conn, params)
    })?;
    // Phase 2: blocking HTTP I/O with ZERO mutexes held
    let opts = HttpOptions {
        bearer_token: Some(&tok),
        device_id,
        extra_headers: &[],
        read_timeout_secs,
    };
    let res = desktop_http_json_ext(method, &format!("{ep}{path}"), &opts, body);
    // Phase 3: automatic session & cache wipe on HTTP 401 (only if session token still matches)
    if let Err(ref e) = res {
        if is_http_401(e) {
            let _ = crate::with_db(db, |conn| {
                clear_server_session_and_caches_if_token_matches(conn, &tok)
            });
        }
    }
    res
}

fn desktop_http_json_ext(
    method: &str,
    url: &str,
    opts: &HttpOptions<'_>,
    body: Option<Value>,
) -> Result<Value, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(8))
        .timeout_read(std::time::Duration::from_secs(opts.read_timeout_secs))
        .build();
    let mut req = agent.request(method, url);
    if let Some(tok) = opts.bearer_token.filter(|s| !s.is_empty()) {
        req = req.set("Authorization", &format!("Bearer {tok}"));
    }
    if let Some(did) = opts.device_id.filter(|s| !s.is_empty()) {
        req = req.set("X-Device-Id", did);
    }
    for &(k, v) in opts.extra_headers {
        req = req.set(k, v);
    }
    let result = if let Some(payload) = body {
        req.set("Content-Type", "application/json")
            .send_json(payload)
    } else {
        req.call()
    };
    match result {
        Ok(resp) => {
            let text = resp.into_string().unwrap_or_default();
            if text.trim().is_empty() {
                Ok(json!({ "ok": true }))
            } else {
                serde_json::from_str::<Value>(&text)
                    .map_err(|e| format!("Invalid JSON from {url}: {e}"))
            }
        }
        Err(ureq::Error::Status(code, resp)) => {
            let raw = resp.into_string().unwrap_or_default();
            let detail = parse_http_error_detail(&raw);
            Err(format!("HTTP {code}: {}", detail.trim()))
        }
        Err(ureq::Error::Transport(tr)) => Err(format!("SERVER_UNREACHABLE: {tr}")),
    }
}

fn desktop_http_bytes(
    url: &str,
    bearer_token: Option<&str>,
    read_timeout_secs: u64,
) -> Result<(Vec<u8>, String, Option<String>), String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(8))
        .timeout_read(std::time::Duration::from_secs(read_timeout_secs))
        .build();
    let mut req = agent.get(url);
    if let Some(tok) = bearer_token.filter(|s| !s.is_empty()) {
        req = req.set("Authorization", &format!("Bearer {tok}"));
    }
    match req.call() {
        Ok(resp) => {
            let mime = resp
                .header("Content-Type")
                .unwrap_or("application/octet-stream")
                .to_string();
            let filename = resp
                .header("Content-Disposition")
                .and_then(parse_content_disposition_filename);
            let mut buf = Vec::new();
            resp.into_reader()
                .take(MAX_MEDIA_DOWNLOAD_BYTES)
                .read_to_end(&mut buf)
                .map_err(|e| format!("Failed reading media bytes from {url}: {e}"))?;
            Ok((buf, mime, filename))
        }
        Err(ureq::Error::Status(code, resp)) => {
            let raw = resp.into_string().unwrap_or_default();
            let detail = parse_http_error_detail(&raw);
            Err(format!("HTTP {code}: {}", detail.trim()))
        }
        Err(ureq::Error::Transport(tr)) => Err(format!("SERVER_UNREACHABLE: {tr}")),
    }
}

fn parse_http_error_detail(raw: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(raw) {
        let err_s = v.get("error").and_then(Value::as_str).map(str::trim);
        let msg_s = v.get("message").and_then(Value::as_str).map(str::trim);
        match (err_s, msg_s) {
            (Some(e), Some(m)) if !e.is_empty() && !m.is_empty() && e != m => {
                return format!("{e}: {m}");
            }
            (Some(e), _) if !e.is_empty() => return e.to_string(),
            (_, Some(m)) if !m.is_empty() => return m.to_string(),
            _ => {}
        }
    }
    raw.to_string()
}

fn parse_content_disposition_filename(header: &str) -> Option<String> {
    for part in header.split(';') {
        let trimmed = part.trim();
        if let Some(rest) = trimmed.strip_prefix("filename=") {
            let unquoted = rest.trim().trim_matches('"');
            if !unquoted.is_empty() {
                return Some(unquoted.to_string());
            }
        }
    }
    None
}

fn url_encode_query(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for b in raw.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Encodes a URL path segment while preserving RFC 3986 `:` (used in IDs like `cloud:42:prod-1`).
/// Literal dot segments `"."` and `".."` are percent-encoded so they cannot traverse URL paths.
pub fn url_encode_path_segment(raw: &str) -> String {
    if raw == "." {
        return "%2E".to_string();
    }
    if raw == ".." {
        return "%2E%2E".to_string();
    }
    let mut out = String::with_capacity(raw.len());
    for b in raw.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b':' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
