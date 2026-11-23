use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use gpui_telegram_proto::user_vault::Argon2KdfParamsDto;
use hkdf::Hkdf;
use mahayana_app_host::telegram_ext::normalize_auth_identifier;
use mahayana_app_host::user_vault::{
    derive_split_key_v2, derive_split_key_v2_b64, SPLIT_KEY_AUTH_INFO, SPLIT_KEY_VAULT_INFO,
};
use mahayana_app_host::MahayanaAndroidHost;
use serde_json::{json, Value};
use sha2::Sha256;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "fabushi_v240_splitkey_{}_{}",
        tag,
        uuid::Uuid::new_v4().simple()
    ))
}

#[derive(Debug, Clone)]
struct CapturedReq {
    method: String,
    path: String,
    body: String,
}

impl CapturedReq {
    fn json_body(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
}

fn read_req(stream: &mut TcpStream) -> Option<CapturedReq> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let header_end;
    loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
    }
    let header_str = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = header_str.lines();
    let first = lines.next()?;
    let mut parts = first.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut content_length = 0usize;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    while buf.len() < header_end + content_length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = String::from_utf8_lossy(
        &buf[header_end..(header_end + content_length).min(buf.len())],
    )
    .to_string();
    Some(CapturedReq { method, path, body })
}

struct MockSplitKeyServer {
    base_url: String,
    recorded: Arc<Mutex<Vec<CapturedReq>>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

#[derive(Clone)]
struct StoredUser {
    user_id: i64,
    username: String,
    email: Option<String>,
    first_name: String,
    auth_version: i32,
    raw_password: Option<String>,
    auth_salt_b64: Option<String>,
    auth_verifier_b64: Option<String>,
    kdf_params: Argon2KdfParamsDto,
    two_factor_password: Option<String>,
}

impl MockSplitKeyServer {
    fn start(initial_users: Vec<StoredUser>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let users_map: Arc<Mutex<HashMap<String, StoredUser>>> = Arc::new(Mutex::new(
            initial_users
                .into_iter()
                .map(|u| (u.username.to_ascii_lowercase(), u))
                .collect(),
        ));
        let stop_c = stop.clone();
        let rec_c = recorded.clone();
        let users_c = users_map.clone();

        let handle = thread::spawn(move || {
            let mut next_uid = 1000i64;
            let mut active_tokens: HashMap<String, String> = HashMap::new();
            let mut tfa_challenges: HashMap<String, String> = HashMap::new();

            while !stop_c.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let Some(req) = read_req(&mut stream) else {
                            continue;
                        };
                        rec_c.lock().unwrap().push(req.clone());
                        let (path_only, query) = req
                            .path
                            .split_once('?')
                            .unwrap_or((req.path.as_str(), ""));
                        let body = req.json_body();
                        let (status, resp_json): (u16, Value) = match (req.method.as_str(), path_only)
                        {
                            ("GET", "/api/v1/auth/kdf-params") => {
                                let raw_u = query
                                    .split('&')
                                    .find_map(|kv| kv.strip_prefix("username="))
                                    .unwrap_or("");
                                let ident = percent_decode(raw_u).to_ascii_lowercase();
                                let guard = users_c.lock().unwrap();
                                let found = guard.values().find(|u| {
                                    u.username.eq_ignore_ascii_case(&ident)
                                        || u.email
                                            .as_deref()
                                            .map(|e| e.eq_ignore_ascii_case(&ident))
                                            .unwrap_or(false)
                                });
                                match found {
                                    Some(u) if u.auth_version == 2 => (
                                        200,
                                        json!({
                                            "auth_version": 2,
                                            "auth_salt_b64": u.auth_salt_b64.clone().unwrap_or_default(),
                                            "kdf_params": u.kdf_params,
                                        }),
                                    ),
                                    Some(_) => (
                                        200,
                                        json!({
                                            "auth_version": 1,
                                            "auth_salt_b64": "",
                                            "kdf_params": Argon2KdfParamsDto::default(),
                                        }),
                                    ),
                                    None => (
                                        200,
                                        json!({
                                            "auth_version": 2,
                                            "auth_salt_b64": B64.encode([7u8; 16]),
                                            "kdf_params": { "m_cost_kib": 4096, "t_cost": 1, "p_cost": 1 },
                                        }),
                                    ),
                                }
                            }
                            ("POST", "/api/v1/auth/register")
                            | ("POST", "/api/v1/auth/email/register") => {
                                let uname = body["username"].as_str().unwrap_or("").trim().to_string();
                                let fname = body["first_name"]
                                    .as_str()
                                    .unwrap_or(&uname)
                                    .to_string();
                                let email = body["email"].as_str().map(ToString::to_string);
                                let auth_ver = body["auth_version"].as_i64().unwrap_or(1) as i32;
                                next_uid += 1;
                                let u = if auth_ver == 2 {
                                    let salt = body["auth_salt_b64"].as_str().unwrap_or("").to_string();
                                    let verifier =
                                        body["auth_verifier_b64"].as_str().unwrap_or("").to_string();
                                    let kdf = Argon2KdfParamsDto {
                                        m_cost_kib: body["kdf_params"]["m_cost_kib"]
                                            .as_u64()
                                            .unwrap_or(65536)
                                            as u32,
                                        t_cost: body["kdf_params"]["t_cost"].as_u64().unwrap_or(3)
                                            as u32,
                                        p_cost: body["kdf_params"]["p_cost"].as_u64().unwrap_or(1)
                                            as u32,
                                    };
                                    StoredUser {
                                        user_id: next_uid,
                                        username: uname.clone(),
                                        email,
                                        first_name: fname.clone(),
                                        auth_version: 2,
                                        raw_password: None,
                                        auth_salt_b64: Some(salt),
                                        auth_verifier_b64: Some(verifier),
                                        kdf_params: kdf,
                                        two_factor_password: None,
                                    }
                                } else {
                                    StoredUser {
                                        user_id: next_uid,
                                        username: uname.clone(),
                                        email,
                                        first_name: fname.clone(),
                                        auth_version: 1,
                                        raw_password: body["password"]
                                            .as_str()
                                            .map(ToString::to_string),
                                        auth_salt_b64: None,
                                        auth_verifier_b64: None,
                                        kdf_params: Argon2KdfParamsDto::default(),
                                        two_factor_password: None,
                                    }
                                };
                                users_c
                                    .lock()
                                    .unwrap()
                                    .insert(uname.to_ascii_lowercase(), u.clone());
                                let tok = format!("tok_{next_uid}");
                                active_tokens.insert(tok.clone(), uname.to_ascii_lowercase());
                                (
                                    201,
                                    json!({
                                        "user_id": u.user_id,
                                        "username": u.username,
                                        "first_name": u.first_name,
                                        "session_id": format!("sess_{next_uid}"),
                                        "token": tok,
                                        "auth_version": u.auth_version,
                                    }),
                                )
                            }
                            ("POST", "/api/v1/auth/login") => {
                                let ident = body["username"].as_str().unwrap_or("").trim().to_string();
                                let guard = users_c.lock().unwrap();
                                let found = guard.values().find(|u| {
                                    u.username.eq_ignore_ascii_case(&ident)
                                        || u.email
                                            .as_deref()
                                            .map(|e| e.eq_ignore_ascii_case(&ident))
                                            .unwrap_or(false)
                                });
                                match found {
                                    Some(u) => {
                                        let ok = if u.auth_version == 2 {
                                            // Exact gpui_telegram_server behavior: v2 REJECTS raw password!
                                            body["auth_verifier_b64"].as_str()
                                                == u.auth_verifier_b64.as_deref()
                                        } else {
                                            body["password"].as_str() == u.raw_password.as_deref()
                                        };
                                        if !ok {
                                            (
                                                401,
                                                json!({
                                                    "error": "invalid_credentials",
                                                    "message": "Invalid username or password"
                                                }),
                                            )
                                        } else if u.two_factor_password.is_some() {
                                            let ctok = format!("tfa_chal_{}", u.user_id);
                                            tfa_challenges
                                                .insert(ctok.clone(), u.username.to_ascii_lowercase());
                                            (
                                                200,
                                                json!({
                                                    "requires_2fa": true,
                                                    "challenge_token": ctok,
                                                    "hint": "my-pet-name",
                                                    "user_id": u.user_id,
                                                }),
                                            )
                                        } else {
                                            let tok = format!("tok_login_{}", u.user_id);
                                            active_tokens
                                                .insert(tok.clone(), u.username.to_ascii_lowercase());
                                            (
                                                200,
                                                json!({
                                                    "user_id": u.user_id,
                                                    "username": u.username,
                                                    "first_name": u.first_name,
                                                    "session_id": format!("sess_login_{}", u.user_id),
                                                    "token": tok,
                                                    "auth_version": u.auth_version,
                                                }),
                                            )
                                        }
                                    }
                                    None => (
                                        401,
                                        json!({
                                            "error": "invalid_credentials",
                                            "message": "Invalid username or password"
                                        }),
                                    ),
                                }
                            }
                            ("POST", "/api/v1/auth/upgrade-split-key") => {
                                let old_pw = body["old_password"].as_str().unwrap_or("");
                                let salt = body["auth_salt_b64"].as_str().unwrap_or("").to_string();
                                let verifier =
                                    body["auth_verifier_b64"].as_str().unwrap_or("").to_string();
                                let kdf = Argon2KdfParamsDto {
                                    m_cost_kib: body["kdf_params"]["m_cost_kib"]
                                        .as_u64()
                                        .unwrap_or(4096)
                                        as u32,
                                    t_cost: body["kdf_params"]["t_cost"].as_u64().unwrap_or(1)
                                        as u32,
                                    p_cost: body["kdf_params"]["p_cost"].as_u64().unwrap_or(1)
                                        as u32,
                                };
                                let mut guard = users_c.lock().unwrap();
                                for u in guard.values_mut() {
                                    if u.auth_version == 1
                                        && u.raw_password.as_deref() == Some(old_pw)
                                    {
                                        u.auth_version = 2;
                                        u.raw_password = None;
                                        u.auth_salt_b64 = Some(salt.clone());
                                        u.auth_verifier_b64 = Some(verifier.clone());
                                        u.kdf_params = kdf;
                                    }
                                }
                                (200, json!({ "ok": true, "auth_version": 2 }))
                            }
                            ("POST", "/api/v1/auth/2fa/verify") => {
                                let ctok = body["challenge_token"].as_str().unwrap_or("");
                                let cpw = body["cloud_password"].as_str().unwrap_or("");
                                if let Some(uname_lc) = tfa_challenges.get(ctok).cloned() {
                                    let guard = users_c.lock().unwrap();
                                    if let Some(u) = guard.get(&uname_lc) {
                                        if u.two_factor_password.as_deref() == Some(cpw) {
                                            let tok = format!("tok_2fa_{}", u.user_id);
                                            (
                                                200,
                                                json!({
                                                    "user_id": u.user_id,
                                                    "username": u.username,
                                                    "first_name": u.first_name,
                                                    "session_id": format!("sess_2fa_{}", u.user_id),
                                                    "token": tok,
                                                    "auth_version": u.auth_version,
                                                }),
                                            )
                                        } else {
                                            (
                                                401,
                                                json!({
                                                    "error": "invalid_cloud_password",
                                                    "message": "Incorrect 2FA cloud password"
                                                }),
                                            )
                                        }
                                    } else {
                                        (401, json!({ "error": "invalid_challenge" }))
                                    }
                                } else {
                                    (401, json!({ "error": "invalid_challenge" }))
                                }
                            }
                            ("POST", "/api/v1/contacts/add") => (
                                200,
                                json!({
                                    "chat_id": 9001,
                                    "received_is_bot": body.get("is_bot").cloned().unwrap_or(Value::Null),
                                    "received_first_name": body.get("first_name").cloned().unwrap_or(Value::Null),
                                    "received_username": body.get("username").cloned().unwrap_or(Value::Null),
                                }),
                            ),
                            ("POST", "/api/v1/auth/logout") => (200, json!({ "ok": true })),
                            _ => (404, json!({ "error": "not_found" })),
                        };

                        let resp_bytes = resp_json.to_string().into_bytes();
                        let status_text = match status {
                            200 => "OK",
                            201 => "Created",
                            401 => "Unauthorized",
                            _ => "Error",
                        };
                        let header = format!(
                            "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            resp_bytes.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&resp_bytes);
                        let _ = stream.flush();
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            recorded,
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for MockSplitKeyServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.base_url.trim_start_matches("http://"));
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(val) = u8::from_str_radix(hex, 16) {
                    out.push(val);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// AC-1: Verify bit-exact parity with Desktop `bot-browser/src/e2ee_sync.rs::derive_split_key`.
#[test]
fn split_key_v2_matches_desktop_bot_browser_bit_for_bit() {
    let password = "CorrectHorseBatteryStaple!42";
    let salt: [u8; 16] = [
        0x10, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
        0x01,
    ];
    let salt_b64 = B64.encode(salt);
    let kdf = Argon2KdfParamsDto {
        m_cost_kib: 4_096,
        t_cost: 1,
        p_cost: 1,
    };

    // Independent Desktop reference calculation (`bot-browser/src/e2ee_sync.rs:484-503`)
    let argon = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(4_096, 1, 1, Some(32)).unwrap(),
    );
    let mut master_key = [0u8; 32];
    argon
        .hash_password_into(password.as_bytes(), &salt, &mut master_key)
        .unwrap();
    let hk = Hkdf::<Sha256>::new(None, &master_key);
    let mut expected_verifier = [0u8; 32];
    let mut expected_kek = [0u8; 32];
    hk.expand(b"gpui-telegram/auth-verifier/v2", &mut expected_verifier)
        .unwrap();
    hk.expand(b"gpui-telegram/vault-kek/v2", &mut expected_kek)
        .unwrap();

    assert_eq!(SPLIT_KEY_AUTH_INFO, b"gpui-telegram/auth-verifier/v2");
    assert_eq!(SPLIT_KEY_VAULT_INFO, b"gpui-telegram/vault-kek/v2");

    let (actual_verifier, actual_kek) = derive_split_key_v2(password, &salt, kdf).unwrap();
    assert_eq!(actual_verifier, expected_verifier);
    assert_eq!(actual_kek, expected_kek);

    let (actual_verifier_b64, actual_kek2) =
        derive_split_key_v2_b64(password, &salt_b64, kdf).unwrap();
    assert_eq!(actual_verifier_b64, B64.encode(expected_verifier));
    assert_eq!(actual_kek2, expected_kek);
}

#[test]
fn identifier_normalization_strips_at_handle_but_preserves_emails() {
    assert_eq!(normalize_auth_identifier(" @alice "), "alice");
    assert_eq!(normalize_auth_identifier("alice"), "alice");
    assert_eq!(
        normalize_auth_identifier(" alice@example.com "),
        "alice@example.com"
    );
    assert_eq!(normalize_auth_identifier("@botfather"), "botfather");
}

/// AC-2: Register (v2) -> Logout -> Re-login (v2) roundtrip + zero raw password on wire.
#[test]
fn split_key_v2_register_logout_relogin_roundtrip() {
    let srv = MockSplitKeyServer::start(vec![]);
    let dir = temp_dir("reg_logout_relogin");
    let host = MahayanaAndroidHost::new(&dir, false).unwrap();
    let fast_kdf = json!({ "m_cost_kib": 4096, "t_cost": 1, "p_cost": 1 });

    // 1. Register on Android (must use Split-Key V2 with zero raw password)
    let reg = host
        .dispatch(
            "telegram.server.register",
            &json!({
                "endpoint": srv.base_url,
                "username": "@bob_android",
                "password": "MySecretPassword99!",
                "firstName": "Bob",
                "kdfParams": fast_kdf,
            }),
        )
        .expect("register v2");
    assert_eq!(reg["auth_version"], 2);
    assert_eq!(reg["username"], "bob_android");

    let reg_req = srv
        .recorded
        .lock()
        .unwrap()
        .iter()
        .find(|r| r.path == "/api/v1/auth/register")
        .cloned()
        .expect("register request captured");
    let reg_body = reg_req.json_body();
    assert_eq!(reg_body["auth_version"], 2);
    assert!(
        reg_body.get("password").is_none(),
        "Split-Key V2 register must NOT send raw password"
    );
    assert!(reg_body["auth_verifier_b64"].as_str().unwrap().len() >= 40);

    // 2. Logout cleanly
    host.dispatch("telegram.server.logout", &json!({}))
        .expect("logout");
    let sess_after_logout = host
        .dispatch("telegram.server.session", &json!({}))
        .expect("session");
    assert_eq!(sess_after_logout["authenticated"], false);

    // 3. Re-login with leading `@` — must query kdf-params and authenticate with v2 verifier!
    let relogin = host
        .dispatch(
            "telegram.server.login",
            &json!({
                "endpoint": srv.base_url,
                "username": "@bob_android",
                "password": "MySecretPassword99!",
            }),
        )
        .expect("re-login v2 after logout");
    assert_eq!(relogin["username"], "bob_android");
    assert!(relogin["token"].as_str().unwrap().starts_with("tok_login_"));

    let login_req = srv
        .recorded
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|r| r.path == "/api/v1/auth/login")
        .cloned()
        .expect("login request captured");
    assert_eq!(login_req.json_body()["password"], "");
    assert!(login_req.json_body()["auth_verifier_b64"].is_string());

    let _ = std::fs::remove_dir_all(dir);
}

/// AC-2: Desktop-registered V2 user logs in on Android, and V1 user upgrades to V2 then re-logs in.
#[test]
fn desktop_v2_account_and_v1_upgraded_account_login_on_android() {
    let kdf = Argon2KdfParamsDto {
        m_cost_kib: 4_096,
        t_cost: 1,
        p_cost: 1,
    };
    let desktop_salt_b64 = B64.encode([0x42u8; 16]);
    let (desktop_verifier, _) =
        derive_split_key_v2_b64("DesktopPass#2026", &desktop_salt_b64, kdf).unwrap();

    let desktop_user = StoredUser {
        user_id: 501,
        username: "desktop_alice".into(),
        email: Some("alice@fabushi.app".into()),
        first_name: "Alice".into(),
        auth_version: 2,
        raw_password: None,
        auth_salt_b64: Some(desktop_salt_b64),
        auth_verifier_b64: Some(desktop_verifier),
        kdf_params: kdf,
        two_factor_password: None,
    };
    let legacy_v1_user = StoredUser {
        user_id: 502,
        username: "legacy_charlie".into(),
        email: None,
        first_name: "Charlie".into(),
        auth_version: 1,
        raw_password: Some("OldV1Password!".into()),
        auth_salt_b64: None,
        auth_verifier_b64: None,
        kdf_params: Argon2KdfParamsDto::default(),
        two_factor_password: None,
    };

    let srv = MockSplitKeyServer::start(vec![desktop_user, legacy_v1_user]);
    let dir = temp_dir("desktop_interop");
    let host = MahayanaAndroidHost::new(&dir, false).unwrap();

    // 1. Desktop V2 account logs in by email on Android
    let res1 = host
        .dispatch(
            "telegram.server.login",
            &json!({
                "endpoint": srv.base_url,
                "username": "alice@fabushi.app",
                "password": "DesktopPass#2026",
            }),
        )
        .expect("Desktop v2 account login by email on Android");
    assert_eq!(res1["user_id"], 501);

    // 2. Wrong password on Desktop V2 account returns 401
    let bad = host.dispatch(
        "telegram.server.login",
        &json!({
            "endpoint": srv.base_url,
            "username": "desktop_alice",
            "password": "WrongPassword!",
        }),
    );
    assert!(bad.is_err());
    assert!(bad.unwrap_err().contains("HTTP 401"));

    // 3. Legacy V1 user logs in -> silently upgraded to V2 -> logs out -> logs back in with V2!
    let v1_login = host
        .dispatch(
            "telegram.server.login",
            &json!({
                "endpoint": srv.base_url,
                "username": "legacy_charlie",
                "password": "OldV1Password!",
                "kdfParams": { "m_cost_kib": 4096, "t_cost": 1, "p_cost": 1 },
            }),
        )
        .expect("legacy v1 login");
    assert_eq!(v1_login["user_id"], 502);

    host.dispatch("telegram.server.logout", &json!({}))
        .unwrap();

    let v2_relogin = host
        .dispatch(
            "telegram.server.login",
            &json!({
                "endpoint": srv.base_url,
                "username": "legacy_charlie",
                "password": "OldV1Password!",
            }),
        )
        .expect("post-upgrade v2 relogin");
    assert_eq!(v2_relogin["user_id"], 502);
    assert_eq!(v2_relogin["auth_version"], 2);

    let _ = std::fs::remove_dir_all(dir);
}

/// AC-3 + R5: 2FA challenge/verify flow & addContact forwarding `is_bot: true` + `first_name`.
#[test]
fn two_factor_login_and_bot_add_contact_contract() {
    let kdf = Argon2KdfParamsDto {
        m_cost_kib: 4_096,
        t_cost: 1,
        p_cost: 1,
    };
    let salt_b64 = B64.encode([0x99u8; 16]);
    let (verifier, _) = derive_split_key_v2_b64("TwoFactorPw!", &salt_b64, kdf).unwrap();
    let tfa_user = StoredUser {
        user_id: 777,
        username: "secure_dana".into(),
        email: None,
        first_name: "Dana".into(),
        auth_version: 2,
        raw_password: None,
        auth_salt_b64: Some(salt_b64),
        auth_verifier_b64: Some(verifier),
        kdf_params: kdf,
        two_factor_password: Some("cloud-2fa-secret".into()),
    };

    let srv = MockSplitKeyServer::start(vec![tfa_user]);
    let dir = temp_dir("tfa_and_bot");
    let host = MahayanaAndroidHost::new(&dir, false).unwrap();

    // 1. Login returns requires_2fa=true cleanly without error
    let chal = host
        .dispatch(
            "telegram.server.login",
            &json!({
                "endpoint": srv.base_url,
                "username": "secure_dana",
                "password": "TwoFactorPw!",
            }),
        )
        .expect("2fa challenge");
    assert_eq!(chal["requires_2fa"], true);
    assert_eq!(chal["hint"], "my-pet-name");
    let chal_token = chal["challenge_token"].as_str().unwrap().to_string();

    // Session is NOT persisted yet
    let sess_before = host
        .dispatch("telegram.server.session", &json!({}))
        .unwrap();
    assert_eq!(sess_before["authenticated"], false);

    // 2. verifyTwoFactor completes authentication and persists session
    let verified = host
        .dispatch(
            "telegram.server.verifyTwoFactor",
            &json!({
                "endpoint": srv.base_url,
                "challengeToken": chal_token,
                "cloudPassword": "cloud-2fa-secret",
            }),
        )
        .expect("verifyTwoFactor");
    assert_eq!(verified["user_id"], 777);

    let sess_after = host
        .dispatch("telegram.server.session", &json!({}))
        .unwrap();
    assert_eq!(sess_after["authenticated"], true);

    // 3. addContact forwards `is_bot: true` and `first_name`
    let added = host
        .dispatch(
            "telegram.server.addContact",
            &json!({
                "username": "@BotFather",
                "isBot": true,
            }),
        )
        .expect("addContact bot");
    assert_eq!(added["chat_id"], 9001);
    assert_eq!(added["received_is_bot"], true);
    assert_eq!(added["received_username"], "BotFather");
    assert_eq!(added["received_first_name"], "BotFather");

    let _ = std::fs::remove_dir_all(dir);
}
