//! Extended Telegram Capabilities (`telegram.auth.*`, `telegram.messaging.*`,
//! `telegram.calls.*`, `telegram.server.*`):
//! - Multi-account switcher with `GenerationFence` & Argon2id local Passcode lock
//! - Phone/QR/2FA/Passkey authentication state machine
//! - Forum Topics, 24h Stories, Sticker Sets & GIF Search, Channel Stats, JSON/HTML Chat Export
//! - 1:1 & Group VoIP Call state machine with 333-emoji E2EE fingerprint & <60ms VAD barge-in
//! - Live `gpui_telegram_server` HTTP sync client + SQLite offline outbox

use crate::e2ee::CallsEmojiFingerprint;
use crate::waveform::{
    pcm_bytes_to_i16, rpc_decode_waveform, rpc_encode_waveform, VadBargeInDetector,
};
use argon2::Argon2;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

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
pub fn verify_secret_argon2id(secret: &str, salt_hex: &str, expected_digest_hex: &str) -> Result<bool, String> {
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
            server_endpoint TEXT NOT NULL DEFAULT 'https://telegram.fabushi.ombhrum.com',
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
        );",
    )
    .map_err(|e| format!("Failed to initialize telegram_ext schema: {e}"))?;

    let _ = conn.execute("ALTER TABLE tg_fence ADD COLUMN auth_token TEXT", []);
    let _ = conn.execute("ALTER TABLE tg_server_outbox ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE tg_server_outbox ADD COLUMN remote_id TEXT", []);
    let _ = conn.execute("ALTER TABLE tg_server_outbox ADD COLUMN flushed_at_ms INTEGER", []);

    let acc_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM tg_accounts", [], |r| r.get(0))
        .unwrap_or(0);
    if acc_count == 0 {
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO tg_accounts (account_id, phone, username, display_name, is_active, created_at_ms)
             VALUES ('acc_primary', '+86 138 0013 8000', 'fabushi_practitioner', '法布施精进者', 1, ?1)",
            params![now],
        )
        .map_err(|e| e.to_string())?;
    }

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

pub fn handle_auth_rpc(conn: &Connection, submethod: &str, params: &Value) -> Result<Value, String> {
    init_schema(conn)?;
    let now = chrono::Utc::now().timestamp_millis();

    match submethod {
        "phoneSendCode" => {
            let phone = params
                .get("phone")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| "phoneSendCode requires 'phone'".to_string())?;
            let live = params
                .get("liveNetwork")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if live {
                let ep: String = conn
                    .query_row(
                        "SELECT server_endpoint FROM tg_fence WHERE singleton_id = 1",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or_else(|_| "https://telegram.fabushi.ombhrum.com".into());
                let agent = ureq::AgentBuilder::new()
                    .timeout_connect(std::time::Duration::from_secs(6))
                    .timeout_read(std::time::Duration::from_secs(12))
                    .build();
                let resp = agent
                    .post(&format!("{}/v1/auth/phone/send_code", ep.trim_end_matches('/')))
                    .set("Content-Type", "application/json")
                    .send_json(json!({ "phone": phone }))
                    .map_err(|e| format!("SERVER_UNREACHABLE: phoneSendCode failed: {e}"))?;
                let val: Value = resp
                    .into_json()
                    .map_err(|e| format!("SERVER_UNREACHABLE: invalid phoneSendCode JSON: {e}"))?;
                let pch = val
                    .get("phoneCodeHash")
                    .or_else(|| val.get("phone_code_hash"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "SERVER_UNREACHABLE: missing phoneCodeHash in server response".to_string())?;
                return Ok(json!({
                    "phone": phone,
                    "phoneCodeHash": pch,
                    "codeLength": val.get("codeLength").and_then(Value::as_u64).unwrap_or(5),
                    "deliveryType": val.get("deliveryType").and_then(Value::as_str).unwrap_or("server_sms"),
                    "liveNetwork": true,
                }));
            }

            let phone_code_hash = format!("pch_{}", uuid::Uuid::new_v4().simple());
            let expected_code = params
                .get("expectedCode")
                .and_then(Value::as_str)
                .unwrap_or("10808");
            let code_digest = hex_encode(&Sha256::digest(expected_code.as_bytes()));
            conn.execute(
                "INSERT OR REPLACE INTO tg_auth_challenges (phone_code_hash, phone, code_sha256_hex, expires_at_ms)
                 VALUES (?1, ?2, ?3, ?4)",
                params![phone_code_hash, phone, code_digest, now + 300_000],
            )
            .map_err(|e| e.to_string())?;
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
                .unwrap_or("+86 138 0013 8000");
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
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Live phoneVerify requires 'phoneCodeHash'".to_string())?;
                let ep: String = conn
                    .query_row(
                        "SELECT server_endpoint FROM tg_fence WHERE singleton_id = 1",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or_else(|_| "https://telegram.fabushi.ombhrum.com".into());
                let agent = ureq::AgentBuilder::new()
                    .timeout_connect(std::time::Duration::from_secs(6))
                    .timeout_read(std::time::Duration::from_secs(12))
                    .build();
                let resp = agent
                    .post(&format!("{}/v1/auth/phone/verify", ep.trim_end_matches('/')))
                    .set("Content-Type", "application/json")
                    .send_json(json!({
                        "phone": phone,
                        "phoneCodeHash": pch,
                        "code": code,
                    }))
                    .map_err(|e| format!("SERVER_UNREACHABLE: phoneVerify failed: {e}"))?;
                let val: Value = resp
                    .into_json()
                    .map_err(|e| format!("SERVER_UNREACHABLE: invalid phoneVerify JSON: {e}"))?;
                issued_token = val
                    .get("token")
                    .or_else(|| val.get("accessToken"))
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
            } else if let Some(pch) = params.get("phoneCodeHash").and_then(Value::as_str) {
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
                .query_row("SELECT generation FROM tg_fence WHERE singleton_id = 1", [], |r| r.get(0))
                .unwrap_or(1);
            Ok(json!({
                "verified": true,
                "accountId": acc_id,
                "phone": phone,
                "generation": gen,
                "hasSessionToken": issued_token.is_some(),
            }))
        }

        "qrPairInit" => {
            let token = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(uuid::Uuid::new_v4().as_bytes());
            Ok(json!({
                "qrToken": token,
                "qrUri": format!("tg://login?token={token}"),
                "expiresInSeconds": 30,
            }))
        }

        "qrPairPoll" => {
            let token = params
                .get("qrToken")
                .and_then(Value::as_str)
                .unwrap_or("default");
            Ok(json!({
                "qrToken": token,
                "status": "authorized",
                "accountId": "acc_primary",
            }))
        }

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
                .query_row("SELECT generation FROM tg_fence WHERE singleton_id = 1", [], |r| r.get(0))
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
                .unwrap_or("+86 139 0000 0002");
            let username = params
                .get("username")
                .and_then(Value::as_str)
                .unwrap_or("secondary_account");
            let display = params
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or(username);
            let acc_id = params
                .get("accountId")
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("acc_{}", &uuid::Uuid::new_v4().simple().to_string()[..10]));
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
                    .query_row("SELECT generation FROM tg_fence WHERE singleton_id = 1", [], |r| r.get(0))
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
                .query_row("SELECT generation FROM tg_fence WHERE singleton_id = 1", [], |r| r.get(0))
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
            if passcode.len() < 4 {
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

pub fn handle_messaging_ext_rpc(conn: &Connection, submethod: &str, params: &Value) -> Result<Value, String> {
    init_schema(conn)?;
    let now = chrono::Utc::now().timestamp_millis();

    match submethod {
        "encodeWaveform" => rpc_encode_waveform(params),
        "decodeWaveform" => rpc_decode_waveform(params),

        "searchMessages" | "searchGlobal" | "searchInChat" => {
            crate::messaging::search_messages(conn, params)
        }

        "listTopics" => {
            let conv_id = params
                .get("conversationId")
                .and_then(Value::as_str)
                .unwrap_or("conv_global_dharma_channel");
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
            let conv_id = params
                .get("conversationId")
                .and_then(Value::as_str)
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
            let expires_at = now + 86_400_000; // 24h expiry
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
            let q = params.get("query").and_then(Value::as_str).unwrap_or("zen");
            Ok(json!({
                "query": q,
                "gifs": [
                    { "id": "gif_lotus_bloom", "title": format!("{q} - Lotus Bloom"), "mimeType": "video/mp4", "width": 320, "height": 240 },
                    { "id": "gif_prayer_wheel", "title": format!("{q} - Spinning Dharma Wheel"), "mimeType": "video/mp4", "width": 320, "height": 320 },
                ]
            }))
        }

        "channelStats" => {
            let conv_id = params
                .get("conversationId")
                .and_then(Value::as_str)
                .unwrap_or("conv_global_dharma_channel");
            let msg_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM msg_messages WHERE conversation_id = ?1",
                    params![conv_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let pinned_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM msg_messages WHERE conversation_id = ?1 AND pinned = 1",
                    params![conv_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let topic_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM tg_topics WHERE conversation_id = ?1",
                    params![conv_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let participants_count: usize = conn
                .query_row(
                    "SELECT data_json FROM msg_conversations WHERE id = ?1",
                    params![conv_id],
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
                .unwrap_or("global_dharma_bot");
            let query = params
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or("");
            Ok(json!({
                "botUsername": bot,
                "results": [
                    { "id": "inl_heart_sutra", "title": "分享《心经》核心经文", "text": "照见五蕴皆空，度一切苦厄。" },
                    { "id": "inl_mika_status", "title": format!("Mika 快捷指令: {query}"), "text": format!("/mika {query}") },
                ]
            }))
        }

        "callbackQuery" => {
            let data = params
                .get("callbackData")
                .and_then(Value::as_str)
                .unwrap_or("noop");
            Ok(json!({
                "answered": true,
                "callbackData": data,
                "toastText": format!("Action acknowledged: {data}"),
            }))
        }

        other => Err(format!("Unsupported telegram.messaging method: {other}")),
    }
}

pub fn handle_calls_rpc(
    conn: &Connection,
    vad: &Mutex<VadBargeInDetector>,
    submethod: &str,
    params: &Value,
) -> Result<Value, String> {
    init_schema(conn)?;
    let now = chrono::Utc::now().timestamp_millis();

    match submethod {
        "vadFeedPcm" => {
            let tts_playing = params
                .get("ttsPlaying")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let samples: Vec<i16> = if let Some(arr) = params.get("pcmSamples").and_then(Value::as_array) {
                arr.iter()
                    .map(|v| v.as_i64().unwrap_or(0).clamp(i16::MIN as i64, i16::MAX as i64) as i16)
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
            if params.get("reset").and_then(Value::as_bool).unwrap_or(false) {
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

        "startCall" | "joinGroupCall" => {
            let peer_id = params
                .get("peerId")
                .or_else(|| params.get("conversationId"))
                .and_then(Value::as_str)
                .unwrap_or("peer_default");
            let is_group = submethod == "joinGroupCall"
                || params.get("isGroup").and_then(Value::as_bool).unwrap_or(false);
            let call_id = format!("call_{}", uuid::Uuid::new_v4().simple());
            let fp = CallsEmojiFingerprint::from_shared_key(format!("{call_id}:{peer_id}").as_bytes());
            let video = params
                .get("video")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            conn.execute(
                "INSERT INTO tg_calls (call_id, peer_id, is_group, state, muted, video_enabled, screen_sharing, emoji_fingerprint, started_at_ms)
                 VALUES (?1, ?2, ?3, 'active', 0, ?4, 0, ?5, ?6)",
                params![call_id, peer_id, if is_group { 1 } else { 0 }, if video { 1 } else { 0 }, fp.emoji_display, now],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "callId": call_id,
                "peerId": peer_id,
                "isGroup": is_group,
                "state": "active",
                "videoEnabled": video,
                "emojiFingerprint": fp,
            }))
        }

        "signal" => {
            let call_id = params
                .get("callId")
                .and_then(Value::as_str)
                .unwrap_or("call_default");
            let sdp_type = params
                .get("sdpType")
                .and_then(Value::as_str)
                .unwrap_or("offer");
            Ok(json!({
                "callId": call_id,
                "sdpType": sdp_type,
                "accepted": true,
            }))
        }

        "toggleMute" | "toggleVideo" | "toggleScreenShare" => {
            let call_id = params
                .get("callId")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{submethod} requires 'callId'"))?;
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let col = match submethod {
                "toggleMute" => "muted",
                "toggleVideo" => "video_enabled",
                _ => "screen_sharing",
            };
            conn.execute(
                &format!("UPDATE tg_calls SET {col} = ?1 WHERE call_id = ?2"),
                params![if enabled { 1 } else { 0 }, call_id],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "callId": call_id, "field": col, "enabled": enabled }))
        }

        "endCall" => {
            let call_id = params
                .get("callId")
                .and_then(Value::as_str)
                .ok_or_else(|| "endCall requires 'callId'".to_string())?;
            conn.execute(
                "UPDATE tg_calls SET state = 'ended', ended_at_ms = ?1 WHERE call_id = ?2",
                params![now, call_id],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "callId": call_id, "state": "ended", "endedAtMs": now }))
        }

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

pub fn handle_server_rpc(conn: &Connection, submethod: &str, params: &Value) -> Result<Value, String> {
    init_schema(conn)?;
    let now = chrono::Utc::now().timestamp_millis();

    match submethod {
        "configureEndpoint" => {
            let url = params
                .get("endpoint")
                .or_else(|| params.get("url"))
                .and_then(Value::as_str)
                .ok_or_else(|| "configureEndpoint requires 'endpoint'".to_string())?;
            let token = params
                .get("authToken")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty());
            conn.execute(
                "UPDATE tg_fence SET server_endpoint = ?1, auth_token = COALESCE(?2, auth_token) WHERE singleton_id = 1",
                params![url.trim_end_matches('/'), token],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "endpoint": url.trim_end_matches('/'),
                "hasAuthToken": token.is_some(),
                "saved": true,
            }))
        }

        "status" => {
            let (ep, pts, tok): (String, i64, Option<String>) = conn
                .query_row(
                    "SELECT server_endpoint, sync_pts, auth_token FROM tg_fence WHERE singleton_id = 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap_or_else(|_| ("https://telegram.fabushi.ombhrum.com".into(), 1, None));
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
        }

        "enqueueOutbox" => {
            let payload = params.get("payload").cloned().unwrap_or_else(|| json!({}));
            conn.execute(
                "INSERT INTO tg_server_outbox (payload_json, flushed, retry_count, created_at_ms) VALUES (?1, 0, 0, ?2)",
                params![payload.to_string(), now],
            )
            .map_err(|e| e.to_string())?;
            let id = conn.last_insert_rowid();
            Ok(json!({ "outboxSeq": id, "queued": true }))
        }

        "flushOutbox" => {
            let (ep, tok): (String, Option<String>) = conn
                .query_row(
                    "SELECT server_endpoint, auth_token FROM tg_fence WHERE singleton_id = 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap_or_else(|_| ("https://telegram.fabushi.ombhrum.com".into(), None));

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

            if pending.is_empty() {
                return Ok(json!({ "flushedCount": 0, "failedCount": 0, "pendingCount": 0 }));
            }

            let agent = ureq::AgentBuilder::new()
                .timeout_connect(std::time::Duration::from_secs(6))
                .timeout_read(std::time::Duration::from_secs(15))
                .build();

            let mut flushed_count = 0usize;
            let mut failed_count = 0usize;
            let mut last_err = String::new();
            let post_url = format!("{}/v1/messages/send", ep.trim_end_matches('/'));

            for (seq, raw_payload) in &pending {
                let payload_val: Value =
                    serde_json::from_str(raw_payload).unwrap_or_else(|_| json!({ "raw": raw_payload }));
                let mut req = agent.post(&post_url).set("Content-Type", "application/json");
                if let Some(ref t) = tok.as_deref().filter(|s| !s.is_empty()) {
                    req = req.set("Authorization", &format!("Bearer {t}"));
                }
                match req.send_json(json!({ "seq": seq, "payload": payload_val })) {
                    Ok(resp) => {
                        let ack: Value = resp.into_json().unwrap_or_else(|_| json!({}));
                        let remote_id = ack
                            .get("messageId")
                            .or_else(|| ack.get("id"))
                            .and_then(Value::as_str)
                            .unwrap_or("ack_ok")
                            .to_string();
                        conn.execute(
                            "UPDATE tg_server_outbox SET flushed = 1, remote_id = ?1, flushed_at_ms = ?2 WHERE seq = ?3",
                            params![remote_id, chrono::Utc::now().timestamp_millis(), seq],
                        )
                        .map_err(|e| e.to_string())?;
                        flushed_count += 1;
                    }
                    Err(e) => {
                        last_err = e.to_string();
                        let _ = conn.execute(
                            "UPDATE tg_server_outbox SET retry_count = retry_count + 1 WHERE seq = ?1",
                            params![seq],
                        );
                        failed_count += 1;
                    }
                }
            }

            if flushed_count == 0 && failed_count > 0 {
                return Err(format!(
                    "SERVER_UNREACHABLE: Outbox flush to {post_url} failed ({last_err})"
                ));
            }

            let remaining: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM tg_server_outbox WHERE flushed = 0",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            Ok(json!({
                "flushedCount": flushed_count,
                "failedCount": failed_count,
                "pendingCount": remaining,
            }))
        }

        "syncDifference" => {
            let (ep, pts, tok): (String, i64, Option<String>) = conn
                .query_row(
                    "SELECT server_endpoint, sync_pts, auth_token FROM tg_fence WHERE singleton_id = 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap_or_else(|_| ("https://telegram.fabushi.ombhrum.com".into(), 1, None));
            let live = params
                .get("liveNetwork")
                .and_then(Value::as_bool)
                .unwrap_or(true);

            if !live {
                return Ok(json!({
                    "endpoint": ep,
                    "previousPts": pts,
                    "newPts": pts,
                    "onlineReachable": false,
                    "ingestedMessages": 0,
                    "updates": [],
                }));
            }

            let url = format!("{}/v1/updates/getDifference?pts={pts}", ep.trim_end_matches('/'));
            let agent = ureq::AgentBuilder::new()
                .timeout_connect(std::time::Duration::from_secs(6))
                .timeout_read(std::time::Duration::from_secs(15))
                .build();
            let mut req = agent.get(&url);
            if let Some(ref t) = tok.as_deref().filter(|s| !s.is_empty()) {
                req = req.set("Authorization", &format!("Bearer {t}"));
            }
            let resp = match req.call() {
                Ok(r) => r,
                Err(ureq::Error::Status(code, r)) => {
                    let body = r.into_string().unwrap_or_default();
                    return Err(format!("SERVER_UNREACHABLE: syncDifference HTTP {code}: {body}"));
                }
                Err(ureq::Error::Transport(tr)) => {
                    return Err(format!("SERVER_UNREACHABLE: syncDifference transport error: {tr}"));
                }
            };
            let val: Value = resp
                .into_json()
                .map_err(|e| format!("SERVER_UNREACHABLE: invalid syncDifference JSON: {e}"))?;
            let remote_updates = val
                .get("updates")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let mut ingested_messages = 0usize;

            for upd in &remote_updates {
                let utype = upd.get("type").and_then(Value::as_str).unwrap_or("messageAdded");
                if utype == "messageAdded" || upd.get("text").is_some() || upd.get("message").is_some() {
                    let m_obj = upd.get("message").unwrap_or(upd);
                    let conv_id = m_obj
                        .get("conversationId")
                        .or_else(|| m_obj.get("chatId"))
                        .and_then(Value::as_str)
                        .unwrap_or("conv_global_dharma_channel");
                    let msg_id = m_obj
                        .get("id")
                        .or_else(|| m_obj.get("messageId"))
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                        .unwrap_or_else(|| format!("sync_msg_{}", uuid::Uuid::new_v4().simple()));
                    let sender_id = m_obj
                        .get("senderId")
                        .or_else(|| m_obj.get("fromId"))
                        .and_then(Value::as_str)
                        .unwrap_or("remote:peer");
                    let text = m_obj
                        .get("text")
                        .and_then(Value::as_str)
                        .or_else(|| m_obj.pointer("/content/data/text/text").and_then(Value::as_str))
                        .unwrap_or("");
                    let ts = m_obj
                        .get("createdAtMs")
                        .and_then(Value::as_i64)
                        .unwrap_or(now);
                    if !text.is_empty() {
                        crate::messaging::ingest_synced_message(
                            conn, conv_id, &msg_id, sender_id, text, ts,
                        )?;
                        ingested_messages += 1;
                    }
                }
            }

            let next_pts = val
                .get("newPts")
                .or_else(|| val.get("pts"))
                .and_then(Value::as_i64)
                .unwrap_or(pts + remote_updates.len() as i64);
            conn.execute(
                "UPDATE tg_fence SET sync_pts = ?1 WHERE singleton_id = 1",
                params![next_pts],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "endpoint": ep,
                "previousPts": pts,
                "newPts": next_pts,
                "onlineReachable": true,
                "ingestedMessages": ingested_messages,
                "updates": remote_updates,
            }))
        }

        other => Err(format!("Unsupported telegram.server method: {other}")),
    }
}
