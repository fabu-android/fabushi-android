use mahayana_app_host::MahayanaAndroidHost;
use serde_json::json;
use std::path::PathBuf;

fn new_test_host(tag: &str) -> (MahayanaAndroidHost, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "fabushi_v162_parity_{}_{}",
        tag,
        uuid::Uuid::new_v4().simple()
    ));
    let host = MahayanaAndroidHost::new(&dir, true).expect("create test host");
    (host, dir)
}

#[test]
fn test_feature_info_reports_desktop_v162_parity() {
    let (host, dir) = new_test_host("feature_info");
    let info = host.dispatch("feature.info", &json!({})).unwrap();
    assert_eq!(info["desktopParityVersion"], "1.6.6");
    assert_eq!(info["desktopParityCommit"], "6ca754c");
    assert_eq!(info["platform"], "android");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_user_vault_split_key_v2_per_user_isolation_and_x25519_atomic_wipe() {
    let (host, dir) = new_test_host("user_vault");

    // 1. Split-Key V2 Argon2id verifier derivation (deterministic for same salt)
    let kdf1 = host
        .dispatch(
            "userVault.deriveSplitKeyVerifier",
            &json!({ "username": "alice", "password": "correct-horse-battery-staple" }),
        )
        .unwrap();
    assert_eq!(kdf1["authVersion"], 2);
    assert_eq!(kdf1["kdfAlg"], "split_key_v2");
    let salt_b64 = kdf1["authSaltB64"].as_str().unwrap();
    let kdf2 = host
        .dispatch(
            "userVault.deriveSplitKeyVerifier",
            &json!({ "username": "alice", "password": "correct-horse-battery-staple", "authSaltB64": salt_b64 }),
        )
        .unwrap();
    assert_eq!(kdf1["authVerifierB64"], kdf2["authVerifierB64"]);
    assert_ne!(kdf1["authVerifierB64"], kdf1["vaultKekB64"]);

    // 2. Init envelope + per-user ChaCha20-Poly1305 encrypted items isolation
    let env_a = host
        .dispatch(
            "userVault.initEnvelope",
            &json!({ "userId": "user_alice", "password": "alice-secret-pass" }),
        )
        .unwrap();
    let root_a = env_a["rootKeyB64"].as_str().unwrap().to_string();

    host.dispatch(
        "userVault.upsertItem",
        &json!({
            "userId": "user_alice",
            "category": "ai_oauth",
            "itemKey": "openai_codex",
            "rootKeyB64": root_a,
            "plaintext": "{\"access_token\":\"sk-alice-secret-123\"}"
        }),
    )
    .unwrap();

    // User Alice decrypts her own item
    let pull_a = host
        .dispatch(
            "userVault.syncPull",
            &json!({ "userId": "user_alice", "category": "ai_oauth", "rootKeyB64": root_a }),
        )
        .unwrap();
    assert_eq!(pull_a["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        pull_a["items"][0]["plaintext"],
        "{\"access_token\":\"sk-alice-secret-123\"}"
    );

    // User Bob sees ZERO items from Alice (strict per-user isolation)
    let pull_b = host
        .dispatch(
            "userVault.syncPull",
            &json!({ "userId": "user_bob", "category": "ai_oauth" }),
        )
        .unwrap();
    assert_eq!(pull_b["items"].as_array().unwrap().len(), 0);

    // 3. X25519 Device Trust Ring sync + atomic one-time wipe on first approved poll
    let sync_req = host
        .dispatch(
            "userVault.requestDeviceSync",
            &json!({ "userId": "user_alice", "deviceName": "Pixel 9 Pro" }),
        )
        .unwrap();
    let req_id = sync_req["requestId"].as_str().unwrap();
    let dev_priv = sync_req["devicePrivateKeyB64"].as_str().unwrap();
    assert_eq!(sync_req["verificationCode"].as_str().unwrap().len(), 6);

    let pending = host
        .dispatch("userVault.listPendingDeviceSync", &json!({ "userId": "user_alice" }))
        .unwrap();
    assert_eq!(pending["items"].as_array().unwrap().len(), 1);

    host.dispatch(
        "userVault.approveDeviceSync",
        &json!({ "userId": "user_alice", "requestId": req_id, "rootKeyB64": root_a }),
    )
    .unwrap();

    // First poll returns unwrapped root key matching root_a
    let poll1 = host
        .dispatch(
            "userVault.pollDeviceSyncStatus",
            &json!({
                "userId": "user_alice",
                "requestId": req_id,
                "devicePrivateKeyB64": dev_priv
            }),
        )
        .unwrap();
    assert_eq!(poll1["status"], "approved");
    assert_eq!(poll1["decryptedRootKeyB64"], root_a);

    // Second poll MUST show status == "consumed" with wiped ciphertext!
    let poll2 = host
        .dispatch(
            "userVault.pollDeviceSyncStatus",
            &json!({ "userId": "user_alice", "requestId": req_id }),
        )
        .unwrap();
    assert_eq!(poll2["status"], "consumed");
    assert!(poll2["encryptedRootKeyB64"].is_null());
    assert!(poll2["approverEphemeralPubB64"].is_null());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_coding_agent_bots_crud_control_checkpoints_and_approvals() {
    let (host, dir) = new_test_host("coding_agent");

    let providers = host.dispatch("codingAgent.listProviders", &json!({})).unwrap();
    assert_eq!(providers["total"], 6);

    let created = host
        .dispatch(
            "codingAgent.createBot",
            &json!({
                "providerKind": "claude_code",
                "displayName": "Claude Code Architect",
                "username": "claude_arch",
                "workspacePath": "/srv/repo/fabushi",
                "useWorktree": true,
                "permissionMode": "ask",
                "boundNodeId": "mac-studio-01"
            }),
        )
        .unwrap();
    let bot_id = created["botId"].as_str().unwrap().to_string();
    assert_eq!(created["username"], "claude_arch_bot");
    assert_eq!(created["config"]["provider_kind"], "claude_code");
    assert_eq!(created["config"]["permission_mode"], "ask");

    // Dispatch CodingControlAction::SetPermissionMode & SetModel
    let ctrl = host
        .dispatch(
            "codingAgent.dispatchControl",
            &json!({
                "botId": bot_id,
                "control": { "action": "set_permission_mode", "mode": "auto_edit" }
            }),
        )
        .unwrap();
    assert_eq!(ctrl["config"]["permission_mode"], "auto_edit");

    // Record 2 coding turns with git diff + token usage, then rollback to turn 1
    let t1 = host
        .dispatch(
            "codingAgent.recordTurn",
            &json!({
                "botId": bot_id,
                "summary": "Implement Split-Key V2 verifier",
                "diffStat": { "files_changed": 2, "insertions": 140, "deletions": 8, "changed_files": ["user_vault.rs"] },
                "usage": { "input_tokens": 1200, "output_tokens": 450, "cached_tokens": 200, "cost_usd": 0.014 }
            }),
        )
        .unwrap();
    assert_eq!(t1["turnIdx"], 1);
    let t2 = host
        .dispatch(
            "codingAgent.recordTurn",
            &json!({ "botId": bot_id, "summary": "Experimental refactor" }),
        )
        .unwrap();
    assert_eq!(t2["turnIdx"], 2);

    host.dispatch(
        "codingAgent.dispatchControl",
        &json!({
            "botId": bot_id,
            "control": { "action": "rollback_checkpoint", "turn_idx": 1 }
        }),
    )
    .unwrap();
    let cps = host
        .dispatch("codingAgent.listCheckpoints", &json!({ "botId": bot_id }))
        .unwrap();
    assert_eq!(cps["checkpoints"].as_array().unwrap().len(), 1);

    // Interactive coding tool approval decision
    let appr = host
        .dispatch(
            "codingAgent.decideApproval",
            &json!({
                "botId": bot_id,
                "toolName": "bash",
                "commandPreview": "cargo test --workspace",
                "decision": "allow_session"
            }),
        )
        .unwrap();
    assert_eq!(appr["decision"], "allow_session");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_uu_remote_assist_stream_settings_extended_ops_and_mcp_oauth_grants() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    let (host, dir) = new_test_host("uu_remote");

    // 1. 19 Action Bus categories (v1.5.0)
    let cats = host.dispatch("remote.actionBusCategories", &json!({})).unwrap();
    assert_eq!(cats["total"], 19);

    // 2. Unauthenticated remote.* assist/sessions/MCP grants MUST fail closed (R-F4, R-F5)
    assert!(host
        .dispatch("remote.getAssistConfig", &json!({}))
        .unwrap_err()
        .contains("REMOTE_ASSIST_UNAVAILABLE"));
    assert!(host
        .dispatch(
            "remote.startAssist",
            &json!({ "assistId": "123456789", "accessCode": "Fabu2026" })
        )
        .unwrap_err()
        .contains("REMOTE_ASSIST_UNAVAILABLE"));
    assert!(host
        .dispatch("remote.listMcpGrants", &json!({}))
        .unwrap_err()
        .contains("AUTH_REQUIRED"));
    assert!(host
        .dispatch(
            "remote.issueMcpGrant",
            &json!({ "clientName": "Claude Desktop MCP" })
        )
        .unwrap_err()
        .contains("OAUTH_FLOW_REQUIRED"));

    // 3. Stream settings local persistence
    let updated_stream = host
        .dispatch(
            "remote.updateStreamSettings",
            &json!({
                "codec": "h265",
                "target_fps": 120,
                "quality": "ultra4_k",
                "color_mode": "true_color444",
                "hdr": true,
                "audio_enabled": true,
                "mute_remote_speaker": true,
                "mic_passthrough": true,
                "privacy_screen": true,
                "relative_mouse": true,
                "lock_cursor_in_window": false,
                "macos_hotkey_mapping": true,
                "display_index": 1
            }),
        )
        .unwrap();
    assert_eq!(updated_stream["codec"], "h265");
    assert_eq!(updated_stream["target_fps"], 120);
    assert_eq!(updated_stream["privacy_screen"], true);

    // 4. Local computer.executeOp MUST fail closed (R-F3) while preserving generation fencing
    let lease = host
        .dispatch(
            "computer.leaseAcquire",
            &json!({ "nodeId": "local-android-mika", "holder": "desktop_remote" }),
        )
        .unwrap();
    let gen = lease["generation"].as_i64().unwrap();
    let local_err = host
        .dispatch(
            "computer.executeOp",
            &json!({
                "nodeId": "local-android-mika",
                "generation": gen,
                "op": { "op": "clipboard_get" }
            }),
        )
        .unwrap_err();
    assert!(local_err.contains("LOCAL_EXECUTOR_REQUIRED"));

    // 5. Real loopback server backing assist, sessions, extended RemoteComputerOp, and MCP grants
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().unwrap();
    let server_url = format!("http://{addr}");
    let srv = thread::spawn(move || {
        for _ in 0..10 {
            let (mut stream, _) = match listener.accept() {
                Ok(s) => s,
                Err(_) => break,
            };
            let mut raw_buf = Vec::with_capacity(4096);
            let mut chunk = [0u8; 4096];
            let mut header_end = None;
            let mut content_len = 0usize;
            loop {
                let n = stream.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break;
                }
                raw_buf.extend_from_slice(&chunk[..n]);
                if header_end.is_none() {
                    if let Some(pos) = raw_buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let hend = pos + 4;
                        let head = String::from_utf8_lossy(&raw_buf[..pos]);
                        for line in head.lines() {
                            if let Some((k, v)) = line.split_once(':') {
                                if k.trim().eq_ignore_ascii_case("content-length") {
                                    content_len = v.trim().parse().unwrap_or(0);
                                }
                            }
                        }
                        header_end = Some(hend);
                    }
                }
                if let Some(hend) = header_end {
                    if raw_buf.len() >= hend + content_len {
                        break;
                    }
                }
            }
            let req = String::from_utf8_lossy(&raw_buf);
            let first = req.lines().next().unwrap_or("");
            let (status, body) = if first.starts_with("POST /api/v1/auth/login") {
                ("200 OK", r#"{"user_id":42,"username":"ops","session_id":"s_ops","token":"tok_remote_12345678901234567890"}"#)
            } else if first.starts_with("GET /api/v1/remote/assist/info") {
                ("200 OK", r#"{"assist_id":"987654321","allow_assist":true,"code_mode":"temporary","current_code":"AbCd1234"}"#)
            } else if first.starts_with("POST /api/v1/remote/assist/set-code") {
                ("200 OK", r#"{"assist_id":"987654321","allow_assist":true,"code_mode":"custom","current_code":"Fabu2026"}"#)
            } else if first.starts_with("POST /api/v1/remote/assist/start") {
                if req.contains("WRONG99") {
                    ("403 Forbidden", r#"{"error":"invalid_code","message":"Invalid verification code"}"#)
                } else {
                    ("200 OK", r#"{"session_id":"rsess_01","target_node_id":"cloud:42:prod","target_device_id":"987654321","temp_relay_token":"fbrelay_tok_01","requires_host_confirm":true}"#)
                }
            } else if first.starts_with("GET /api/v1/remote/sessions") {
                ("200 OK", r#"{"sessions":[{"session_id":"rsess_01","controller_kind":"assist_guest","controller_name":"Mac Studio Desktop"}]}"#)
            } else if first.starts_with("POST /api/v1/remote/sessions/disconnect") {
                ("200 OK", r#"{"disconnected":true,"session_id":"rsess_01"}"#)
            } else if first.starts_with("POST /api/v1/computers/exec") {
                ("200 OK", r#"{"command_id":"cmd_1","node_id":"cloud:42:prod","ok":true,"exit_code":0,"stdout":"copied","stderr":"","entries":[],"executed_by":"daemon","duration_ms":12}"#)
            } else if first.starts_with("GET /api/v1/mcp/grants") {
                ("200 OK", r#"[{"family_id":"fam_srv_01","client_id":"claude_desktop","client_name":"Claude Desktop MCP","scopes":["remote.read"],"allowed_node_ids":["cloud:42:prod"],"created_at_ms":1760000000000,"expires_at_ms":1769999999999}]"#)
            } else if first.starts_with("DELETE /api/v1/mcp/grants/fam_srv_01") {
                ("200 OK", r#"{"revoked":true,"family_id":"fam_srv_01"}"#)
            } else {
                ("404 Not Found", r#"{"error":"not_found"}"#)
            };
            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });

    host.dispatch(
        "telegram.server.login",
        &json!({ "endpoint": server_url, "username": "ops", "password": "secret" }),
    )
    .unwrap();

    let cfg = host.dispatch("remote.getAssistConfig", &json!({})).unwrap();
    assert_eq!(cfg["assist_id"], "987654321");

    host.dispatch("remote.setAssistCode", &json!({ "code": "Fabu2026" }))
        .unwrap();
    assert!(host
        .dispatch(
            "remote.startAssist",
            &json!({ "assistId": "987654321", "accessCode": "WRONG99" })
        )
        .is_err());
    let started = host
        .dispatch(
            "remote.startAssist",
            &json!({
                "assistId": "987654321",
                "accessCode": "Fabu2026",
                "requesterName": "Mac Studio Desktop",
                "mode": "desktop"
            }),
        )
        .unwrap();
    assert_eq!(started["session_id"], "rsess_01");

    let list1 = host.dispatch("remote.listSessions", &json!({})).unwrap();
    assert_eq!(list1["sessions"].as_array().unwrap().len(), 1);
    host.dispatch("remote.disconnectSession", &json!({ "sessionId": "rsess_01" }))
        .unwrap();

    // Extended structured RemoteComputerOp via real telegram.server.execComputer
    let wol_res = host
        .dispatch(
            "telegram.server.execComputer",
            &json!({
                "nodeId": "cloud:42:prod",
                "operation": {
                    "op": "power_action",
                    "action": { "kind": "wake_on_lan", "mac_address": "AA:BB:CC:DD:EE:FF" }
                }
            }),
        )
        .unwrap();
    assert_eq!(wol_res["ok"], true);

    // Real server-backed OAuth 2.1 MCP grants list & revoke
    let grants = host.dispatch("remote.listMcpGrants", &json!({})).unwrap();
    assert_eq!(grants["grants"].as_array().unwrap().len(), 1);
    let revoked = host
        .dispatch("remote.revokeMcpGrant", &json!({ "familyId": "fam_srv_01" }))
        .unwrap();
    assert_eq!(revoked["revoked"], true);

    let _ = srv.join();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_magpie_models_dev_vertex_sanitization_gemini_parallel_merge_and_webmcp() {
    let (host, dir) = new_test_host("magpie_webmcp");

    // 1. models.dev JSON parsing & utility model filtering
    let catalog_json = json!({
        "google-vertex": {
            "id": "google-vertex",
            "models": {
                "gemini-2.5-pro": { "id": "gemini-2.5-pro", "release_date": "2025-05-01" },
                "gemini-3.8-flash": { "id": "gemini-3.8-flash", "release_date": "2026-09-01" },
                "text-embedding-005": { "id": "text-embedding-005", "release_date": "2026-01-01" }
            }
        }
    })
    .to_string();
    let parsed = host
        .dispatch("magpie.parseModelsDevCatalog", &json!({ "json": catalog_json }))
        .unwrap();
    assert_eq!(parsed["providerCount"], 1);
    assert_eq!(
        parsed["providers"]["google-vertex"],
        json!(["gemini-3.8-flash", "gemini-2.5-pro"])
    );

    // 2. Vertex AI schema sanitization
    let sanitized = host
        .dispatch(
            "magpie.sanitizeVertexSchema",
            &json!({
                "schema": {
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "query": { "type": ["string", "null"], "title": "Query", "default": "" }
                    }
                }
            }),
        )
        .unwrap();
    assert!(sanitized["sanitized"].get("$schema").is_none());
    assert!(sanitized["sanitized"].get("additionalProperties").is_none());
    assert_eq!(sanitized["sanitized"]["properties"]["query"]["type"], "string");
    assert_eq!(sanitized["sanitized"]["properties"]["query"]["nullable"], true);

    // 3. Gemini parallel functionResponses merge + thoughtSignature preservation
    let merged = host
        .dispatch(
            "magpie.mergeGeminiFunctionResponses",
            &json!({
                "request": {
                    "model": "gemini-2.5-pro",
                    "system_prompt": "",
                    "messages": [
                        { "role": "user", "content": "Run two tools" },
                        {
                            "role": "assistant",
                            "content": "",
                            "tool_calls": [
                                { "id": "c1", "name": "get_weather", "arguments": "{\"city\":\"SF\"}", "thought_signature": "sig_alpha_123" },
                                { "id": "c2", "name": "get_time", "arguments": "{\"tz\":\"UTC\"}" }
                            ]
                        },
                        { "role": "tool", "name": "get_weather", "tool_call_id": "c1", "content": "{\"temp\":18}" },
                        { "role": "tool", "name": "get_time", "tool_call_id": "c2", "content": "{\"utc\":\"12:00\"}" }
                    ]
                }
            }),
        )
        .unwrap();
    let contents = merged["payload"]["contents"].as_array().unwrap();
    // User + Model(2 parallel functionCalls with thoughtSignature) + Single merged User turn(2 functionResponses)
    assert_eq!(contents.len(), 3);
    assert_eq!(contents[1]["parts"][0]["thoughtSignature"], "sig_alpha_123");
    assert_eq!(contents[2]["role"], "user");
    assert_eq!(contents[2]["parts"].as_array().unwrap().len(), 2);

    // 4. Generic Mini App WebMCP control + PiP minimized state
    let reg = host
        .dispatch(
            "miniapp.webmcp.registerTools",
            &json!({
                "botUsername": "fabushi_bot",
                "shortName": "global_dharma",
                "appTitle": "全球法布施",
                "tools": [
                    { "name": "start_dharma_offering", "description": "Start offering", "input_schema": {}, "read_only": false }
                ]
            }),
        )
        .unwrap();
    assert_eq!(reg["registeredTools"], 1);

    host.dispatch(
        "miniapp.webmcp.setControlMode",
        &json!({
            "botUsername": "fabushi_bot",
            "shortName": "global_dharma",
            "mode": "ai_controlling",
            "statusText": "正在向全球 240 个国家/地区同步法布施光束",
            "activeTool": "start_dharma_offering",
            "pipMinimized": true
        }),
    )
    .unwrap();
    let st = host
        .dispatch(
            "miniapp.webmcp.getControlStatus",
            &json!({ "botUsername": "fabushi_bot", "shortName": "global_dharma" }),
        )
        .unwrap();
    assert_eq!(st["controlMode"], "ai_controlling");
    assert_eq!(st["pipMinimized"], true);
    assert_eq!(st["activeTool"], "start_dharma_offering");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_real_gpui_telegram_server_api_v1_client_loopback() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().unwrap();
    let server_url = format!("http://{addr}");

    let handle = thread::spawn(move || {
        for _ in 0..5 {
            let (mut stream, _) = match listener.accept() {
                Ok(s) => s,
                Err(_) => break,
            };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req_str = String::from_utf8_lossy(&buf[..n]);
            let first_line = req_str.lines().next().unwrap_or("");
            let body = if first_line.starts_with("POST /api/v1/auth/login") {
                r#"{"user_id":1001,"username":"alice","session_id":"sess_1","token":"tok_live_123","first_name":"Alice"}"#
            } else if first_line.starts_with("GET /api/v1/auth/me") {
                r#"{"user_id":1001,"username":"alice","first_name":"Alice","session_id":"sess_1"}"#
            } else if first_line.starts_with("GET /api/v1/chats/42/messages") {
                r#"[{"id":901,"chat_id":42,"sender_id":1002,"sender_name":"Bob","text":"Real message from server","created_at_ms":1763800000000}]"#
            } else if first_line.starts_with("GET /api/v1/chats") {
                r#"[{"id":42,"title":"Bob","kind":"direct","last_message_text":"Real message from server","unread_count":1,"is_pinned":true}]"#
            } else if first_line.starts_with("POST /api/v1/messages/send") {
                r#"{"message_id":902,"pts":55,"ok":true}"#
            } else {
                r#"{"ok":true}"#
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });

    let (host, dir) = new_test_host("real_server");

    let sess0 = host.dispatch("telegram.server.session", &json!({})).unwrap();
    assert_eq!(sess0["authenticated"], false);

    let login = host
        .dispatch(
            "telegram.server.login",
            &json!({ "endpoint": server_url, "username": "alice", "password": "secret" }),
        )
        .unwrap();
    assert_eq!(login["token"], "tok_live_123");

    let sess1 = host.dispatch("telegram.server.session", &json!({})).unwrap();
    assert_eq!(sess1["authenticated"], true);
    assert_eq!(sess1["username"], "alice");

    let me = host.dispatch("telegram.server.me", &json!({})).unwrap();
    assert_eq!(me["user_id"], 1001);

    let chats = host.dispatch("telegram.server.listChats", &json!({})).unwrap();
    assert_eq!(chats["chats"][0]["id"], 42);
    assert_eq!(chats["chats"][0]["title"], "Bob");

    let msgs = host
        .dispatch("telegram.server.getMessages", &json!({ "chatId": 42 }))
        .unwrap();
    assert_eq!(msgs["messages"][0]["text"], "Real message from server");

    let sent = host
        .dispatch(
            "telegram.server.sendMessage",
            &json!({ "chatId": 42, "text": "Hello real server" }),
        )
        .unwrap();
    assert_eq!(sent["message_id"], 902);

    let logout = host.dispatch("telegram.server.logout", &json!({})).unwrap();
    assert_eq!(logout["loggedOut"], true);
    let sess2 = host.dispatch("telegram.server.session", &json!({})).unwrap();
    assert_eq!(sess2["authenticated"], false);

    let _ = handle.join();
    let _ = std::fs::remove_dir_all(dir);
}
