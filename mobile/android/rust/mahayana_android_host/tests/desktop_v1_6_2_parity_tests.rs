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
    assert_eq!(info["desktopParityVersion"], "1.6.2");
    assert_eq!(info["desktopParityCommit"], "9542524");
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
    let (host, dir) = new_test_host("uu_remote");

    // 1. 19 Action Bus categories (v1.5.0)
    let cats = host.dispatch("remote.actionBusCategories", &json!({})).unwrap();
    assert_eq!(cats["total"], 19);

    // 2. Permanent 9-digit Remote Assist ID + code enforcement
    let cfg = host.dispatch("remote.getAssistConfig", &json!({})).unwrap();
    let assist_id = cfg["assist_id"].as_str().unwrap().to_string();
    assert_eq!(assist_id.len(), 9);
    host.dispatch(
        "remote.setAssistCode",
        &json!({ "code": "Fabu2026", "codeMode": "custom" }),
    )
    .unwrap();

    // Wrong code fails closed
    assert!(host
        .dispatch(
            "remote.startAssist",
            &json!({ "assistId": assist_id, "accessCode": "WRONG99" })
        )
        .is_err());

    // Correct ID + code starts active session
    let started = host
        .dispatch(
            "remote.startAssist",
            &json!({
                "assistId": assist_id,
                "accessCode": "Fabu2026",
                "requesterName": "Mac Studio Desktop",
                "mode": "desktop"
            }),
        )
        .unwrap();
    let sid = started["sessionId"].as_str().unwrap().to_string();
    let list1 = host.dispatch("remote.listSessions", &json!({})).unwrap();
    assert_eq!(list1["sessions"].as_array().unwrap().len(), 1);
    host.dispatch("remote.disconnectSession", &json!({ "sessionId": sid }))
        .unwrap();
    let list2 = host.dispatch("remote.listSessions", &json!({})).unwrap();
    assert_eq!(list2["sessions"].as_array().unwrap().len(), 0);

    // 3. Stream settings persistence
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

    // 4. Extended structured RemoteComputerOp with generation fencing
    let lease = host
        .dispatch(
            "computer.leaseAcquire",
            &json!({ "nodeId": "local-android-mika", "holder": "desktop_remote" }),
        )
        .unwrap();
    let gen = lease["generation"].as_i64().unwrap();
    let clip_res = host
        .dispatch(
            "computer.executeOp",
            &json!({
                "nodeId": "local-android-mika",
                "generation": gen,
                "op": { "op": "clipboard_get" }
            }),
        )
        .unwrap();
    assert_eq!(clip_res["executed"], true);
    assert_eq!(clip_res["readOnly"], true);

    let wol_res = host
        .dispatch(
            "computer.executeOp",
            &json!({
                "nodeId": "local-android-mika",
                "generation": gen,
                "op": {
                    "op": "power_action",
                    "action": { "kind": "wake_on_lan", "mac_address": "AA:BB:CC:DD:EE:FF" }
                }
            }),
        )
        .unwrap();
    assert_eq!(wol_res["executed"], true);
    assert_eq!(wol_res["readOnly"], false);

    // 5. Official OAuth 2.1 Remote MCP Grants lifecycle
    let grant = host
        .dispatch(
            "remote.issueMcpGrant",
            &json!({
                "clientName": "Claude Desktop MCP",
                "scopes": ["remote.read", "remote.terminal", "mcp.tools"]
            }),
        )
        .unwrap();
    let family_id = grant["family_id"].as_str().unwrap().to_string();
    let grants = host.dispatch("remote.listMcpGrants", &json!({})).unwrap();
    assert_eq!(grants["grants"].as_array().unwrap().len(), 1);
    host.dispatch("remote.revokeMcpGrant", &json!({ "familyId": family_id }))
        .unwrap();
    let grants_after = host.dispatch("remote.listMcpGrants", &json!({})).unwrap();
    assert_eq!(grants_after["grants"].as_array().unwrap().len(), 0);

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
