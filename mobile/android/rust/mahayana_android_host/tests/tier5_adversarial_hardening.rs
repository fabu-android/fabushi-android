//! Tier 5 Adversarial Hardening Integration Tests (`p2_worker_rust` — Milestone M5).
//!
//! Covers every Rust security, concurrency, replay, bounds, and arithmetic hardening fix:
//! - H1: `waveform.rs` `MAX_WAVEFORM_BARS` clamp on `u64::MAX` & `VadBargeInDetector::feed_pcm` saturating_mul
//! - H2: `fabushi_miniapp.rs` `durationSeconds` / `chantCount` = `u64::MAX` clamp & saturating arithmetic
//! - H4: `telegram_ext.rs` stale-token 401 session wipe race guard + strict `HTTP 401:` prefix match
//! - H5: `telegram_ext.rs` logout wipes `tg_server_outbox` and resets `sync_pts = 1`
//! - H6: `hermes.rs` `approvalsDecide` & `computer.rs` `actionBus.approve` atomic pending guard (no replay / double-exec)
//! - M1: `hermes.rs` `skillUpsert` and `cronUpsert` honor `enabled: false`
//! - M2: `hermes.rs` `parse_cron_field` rejects huge step `1-5/18446744073709551615` without overflow panic
//! - M3: `messaging.rs` rejects `.`/`..` blob traversal & caps sparse-file offset/finish size at 64 MiB
//! - M4: `magpie.rs` `is_local_loopback_url` host delimiter check + bounded non-SSE response cap & agent timeouts
//! - LOW/NIT: `telegram_ext.rs` `.`/`..` path segment encoding (`%2E`, `%2E%2E`) & `wallet.rs` `utime` saturating_mul

use mahayana_app_host::magpie::is_local_loopback_url;
use mahayana_app_host::telegram_ext::url_encode_path_segment;
use mahayana_app_host::waveform::{
    decode_waveform_5bit, extract_bars_from_pcm_i16, resample_bars_to_count, VadBargeInDetector,
    MAX_WAVEFORM_BARS,
};
use mahayana_app_host::MahayanaAndroidHost;
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "fabushi_tier5_adv_{}_{}",
        tag,
        uuid::Uuid::new_v4().simple()
    ))
}

fn new_prod_host(tag: &str) -> (Arc<MahayanaAndroidHost>, PathBuf) {
    let dir = temp_dir(tag);
    let host = Arc::new(MahayanaAndroidHost::new(&dir, false).expect("create prod host"));
    (host, dir)
}

fn dispatch_ok(host: &MahayanaAndroidHost, method: &str, params: Value) -> Value {
    let raw = host.dispatch_json(&json!({ "method": method, "params": params }).to_string());
    let parsed: Value = serde_json::from_str(&raw).expect("valid JSON envelope");
    assert_eq!(
        parsed.get("ok").and_then(Value::as_bool),
        Some(true),
        "Expected ok=true for {method}, got: {parsed}"
    );
    parsed.get("result").cloned().unwrap_or(Value::Null)
}

fn dispatch_err(host: &MahayanaAndroidHost, method: &str, params: Value) -> String {
    let raw = host.dispatch_json(&json!({ "method": method, "params": params }).to_string());
    let parsed: Value = serde_json::from_str(&raw).expect("valid JSON envelope");
    assert_eq!(
        parsed.get("ok").and_then(Value::as_bool),
        Some(false),
        "Expected ok=false for {method}, got: {parsed}"
    );
    parsed
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

#[derive(Debug, Clone)]
struct HttpReq {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl HttpReq {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn read_http_request(stream: &mut TcpStream) -> Option<HttpReq> {
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

    let head_str = String::from_utf8_lossy(&buf[..header_end - 4]);
    let mut lines = head_str.lines();
    let req_line = lines.next().unwrap_or("");
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let path = parts.next().unwrap_or("/").to_string();

    let mut headers = Vec::new();
    let mut content_len = 0usize;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_string();
            let val = v.trim().to_string();
            if key.eq_ignore_ascii_case("content-length") {
                content_len = val.parse().unwrap_or(0);
            }
            headers.push((key, val));
        }
    }

    while buf.len() < header_end + content_len {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body_end = (header_end + content_len).min(buf.len());
    let body = String::from_utf8_lossy(&buf[header_end..body_end]).to_string();

    Some(HttpReq {
        method,
        path,
        headers,
        body,
    })
}

fn write_http(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

struct MockServer {
    base_url: String,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockServer {
    fn start<F>(handler: F) -> Self
    where
        F: Fn(HttpReq, &mut TcpStream) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback port");
        let addr = listener.local_addr().expect("local addr");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let handler = Arc::new(handler);

        let handle = thread::spawn(move || {
            while !stop_flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let h = Arc::clone(&handler);
                        thread::spawn(move || {
                            if let Some(req) = read_http_request(&mut stream) {
                                h(req, &mut stream);
                            }
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            base_url: format!("http://{addr}"),
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

// ============================================================================
// H1: waveform.rs unclamped barCount OOM abort & VadBargeInDetector overflow
// ============================================================================

#[test]
fn h1_waveform_clamps_u64_max_bar_count_and_vad_saturates() {
    let (host, dir) = new_prod_host("h1_waveform");

    let res_max = dispatch_ok(
        &host,
        "telegram.messaging.decodeWaveform",
        json!({
            "encodedBase64": "AAAA",
            "barCount": u64::MAX,
        }),
    );
    assert_eq!(
        res_max["barCount"].as_u64(),
        Some(MAX_WAVEFORM_BARS as u64),
        "barCount=u64::MAX must clamp to MAX_WAVEFORM_BARS ({MAX_WAVEFORM_BARS})"
    );
    assert_eq!(
        res_max["bars"].as_array().map(|a| a.len()),
        Some(MAX_WAVEFORM_BARS)
    );

    let res_million = dispatch_ok(
        &host,
        "telegram.messaging.decodeWaveform",
        json!({
            "encodedBase64": "AAAA",
            "barCount": 1_000_000u64,
        }),
    );
    assert_eq!(res_million["barCount"].as_u64(), Some(MAX_WAVEFORM_BARS as u64));

    // Direct codec bounds verification
    assert_eq!(decode_waveform_5bit(&[0xFF; 8], usize::MAX).len(), MAX_WAVEFORM_BARS);
    assert_eq!(resample_bars_to_count(&[1, 15, 31], usize::MAX).len(), MAX_WAVEFORM_BARS);
    assert_eq!(extract_bars_from_pcm_i16(&[0, 12000, -12000], usize::MAX).len(), MAX_WAVEFORM_BARS);

    // VadBargeInDetector saturating_mul on extreme consecutive_speech_frames
    let mut vad = VadBargeInDetector::default();
    vad.consecutive_speech_frames = usize::MAX;
    let report = vad.feed_pcm(&[], false);
    assert_eq!(report.detection_latency_ms, u32::MAX);

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// H2: fabushi_miniapp.rs durationSeconds / chantCount = u64::MAX clamp
// ============================================================================

#[test]
fn h2_fabushi_miniapp_clamps_extreme_duration_and_chant_count() {
    let (host, dir) = new_prod_host("h2_miniapp");

    let res = dispatch_ok(
        &host,
        "fabushiMiniapp.recordPractice",
        json!({
            "sutraId": "heart_sutra",
            "durationSeconds": u64::MAX,
            "chantCount": u64::MAX,
        }),
    );

    assert_eq!(res["durationSeconds"].as_u64(), Some(604_800));
    assert_eq!(res["chantCount"].as_u64(), Some(1_000_000));
    let total_merit = res["totalMeritPoints"]
        .as_i64()
        .expect("totalMeritPoints i64");
    assert!(
        total_merit > 0,
        "totalMeritPoints must remain positive without wrapping: {total_merit}"
    );
    assert_eq!(res["record"]["duration_minutes"].as_u64(), Some(10_080));

    let status = dispatch_ok(&host, "fabushiMiniapp.status", json!({}));
    assert!(
        status["dharmaState"]["totalPracticeSeconds"]
            .as_u64()
            .unwrap_or(0)
            >= 604_800
    );

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// H4: Stale-token 401 session wipe race & strict HTTP 401 prefix match
// ============================================================================

#[test]
fn h4_stale_token_401_does_not_clobber_fresh_login_session() {
    let (host, dir) = new_prod_host("h4_stale_401");
    let stale_in_flight = Arc::new(AtomicBool::new(false));
    let release_stale_401 = Arc::new(AtomicBool::new(false));

    let stale_in_flight_srv = Arc::clone(&stale_in_flight);
    let release_stale_srv = Arc::clone(&release_stale_401);

    let server = MockServer::start(move |req, stream| {
        if req.method == "POST" && req.path == "/api/v1/auth/login" {
            let body: Value = serde_json::from_str(&req.body).unwrap_or(Value::Null);
            let username = body["username"].as_str().unwrap_or("");
            if username == "alice" {
                let resp = json!({
                    "token": "tok_A_stale",
                    "user_id": 1001,
                    "user": { "user_id": 1001, "username": "alice", "first_name": "Alice" },
                    "session_id": "sess_A"
                })
                .to_string();
                write_http(stream, "200 OK", "application/json", resp.as_bytes());
            } else {
                let resp = json!({
                    "token": "tok_B_fresh",
                    "user_id": 2002,
                    "user": { "user_id": 2002, "username": "bob", "first_name": "Bob" },
                    "session_id": "sess_B"
                })
                .to_string();
                write_http(stream, "200 OK", "application/json", resp.as_bytes());
            }
            return;
        }

        if req.method == "GET" && (req.path == "/api/v1/chats" || req.path == "/api/v1/media/stale-file") {
            let auth = req.header("authorization").unwrap_or("");
            if auth == "Bearer tok_A_stale" {
                stale_in_flight_srv.store(true, Ordering::SeqCst);
                while !release_stale_srv.load(Ordering::SeqCst) {
                    thread::sleep(Duration::from_millis(5));
                }
                let err = json!({ "error": "UNAUTHORIZED", "message": "tok_A revoked" }).to_string();
                write_http(stream, "401 Unauthorized", "application/json", err.as_bytes());
                return;
            }
            let ok = json!({ "chats": [], "total": 0 }).to_string();
            write_http(stream, "200 OK", "application/json", ok.as_bytes());
            return;
        }

        write_http(stream, "404 Not Found", "application/json", b"{\"error\":\"not_found\"}");
    });

    dispatch_ok(
        &host,
        "telegram.server.configureEndpoint",
        json!({ "endpoint": server.base_url }),
    );
    dispatch_ok(
        &host,
        "telegram.server.login",
        json!({ "username": "alice", "password": "pw" }),
    );

    // Launch slow listChats carrying tok_A_stale
    let host_bg = Arc::clone(&host);
    let bg_handle = thread::spawn(move || {
        dispatch_err(&host_bg, "telegram.server.listChats", json!({}))
    });

    // Wait until tok_A_stale request is actively in-flight (DB mutex released!)
    let mut waited = 0;
    while !stale_in_flight.load(Ordering::SeqCst) && waited < 400 {
        thread::sleep(Duration::from_millis(5));
        waited += 1;
    }
    assert!(
        stale_in_flight.load(Ordering::SeqCst),
        "background listChats must reach mock server with DB mutex unlocked"
    );

    // Fresh login as Bob replaces tg_fence.auth_token with tok_B_fresh while Alice's call is in-flight
    dispatch_ok(
        &host,
        "telegram.server.login",
        json!({ "username": "bob", "password": "pw" }),
    );

    // Now let Alice's stale 401 finish
    release_stale_401.store(true, Ordering::SeqCst);
    let stale_err = bg_handle.join().expect("join bg listChats");
    assert!(
        stale_err.starts_with("HTTP 401:"),
        "expected HTTP 401 error, got: {stale_err}"
    );

    // Bob's fresh session MUST still be intact!
    let sess = dispatch_ok(&host, "telegram.server.session", json!({}));
    assert_eq!(sess["authenticated"].as_bool(), Some(true));
    assert_eq!(sess["token"].as_str(), Some("tok_B_fresh"));
    assert_eq!(sess["userId"].as_i64(), Some(2002));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn h4_http_502_containing_http_401_text_does_not_wipe_session() {
    let (host, dir) = new_prod_host("h4_502_no_wipe");

    let server = MockServer::start(move |req, stream| {
        if req.method == "POST" && req.path == "/api/v1/auth/login" {
            let resp = json!({
                "token": "tok_active_502",
                "user": { "id": 3003, "username": "carol", "first_name": "Carol" },
                "session_id": "sess_C"
            })
            .to_string();
            write_http(stream, "200 OK", "application/json", resp.as_bytes());
            return;
        }
        if req.method == "GET" && req.path == "/api/v1/chats" {
            let err = json!({
                "error": "BAD_GATEWAY",
                "message": "upstream bot bridge returned HTTP 401: invalid bot key"
            })
            .to_string();
            write_http(stream, "502 Bad Gateway", "application/json", err.as_bytes());
            return;
        }
        write_http(stream, "404 Not Found", "application/json", b"{\"error\":\"not_found\"}");
    });

    dispatch_ok(
        &host,
        "telegram.server.configureEndpoint",
        json!({ "endpoint": server.base_url }),
    );
    dispatch_ok(
        &host,
        "telegram.server.login",
        json!({ "username": "carol", "password": "pw" }),
    );

    let err = dispatch_err(&host, "telegram.server.listChats", json!({}));
    assert!(err.starts_with("HTTP 502:"), "expected HTTP 502, got: {err}");
    assert!(err.contains("HTTP 401"), "body should carry upstream mention: {err}");

    // Active session MUST NOT be wiped by a 502 body containing "HTTP 401"
    let sess = dispatch_ok(&host, "telegram.server.session", json!({}));
    assert_eq!(sess["authenticated"].as_bool(), Some(true));
    assert_eq!(sess["token"].as_str(), Some("tok_active_502"));

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// H5: Logout clears tg_server_outbox and resets sync_pts = 1
// ============================================================================

#[test]
fn h5_logout_clears_outbox_and_resets_sync_pts_across_accounts() {
    let (host, dir) = new_prod_host("h5_logout_outbox");
    let send_calls = Arc::new(AtomicUsize::new(0));
    let send_calls_srv = Arc::clone(&send_calls);

    let server = MockServer::start(move |req, stream| {
        if req.method == "POST" && req.path == "/api/v1/auth/login" {
            let body: Value = serde_json::from_str(&req.body).unwrap_or(Value::Null);
            let username = body["username"].as_str().unwrap_or("u");
            let resp = json!({
                "token": format!("tok_{username}"),
                "user": { "id": if username == "user_a" { 10 } else { 20 }, "username": username, "first_name": username },
                "session_id": format!("sess_{username}")
            })
            .to_string();
            write_http(stream, "200 OK", "application/json", resp.as_bytes());
            return;
        }
        if req.method == "POST" && req.path == "/api/v1/sync/difference" {
            let resp = json!({
                "new_pts": 500,
                "current_pts": 500,
                "updates": [],
                "messages": [],
                "has_more": false
            })
            .to_string();
            write_http(stream, "200 OK", "application/json", resp.as_bytes());
            return;
        }
        if req.method == "POST" && req.path == "/api/v1/messages/send" {
            send_calls_srv.fetch_add(1, Ordering::SeqCst);
            let resp = json!({ "message_id": 999, "status": "sent" }).to_string();
            write_http(stream, "200 OK", "application/json", resp.as_bytes());
            return;
        }
        write_http(stream, "200 OK", "application/json", b"{\"ok\":true}");
    });

    dispatch_ok(
        &host,
        "telegram.server.configureEndpoint",
        json!({ "endpoint": server.base_url }),
    );
    dispatch_ok(
        &host,
        "telegram.server.login",
        json!({ "username": "user_a", "password": "pw" }),
    );

    // Advance sync_pts to 500 and enqueue a private outbox message for User A
    dispatch_ok(&host, "telegram.server.syncDifference", json!({ "liveNetwork": true }));
    dispatch_ok(
        &host,
        "telegram.server.enqueueOutbox",
        json!({ "payload": { "chat_id": 77, "text": "User A secret draft" } }),
    );

    let st_before = dispatch_ok(&host, "telegram.server.status", json!({}));
    assert_eq!(st_before["pts"].as_i64(), Some(500));
    assert_eq!(st_before["pendingOutboxCount"].as_i64(), Some(1));

    // Logout must wipe outbox and reset sync_pts = 1
    dispatch_ok(&host, "telegram.server.logout", json!({}));

    let st_after = dispatch_ok(&host, "telegram.server.status", json!({}));
    assert_eq!(st_after["pts"].as_i64(), Some(1));
    assert_eq!(st_after["pendingOutboxCount"].as_i64(), Some(0));

    // Login as User B and flushOutbox -> zero messages leaked to User B's bearer token
    dispatch_ok(
        &host,
        "telegram.server.login",
        json!({ "username": "user_b", "password": "pw" }),
    );
    let flushed = dispatch_ok(&host, "telegram.server.flushOutbox", json!({}));
    assert_eq!(flushed["flushedCount"].as_u64(), Some(0));
    assert_eq!(flushed["pendingCount"].as_i64(), Some(0));
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        0,
        "User A's outbox message must NEVER be sent under User B's session"
    );

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// H6: hermes.approvalsDecide & computer.actionBus.approve atomic replay guard
// ============================================================================

#[test]
fn h6_hermes_approvals_decide_and_action_bus_approve_reject_replay() {
    let (host, dir) = new_prod_host("h6_approvals_replay");

    // 1. Create a real TON wallet and fund it via mock TON RPC syncChain
    let created_wallet = dispatch_ok(
        &host,
        "wallet.createWallet",
        json!({ "label": "Hardening Wallet" }),
    );
    let my_addr = created_wallet["addressBounceable"]
        .as_str()
        .expect("wallet bounceable address")
        .to_string();

    let tool_call_counter = Arc::new(AtomicUsize::new(0));
    let tool_call_counter_srv = Arc::clone(&tool_call_counter);
    let recipient_addr = my_addr.clone();

    let server = MockServer::start(move |req, stream| {
        // Mock TON JSON-RPC (for syncChain funding)
        if req.path.contains("/jsonRPC") {
            let body: Value = serde_json::from_str(&req.body).unwrap_or(Value::Null);
            let m = body["method"].as_str().unwrap_or("");
            let resp = match m {
                "getAddressInformation" => json!({
                    "ok": true,
                    "result": { "balance": "10000000000", "state": "active" }
                }),
                "getWalletInformation" => json!({
                    "ok": true,
                    "result": { "seqno": 1 }
                }),
                "getTransactions" => json!({ "ok": true, "result": [] }),
                _ => json!({ "ok": true, "result": {} }),
            }
            .to_string();
            write_http(stream, "200 OK", "application/json", resp.as_bytes());
            return;
        }

        // Mock OpenAI-compatible Magpie endpoint returning wallet_send_ton tool call
        if req.path.ends_with("/chat/completions") {
            let idx = tool_call_counter_srv.fetch_add(1, Ordering::SeqCst) + 1;
            let args = json!({
                "recipient": recipient_addr,
                "amountNanotons": 1_000_000_000u64,
                "comment": format!("transfer_{idx}"),
                "broadcast": false
            })
            .to_string();
            let resp = json!({
                "id": format!("chatcmpl_{idx}"),
                "model": "mock-gpt",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": format!("call_ton_{idx}"),
                            "type": "function",
                            "function": {
                                "name": "wallet_send_ton",
                                "arguments": args
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": { "prompt_tokens": 10, "completion_tokens": 10 }
            })
            .to_string();
            write_http(stream, "200 OK", "application/json", resp.as_bytes());
            return;
        }

        write_http(stream, "404 Not Found", "application/json", b"{}");
    });

    dispatch_ok(
        &host,
        "wallet.configureRpc",
        json!({ "rpcEndpoint": format!("{}/jsonRPC", server.base_url) }),
    );
    dispatch_ok(&host, "wallet.syncChain", json!({}));
    let bal_initial = dispatch_ok(&host, "wallet.status", json!({}))["balanceNanotons"]
        .as_i64()
        .expect("initial balance");
    assert_eq!(bal_initial, 10_000_000_000);

    // Configure Magpie seat pointing at local mock server
    dispatch_ok(
        &host,
        "magpie.upsertProvider",
        json!({
            "presetId": "openai",
            "apiKey": "sk-test-local-mock",
            "baseUrl": format!("{}/v1", server.base_url),
            "defaultModel": "mock-gpt",
            "enabled": true
        }),
    );

    let session = dispatch_ok(&host, "hermes.createSession", json!({ "title": "Replay Test" }));
    let sid = session["sessionId"].as_str().expect("sessionId").to_string();

    // --- Part A: Denied -> Approved replay MUST be rejected & NEVER execute wallet_send_ton ---
    let turn1 = dispatch_ok(
        &host,
        "hermes.sendTurn",
        json!({ "sessionId": sid, "prompt": "Send 1 TON please" }),
    );
    assert_eq!(turn1["status"].as_str(), Some("waiting_approval"));
    let apr1 = turn1["pendingApproval"]["approvalId"]
        .as_str()
        .expect("apr1")
        .to_string();

    let denied = dispatch_ok(
        &host,
        "hermes.approvalsDecide",
        json!({ "approvalId": apr1, "decision": "denied", "resumeTurn": false }),
    );
    assert_eq!(denied["status"].as_str(), Some("denied"));

    let replay_after_deny = dispatch_err(
        &host,
        "hermes.approvalsDecide",
        json!({ "approvalId": apr1, "decision": "approved", "resumeTurn": false }),
    );
    assert!(
        replay_after_deny.contains("already decided"),
        "Expected 'already decided' error, got: {replay_after_deny}"
    );

    let bal_after_deny_replay = dispatch_ok(&host, "wallet.status", json!({}))["balanceNanotons"]
        .as_i64()
        .unwrap();
    assert_eq!(
        bal_after_deny_replay, bal_initial,
        "Replaying approval after denial MUST NOT debit wallet"
    );

    // --- Part B: Approved -> Approved double-execution MUST be rejected ---
    let turn2 = dispatch_ok(
        &host,
        "hermes.sendTurn",
        json!({ "sessionId": sid, "prompt": "Send 1 TON again" }),
    );
    assert_eq!(turn2["status"].as_str(), Some("waiting_approval"));
    let apr2 = turn2["pendingApproval"]["approvalId"]
        .as_str()
        .expect("apr2")
        .to_string();

    let approved = dispatch_ok(
        &host,
        "hermes.approvalsDecide",
        json!({ "approvalId": apr2, "decision": "approved", "resumeTurn": false }),
    );
    assert_eq!(approved["status"].as_str(), Some("approved"));
    let bal_after_first_approve = dispatch_ok(&host, "wallet.status", json!({}))["balanceNanotons"]
        .as_i64()
        .unwrap();
    assert!(
        bal_after_first_approve < bal_initial,
        "First valid approval should execute wallet_send_ton once"
    );

    let double_approve_err = dispatch_err(
        &host,
        "hermes.approvalsDecide",
        json!({ "approvalId": apr2, "decision": "approved", "resumeTurn": false }),
    );
    assert!(
        double_approve_err.contains("already decided"),
        "Second approval must fail with 'already decided', got: {double_approve_err}"
    );
    let bal_after_second_approve = dispatch_ok(&host, "wallet.status", json!({}))["balanceNanotons"]
        .as_i64()
        .unwrap();
    assert_eq!(
        bal_after_second_approve, bal_after_first_approve,
        "Double approval MUST NOT debit wallet a second time"
    );

    // Non-existent approval still distinguishes "not found"
    let missing_err = dispatch_err(
        &host,
        "hermes.approvalsDecide",
        json!({ "approvalId": "apr_nonexistent", "decision": "approved" }),
    );
    assert!(
        missing_err.contains("not found"),
        "Missing approval should report 'not found', got: {missing_err}"
    );

    // --- Part C: computer.actionBus.approve atomic pending_approval guard ---
    let inv1 = dispatch_ok(
        &host,
        "computer.actionBus.invoke",
        json!({ "actionId": "payment.confirm_irreversible", "approved": false }),
    );
    assert_eq!(inv1["status"].as_str(), Some("pending_approval"));
    let inv1_id = inv1["invocationId"].as_str().expect("inv1_id").to_string();

    dispatch_ok(
        &host,
        "computer.actionBus.approve",
        json!({ "invocationId": inv1_id, "decision": "denied" }),
    );
    let inv1_replay = dispatch_err(
        &host,
        "computer.actionBus.approve",
        json!({ "invocationId": inv1_id, "decision": "approved" }),
    );
    assert!(
        inv1_replay.contains("already decided"),
        "computer.actionBus.approve must reject denied->approved replay, got: {inv1_replay}"
    );

    let inv2 = dispatch_ok(
        &host,
        "computer.actionBus.invoke",
        json!({ "actionId": "account.permission_change", "approved": false }),
    );
    let inv2_id = inv2["invocationId"].as_str().expect("inv2_id").to_string();
    dispatch_ok(
        &host,
        "computer.actionBus.approve",
        json!({ "invocationId": inv2_id, "decision": "approved" }),
    );
    let inv2_replay = dispatch_err(
        &host,
        "computer.actionBus.approve",
        json!({ "invocationId": inv2_id, "decision": "approved" }),
    );
    assert!(
        inv2_replay.contains("already decided"),
        "computer.actionBus.approve must reject approved->approved replay, got: {inv2_replay}"
    );

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// M1: hermes.skillUpsert & cronUpsert honor enabled: false
// ============================================================================

#[test]
fn m1_hermes_skill_and_cron_upsert_persists_enabled_false() {
    let (host, dir) = new_prod_host("m1_skill_enabled");

    let upserted = dispatch_ok(
        &host,
        "hermes.skillUpsert",
        json!({
            "name": "sutra-notes",
            "description": "Note helper",
            "bodyMarkdown": "# Sutra Notes\nKeep summaries concise.",
            "enabled": false,
        }),
    );
    assert_eq!(upserted["enabled"].as_bool(), Some(false));

    let skills = dispatch_ok(&host, "hermes.skillsList", json!({}))["skills"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let skill = skills
        .iter()
        .find(|s| s["name"].as_str() == Some("sutra-notes"))
        .expect("sutra-notes skill present");
    assert_eq!(skill["enabled"].as_bool(), Some(false));

    // Re-enable and verify persistence
    dispatch_ok(
        &host,
        "hermes.skillUpsert",
        json!({
            "name": "sutra-notes",
            "description": "Note helper",
            "bodyMarkdown": "# Sutra Notes\nKeep summaries concise.",
            "enabled": true,
        }),
    );
    let skills2 = dispatch_ok(&host, "hermes.skillsList", json!({}))["skills"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let skill2 = skills2
        .iter()
        .find(|s| s["name"].as_str() == Some("sutra-notes"))
        .expect("sutra-notes skill present");
    assert_eq!(skill2["enabled"].as_bool(), Some(true));

    // Cron enabled: false persistence
    let cron = dispatch_ok(
        &host,
        "hermes.cronUpsert",
        json!({
            "jobId": "cron_disabled_test",
            "schedule": "0 8 * * *",
            "prompt": "Morning check",
            "enabled": false,
        }),
    );
    assert_eq!(cron["enabled"].as_bool(), Some(false));
    let jobs = dispatch_ok(&host, "hermes.cronList", json!({}))["jobs"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let job = jobs
        .iter()
        .find(|j| j["jobId"].as_str() == Some("cron_disabled_test"))
        .expect("cron job present");
    assert_eq!(job["enabled"].as_bool(), Some(false));

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// M2: hermes.cronUpsert rejects huge step without usize overflow panic
// ============================================================================

#[test]
fn m2_hermes_cron_huge_step_rejected_without_overflow() {
    let (host, dir) = new_prod_host("m2_cron_step");

    let err1 = dispatch_err(
        &host,
        "hermes.cronUpsert",
        json!({
            "schedule": "1-5/18446744073709551615 * * * *",
            "prompt": "overflow probe",
        }),
    );
    assert!(
        err1.contains("out of bounds") || err1.contains("Invalid cron step"),
        "Expected clean cron step rejection, got: {err1}"
    );

    let err2 = dispatch_err(
        &host,
        "hermes.cronUpsert",
        json!({
            "schedule": "*/18446744073709551615 * * * *",
            "prompt": "overflow probe 2",
        }),
    );
    assert!(
        err2.contains("out of bounds") || err2.contains("Invalid cron step"),
        "Expected clean cron step rejection, got: {err2}"
    );

    // Valid max-boundary steps still work cleanly
    let valid = dispatch_ok(
        &host,
        "hermes.cronUpsert",
        json!({
            "schedule": "*/60 0-23/24 1-31/31 1-12/12 *",
            "prompt": "valid boundary steps",
        }),
    );
    assert!(valid["nextRunMs"].as_i64().unwrap_or(0) > 0);

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// M3: messaging.rs blob dot-traversal & 64 MiB sparse-file cap
// ============================================================================

#[test]
fn m3_messaging_blob_rejects_dot_traversal_and_sparse_oversize() {
    let (host, dir) = new_prod_host("m3_blob_hardening");

    for bad_id in [".", "..", "a..b", "../escape"] {
        let e1 = dispatch_err(
            &host,
            "feature.messaging.execute",
            json!({ "envelope": { "command": { "type": "beginBlobUpload", "metadata": { "id": bad_id } } } }),
        );
        assert!(e1.contains("Invalid blobId"), "beginBlobUpload must reject '{bad_id}': {e1}");

        let e2 = dispatch_err(
            &host,
            "feature.messaging.blob.read",
            json!({ "blobId": bad_id }),
        );
        assert!(e2.contains("Invalid blobId"), "blob.read must reject '{bad_id}': {e2}");
    }

    // Begin a valid blob, then attempt 100 MiB sparse seek & u64::MAX overflow seek
    dispatch_ok(
        &host,
        "feature.messaging.execute",
        json!({ "envelope": { "command": { "type": "beginBlobUpload", "metadata": { "id": "valid_blob_1" } } } }),
    );
    let e_100m = dispatch_err(
        &host,
        "feature.messaging.execute",
        json!({
            "envelope": {
                "command": {
                    "type": "appendBlobChunk",
                    "blobId": "valid_blob_1",
                    "offset": 100u64 * 1024 * 1024,
                    "dataBase64": "QQ=="
                }
            }
        }),
    );
    assert!(
        e_100m.contains("exceeds maximum allowed size"),
        "100MiB sparse offset must be rejected: {e_100m}"
    );

    let e_overflow = dispatch_err(
        &host,
        "feature.messaging.execute",
        json!({
            "envelope": {
                "command": {
                    "type": "appendBlobChunk",
                    "blobId": "valid_blob_1",
                    "offset": u64::MAX,
                    "dataBase64": "QQ=="
                }
            }
        }),
    );
    assert!(
        e_overflow.contains("overflow"),
        "u64::MAX offset must be rejected with overflow: {e_overflow}"
    );

    // Pre-create an oversized sparse file (> 64 MiB) directly on disk and verify finishBlobUpload rejects BEFORE fs::read
    let blob_dir = dir.join("messaging_blobs");
    fs::create_dir_all(&blob_dir).expect("create blob dir");
    let oversized_path = blob_dir.join("oversized_on_disk");
    {
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&oversized_path)
            .expect("open oversized file");
        f.seek(SeekFrom::Start(65 * 1024 * 1024)).expect("seek 65MiB");
        f.write_all(&[1u8]).expect("write 1 byte");
    }
    let e_finish = dispatch_err(
        &host,
        "feature.messaging.execute",
        json!({ "envelope": { "command": { "type": "finishBlobUpload", "blobId": "oversized_on_disk" } } }),
    );
    assert!(
        e_finish.contains("exceeds maximum allowed"),
        "finishBlobUpload must reject >64MiB file before reading into RAM: {e_finish}"
    );

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// M4 & NIT #5: magpie.rs loopback host delimiter, non-SSE size cap & timeout
// ============================================================================

#[test]
fn m4_magpie_loopback_spoof_rejected_and_bounded_non_sse_response_cap() {
    // NIT #5: is_local_loopback_url strict host boundary checks
    assert!(is_local_loopback_url("http://localhost"));
    assert!(is_local_loopback_url("http://localhost:11434/v1"));
    assert!(is_local_loopback_url("https://localhost/v1"));
    assert!(is_local_loopback_url("http://127.0.0.1:8080/v1"));
    assert!(is_local_loopback_url("http://[::1]:8000/v1"));
    assert!(is_local_loopback_url("https://[::1]/v1"));

    assert!(!is_local_loopback_url("http://localhost.evil.com/v1"));
    assert!(!is_local_loopback_url("https://localhost.evil.com"));
    assert!(!is_local_loopback_url("http://localhost@evil.com/v1"));
    assert!(!is_local_loopback_url("http://127.0.0.1.evil.com/v1"));
    assert!(!is_local_loopback_url("http://127.0.0.10/v1"));
    assert!(!is_local_loopback_url("https://api.openai.com/v1"));

    // M4: Oversized non-SSE response (>16 MiB) when stream=true must fail cleanly without OOM
    let (host, dir) = new_prod_host("m4_magpie_cap");
    let server = MockServer::start(move |req, stream| {
        if req.path.ends_with("/chat/completions") {
            let total = 16 * 1024 * 1024 + 4096;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n"
            );
            let _ = stream.write_all(head.as_bytes());
            let chunk = vec![b'a'; 64 * 1024];
            let mut remaining = total;
            while remaining > 0 {
                let n = remaining.min(chunk.len());
                if stream.write_all(&chunk[..n]).is_err() {
                    break;
                }
                remaining -= n;
            }
            let _ = stream.flush();
            return;
        }
        if req.path == "/models-bad" {
            write_http(stream, "500 Internal Server Error", "text/plain", b"broken");
            return;
        }
        write_http(stream, "404 Not Found", "application/json", b"{}");
    });

    dispatch_ok(
        &host,
        "magpie.upsertProvider",
        json!({
            "presetId": "openai",
            "apiKey": "sk-test-oversize",
            "baseUrl": format!("{}/v1", server.base_url),
            "defaultModel": "mock-gpt",
            "enabled": true
        }),
    );

    let err = dispatch_err(
        &host,
        "magpie.chatComplete",
        json!({
            "prompt": "trigger giant non-sse body",
            "stream": true,
            "liveUpstream": true
        }),
    );
    assert!(
        err.contains("exceeded maximum allowed size"),
        "Oversized non-SSE response must fail with size cap error, got: {err}"
    );

    // M4: fetchModelsDevCatalog fails cleanly against broken local endpoint with bounded agent
    let cat_err = dispatch_err(
        &host,
        "magpie.fetchModelsDevCatalog",
        json!({
            "url": format!("{}/models-bad", server.base_url),
            "forceRefresh": true
        }),
    );
    assert!(!cat_err.is_empty(), "broken catalog endpoint must return an error");

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// LOW / NIT: telegram_ext dot-segment URL encoding & wallet utime saturating_mul
// ============================================================================

#[test]
fn low_path_segment_encodes_dots_and_wallet_utime_saturates() {
    assert_eq!(url_encode_path_segment("."), "%2E");
    assert_eq!(url_encode_path_segment(".."), "%2E%2E");
    assert_eq!(url_encode_path_segment("cloud:42:prod-1"), "cloud:42:prod-1");
    assert_eq!(url_encode_path_segment("a/b"), "a%2Fb");

    let (host, dir) = new_prod_host("low_dots_wallet_utime");
    let captured_paths = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured_paths_srv = Arc::clone(&captured_paths);

    let server = MockServer::start(move |req, stream| {
        captured_paths_srv.lock().unwrap().push(req.path.clone());
        if req.method == "POST" && req.path == "/api/v1/auth/login" {
            let resp = json!({
                "token": "tok_dot_test",
                "user": { "id": 55, "username": "dotuser", "first_name": "Dot" },
                "session_id": "sess_dot"
            })
            .to_string();
            write_http(stream, "200 OK", "application/json", resp.as_bytes());
            return;
        }
        if req.method == "GET" && req.path.starts_with("/api/v1/media/") {
            write_http(stream, "200 OK", "application/octet-stream", b"hello");
            return;
        }
        if req.path.contains("/jsonRPC") {
            let body: Value = serde_json::from_str(&req.body).unwrap_or(Value::Null);
            let m = body["method"].as_str().unwrap_or("");
            let resp = match m {
                "getAddressInformation" => json!({
                    "ok": true,
                    "result": { "balance": "5000000000", "state": "active" }
                }),
                "getWalletInformation" => json!({
                    "ok": true,
                    "result": { "seqno": 2 }
                }),
                "getTransactions" => json!({
                    "ok": true,
                    "result": [{
                        "transaction_id": { "hash": "huge_utime_tx_hash" },
                        "utime": i64::MAX,
                        "fee": "1000",
                        "in_msg": { "value": "1000000", "source": "UQTestSource" }
                    }]
                }),
                _ => json!({ "ok": true, "result": {} }),
            }
            .to_string();
            write_http(stream, "200 OK", "application/json", resp.as_bytes());
            return;
        }
        write_http(stream, "404 Not Found", "application/json", b"{}");
    });

    dispatch_ok(
        &host,
        "telegram.server.configureEndpoint",
        json!({ "endpoint": server.base_url }),
    );
    dispatch_ok(
        &host,
        "telegram.server.login",
        json!({ "username": "dotuser", "password": "pw" }),
    );

    // downloadMedia with slash-containing fileId encodes '/' to '%2F' so path stays under /api/v1/media/
    dispatch_ok(&host, "telegram.server.downloadMedia", json!({ "fileId": "nested/file.bin" }));

    let paths = captured_paths.lock().unwrap().clone();
    assert!(
        paths.iter().any(|p| p == "/api/v1/media/nested%2Ffile.bin"),
        "Expected '/api/v1/media/nested%2Ffile.bin' in captured paths, got: {paths:?}"
    );

    // wallet.syncChain with utime = i64::MAX MUST saturate cleanly without panic
    dispatch_ok(&host, "wallet.createWallet", json!({ "label": "Utime Wallet" }));
    dispatch_ok(
        &host,
        "wallet.configureRpc",
        json!({ "rpcEndpoint": format!("{}/jsonRPC", server.base_url) }),
    );
    let synced = dispatch_ok(&host, "wallet.syncChain", json!({}));
    assert_eq!(synced["syncedTransactions"].as_u64(), Some(1));

    let _ = fs::remove_dir_all(&dir);
}
