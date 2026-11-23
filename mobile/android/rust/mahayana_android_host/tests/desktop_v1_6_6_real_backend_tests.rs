use mahayana_app_host::MahayanaAndroidHost;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "fabushi_v166_real_{}_{}",
        tag,
        uuid::Uuid::new_v4().simple()
    ))
}

fn new_prod_host(tag: &str) -> (MahayanaAndroidHost, PathBuf) {
    let dir = temp_dir(tag);
    let host = MahayanaAndroidHost::new(&dir, false).expect("create prod host");
    (host, dir)
}

fn new_test_host(tag: &str) -> (MahayanaAndroidHost, PathBuf) {
    let dir = temp_dir(tag);
    let host = MahayanaAndroidHost::new(&dir, true).expect("create test host");
    (host, dir)
}

#[derive(Debug, Clone)]
struct CapturedRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl CapturedRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn json_body(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
}

fn read_http_request(stream: &mut TcpStream) -> Option<CapturedRequest> {
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

    Some(CapturedRequest {
        method,
        path,
        headers,
        body,
    })
}

fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    extra_headers: &[(&str, &str)],
    body: &[u8],
) {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in extra_headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

struct MockServer {
    url: String,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockServer {
    fn start<F>(handler: F) -> Self
    where
        F: Fn(CapturedRequest) -> (String, String, Vec<(String, String)>, Vec<u8>) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        listener.set_nonblocking(true).expect("set nonblocking");
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{addr}");
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            while !stop_flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        if let Some(req) = read_http_request(&mut stream) {
                            let (status, ct, extra, body) = handler(req);
                            let refs: Vec<(&str, &str)> = extra
                                .iter()
                                .map(|(k, v)| (k.as_str(), v.as_str()))
                                .collect();
                            write_http_response(&mut stream, &status, &ct, &refs, &body);
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            url,
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

fn json_resp(status: &str, v: Value) -> (String, String, Vec<(String, String)>, Vec<u8>) {
    (
        status.to_string(),
        "application/json".to_string(),
        Vec::new(),
        v.to_string().into_bytes(),
    )
}

// ============================================================================
// 1. AC-2: Zero-Fake-Data Cold Start & Fail-Closed Invariants (R-F1..R-F11)
// ============================================================================

#[test]
fn test_zero_fake_data_cold_start_and_fail_closed_invariants() {
    let (host, dir) = new_prod_host("zero_fake");

    // feature.info parity metadata
    let info = host.dispatch("feature.info", &json!({})).unwrap();
    assert_eq!(info["desktopParityVersion"], "1.6.6");
    assert_eq!(info["desktopParityCommit"], "6ca754c");

    // Default endpoint unified to desktop v1.6.6 IP
    let status = host.dispatch("telegram.server.status", &json!({})).unwrap();
    assert_eq!(status["endpoint"], "http://34.46.77.133:8080");

    // R-F1: zero seeded conversations/actors/messages/folders on production host
    let access = host
        .dispatch("feature.messaging.access.issue", &json!({}))
        .unwrap();
    let actor_id = access["actorId"].as_str().unwrap();
    let synced = host
        .dispatch(
            "feature.messaging.execute",
            &json!({
                "requestId": "req-sync",
                "envelope": {
                    "protocolVersion": 2,
                    "context": {
                        "requestId": "req-sync",
                        "deviceId": "android:native",
                        "actorId": actor_id,
                        "sessionId": "s1",
                        "sentAtMs": 1_700_000_000_000i64
                    },
                    "command": { "type": "sync", "cursor": null, "limit": 1000 }
                }
            }),
        )
        .unwrap();
    let batch = &synced["envelopes"][0]["event"];
    assert!(batch["conversations"].as_array().unwrap().is_empty());
    assert!(batch["actors"].as_array().unwrap().is_empty());
    assert!(batch["messages"].as_array().unwrap().is_empty());
    assert!(batch["folders"].as_array().unwrap().is_empty());

    // R-F2: zero fake cloud nodes; local-android-mika starts standby
    let nodes = host.dispatch("computer.nodes", &json!({})).unwrap();
    let node_arr = nodes["nodes"].as_array().unwrap();
    assert_eq!(node_arr.len(), 1);
    assert_eq!(node_arr[0]["nodeId"], "local-android-mika");
    assert_eq!(node_arr[0]["status"], "standby");
    let mika_status = host.dispatch("mika.bridgeStatus", &json!({})).unwrap();
    assert_eq!(mika_status["available"], false);

    // R-F3: computer.executeOp fails closed (never fake executed=true)
    let lease = host
        .dispatch(
            "computer.leaseAcquire",
            &json!({ "nodeId": "local-android-mika", "holder": "test" }),
        )
        .unwrap();
    let err = host
        .dispatch(
            "computer.executeOp",
            &json!({ "nodeId": "local-android-mika", "generation": lease["generation"], "op": "screenshot" }),
        )
        .unwrap_err();
    assert!(err.contains("LOCAL_EXECUTOR_REQUIRED"));

    // R-F4 & R-F5: remote assist, sessions & MCP grants fail closed offline
    assert!(host
        .dispatch("remote.getAssistConfig", &json!({}))
        .unwrap_err()
        .contains("REMOTE_ASSIST_UNAVAILABLE"));
    assert!(host
        .dispatch("remote.listSessions", &json!({}))
        .unwrap_err()
        .contains("REMOTE_ASSIST_UNAVAILABLE"));
    assert!(host
        .dispatch("remote.listMcpGrants", &json!({}))
        .unwrap_err()
        .contains("AUTH_REQUIRED"));
    assert!(host
        .dispatch("remote.issueMcpGrant", &json!({ "clientName": "X" }))
        .unwrap_err()
        .contains("OAUTH_FLOW_REQUIRED"));

    // R-F6: Hermes kanban/approvals/skills start empty
    assert!(host.dispatch("hermes.kanbanBoard", &json!({})).unwrap()["cards"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(host.dispatch("hermes.approvalsList", &json!({})).unwrap()["approvals"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(host.dispatch("hermes.skillsList", &json!({})).unwrap()["skills"]
        .as_array()
        .unwrap()
        .is_empty());

    // R-F7: Wallet zero fake collectibles + tonConnectApprove fail closed + zero/invalid sendTon guard
    let w = host
        .dispatch("wallet.createWallet", &json!({ "initialNanotons": 1_000_000_000i64 }))
        .unwrap();
    assert!(host.dispatch("wallet.collectibles", &json!({})).unwrap()["collectibles"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(host
        .dispatch("wallet.tonConnectApprove", &json!({}))
        .unwrap_err()
        .contains("TONCONNECT_UNAVAILABLE"));
    assert!(host
        .dispatch(
            "wallet.sendTon",
            &json!({ "to": w["address"], "amountNanotons": 0u64 })
        )
        .is_err());
    assert!(host
        .dispatch(
            "wallet.sendTon",
            &json!({ "to": "not-a-ton-address", "amountNanotons": 1000u64 })
        )
        .is_err());

    // R-F8: zero fake accounts, stories, gifs, inline queries, callback queries
    assert!(host.dispatch("telegram.auth.accountsList", &json!({})).unwrap()["accounts"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(host.dispatch("telegram.messaging.listStories", &json!({})).unwrap()["stories"]
        .as_array()
        .unwrap()
        .is_empty());
    let gifs = host
        .dispatch("telegram.messaging.searchGifs", &json!({ "query": "zen" }))
        .unwrap();
    assert!(gifs["gifs"].as_array().unwrap().is_empty());
    assert_eq!(gifs["unavailable"], true);
    let inl = host
        .dispatch("telegram.messaging.inlineQuery", &json!({ "query": "hi" }))
        .unwrap();
    assert!(inl["results"].as_array().unwrap().is_empty());
    assert_eq!(inl["unavailable"], true);
    assert!(host
        .dispatch(
            "telegram.messaging.callbackQuery",
            &json!({ "callbackData": "test" })
        )
        .is_err());
    assert!(host
        .dispatch("telegram.auth.qrPairPoll", &json!({ "qrToken": "abc" }))
        .is_err());

    // R-F9: VoIP call lifecycle fails closed; VAD DSP continues working
    for call_op in [
        "telegram.calls.startCall",
        "telegram.calls.joinGroupCall",
        "telegram.calls.signal",
        "telegram.calls.toggleMute",
        "telegram.calls.endCall",
    ] {
        let e = host.dispatch(call_op, &json!({ "callId": "c1" })).unwrap_err();
        assert!(e.contains("CALL_TRANSPORT_UNAVAILABLE"), "{call_op} -> {e}");
    }
    let vad = host
        .dispatch("telegram.calls.vadFeedPcm", &json!({ "pcmSamples": [0, 0, 0], "reset": true }))
        .unwrap();
    assert!(vad.get("speechDetected").is_some());

    // R-F10: zero hardcoded bots in feature.execute(bot.list)
    host.dispatch(
        "feature.execute",
        &json!({ "command": { "type": "bot.list", "requestId": "b0" } }),
    )
    .unwrap();
    let ev = host.dispatch("feature.receive", &json!({})).unwrap();
    assert_eq!(ev["type"], "bot.listed");
    assert!(ev["bots"].as_array().unwrap().is_empty());

    // R-F11: production auth mocks fail closed; aiDharmaChat fails closed; codingAgent commitHash is null; magpie latency is 0
    assert!(host
        .dispatch("feature.auth.oauthStart", &json!({ "provider": "google" }))
        .unwrap_err()
        .contains("AUTH_TEST_ONLY"));
    assert!(host
        .dispatch(
            "feature.auth.passwordLogin",
            &json!({ "username": "alice", "password": "password123" })
        )
        .unwrap_err()
        .contains("AUTH_TEST_ONLY"));
    assert!(host
        .dispatch("fabushiMiniapp.aiDharmaChat", &json!({ "prompt": "hi" }))
        .unwrap_err()
        .contains("PROVIDER_NOT_CONFIGURED"));

    let cbot = host
        .dispatch(
            "codingAgent.createBot",
            &json!({ "providerKind": "claude_code", "displayName": "Dev", "username": "dev_bot" }),
        )
        .unwrap();
    let turn = host
        .dispatch(
            "codingAgent.recordTurn",
            &json!({ "botId": cbot["botId"], "summary": "Checked files" }),
        )
        .unwrap();
    assert!(turn["commitHash"].is_null());

    let magpie_st = host.dispatch("magpie.providers", &json!({})).unwrap();
    for p in magpie_st["providers"].as_array().unwrap() {
        assert_eq!(p["configuredForLive"], false);
        assert_eq!(p["lastLatencyMs"], 0);
    }

    let _ = std::fs::remove_dir_all(dir);
}

// ============================================================================
// 2. AC-3: channelStats SQL Fix, Legacy Endpoint Migration, Panic Safety
// ============================================================================

#[test]
fn test_channel_stats_sql_fix_endpoint_migration_and_panic_safety() {
    let dir = temp_dir("sql_fix");
    {
        let host = MahayanaAndroidHost::new(&dir, false).unwrap();
        let actor = "human:account:tester";
        let conv_ev = host
            .dispatch(
                "feature.messaging.execute",
                &json!({
                    "requestId": "req-c1",
                    "envelope": {
                        "protocolVersion": 2,
                        "context": {
                            "requestId": "req-c1",
                            "deviceId": "android:native",
                            "actorId": actor,
                            "sessionId": "s1",
                            "sentAtMs": 1_700_000_000_000i64
                        },
                        "command": {
                            "type": "createConversation",
                            "conversation": {
                                "id": "conv_real_channel",
                                "kind": "channel",
                                "title": "Real Channel",
                                "participants": [
                                    { "actorId": actor, "role": "owner", "joinedAtMs": 1000 },
                                    { "actorId": "human:account:peer2", "role": "member", "joinedAtMs": 1000 }
                                ]
                            }
                        }
                    }
                }),
            )
            .unwrap();
        let conv_id = conv_ev["envelopes"][0]["event"]["conversation"]["id"]
            .as_str()
            .unwrap()
            .to_string();

        let m1 = host
            .dispatch(
                "feature.messaging.execute",
                &json!({
                    "requestId": "req-m1",
                    "envelope": {
                        "protocolVersion": 2,
                        "context": {
                            "requestId": "req-m1",
                            "deviceId": "android:native",
                            "actorId": actor,
                            "sessionId": "s1",
                            "sentAtMs": 1_700_000_000_001i64
                        },
                        "command": {
                            "type": "sendMessage",
                            "conversationId": conv_id,
                            "content": { "type": "text", "data": { "text": { "text": "Announcement 1", "entities": [] } } }
                        }
                    }
                }),
            )
            .unwrap();
        let msg1_id = m1["envelopes"][0]["event"]["message"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        host.dispatch(
            "feature.messaging.execute",
            &json!({
                "requestId": "req-p1",
                "envelope": {
                    "protocolVersion": 2,
                    "context": {
                        "requestId": "req-p1",
                        "deviceId": "android:native",
                        "actorId": actor,
                        "sessionId": "s1",
                        "sentAtMs": 1_700_000_000_002i64
                    },
                    "command": {
                        "type": "pinMessage",
                        "conversationId": conv_id,
                        "messageId": msg1_id,
                        "pinned": true
                    }
                }
            }),
        )
        .unwrap();

        let stats = host
            .dispatch(
                "telegram.messaging.channelStats",
                &json!({ "conversationId": conv_id }),
            )
            .unwrap();
        assert_eq!(stats["totalMessages"], 1);
        assert_eq!(stats["pinnedMessages"], 1);
        assert_eq!(stats["subscribers"], 2);
        assert_eq!(stats["source"], "sqlite_local");

        // Set legacy endpoint manually via setEndpoint alias, then verify migration on cold reopen
        let set_ep = host
            .dispatch(
                "telegram.server.setEndpoint",
                &json!({ "endpoint": "https://telegram.fabushi.ombhrum.com" }),
            )
            .unwrap();
        assert_eq!(set_ep["saved"], true);
    }

    // Reopen production host: legacy endpoint MUST migrate to http://34.46.77.133:8080
    let host2 = MahayanaAndroidHost::new(&dir, false).unwrap();
    let st2 = host2.dispatch("telegram.server.status", &json!({})).unwrap();
    assert_eq!(st2["endpoint"], "http://34.46.77.133:8080");

    // E2E-T2-E4: Panic safety on malformed JSON, unknown method, non-ASCII hex
    let bad_json: Value = serde_json::from_str(&host2.dispatch_json("{not valid json")).unwrap();
    assert_eq!(bad_json["ok"], false);
    let unknown: Value =
        serde_json::from_str(&host2.dispatch_json(r#"{"method":"does.not.exist","params":{}}"#))
            .unwrap();
    assert_eq!(unknown["ok"], false);
    assert!(mahayana_app_host::e2ee::decode_hex_vec("你好世界").is_err());
    assert!(mahayana_app_host::telegram_ext::verify_secret_argon2id("1234", "你好", "00").is_err());

    let _ = std::fs::remove_dir_all(dir);
}

// ============================================================================
// 3. AC-3 / E2E-T2-E1: Lock-Free Concurrent HTTP + Local SQLite Proof
// ============================================================================

#[test]
fn test_lock_free_http_concurrency_e2e_t2_e1() {
    let srv = MockServer::start(|req| {
        if req.method == "POST" && req.path == "/api/v1/auth/login" {
            return json_resp(
                "200 OK",
                json!({
                    "user_id": 100,
                    "username": "alice",
                    "session_id": "sess_lockfree",
                    "token": "tok_lockfree_12345678901234567890"
                }),
            );
        }
        if req.method == "GET" && req.path == "/api/v1/chats" {
            thread::sleep(Duration::from_millis(400));
            return json_resp(
                "200 OK",
                json!([
                    { "id": 101, "title": "Delayed Chat", "kind": "direct", "is_pinned": false }
                ]),
            );
        }
        json_resp("404 Not Found", json!({ "error": "not_found" }))
    });

    let (host, dir) = new_test_host("lockfree");
    host.dispatch(
        "telegram.server.login",
        &json!({ "endpoint": srv.url, "username": "alice", "password": "secret" }),
    )
    .unwrap();

    let host_arc = Arc::new(host);
    let host_a = Arc::clone(&host_arc);
    let thread_a = thread::spawn(move || {
        let t0 = Instant::now();
        let res = host_a.dispatch("telegram.server.listChats", &json!({})).unwrap();
        (res, t0.elapsed())
    });

    // Wait 50ms so Thread A is guaranteed inside the 400ms blocking HTTP read
    thread::sleep(Duration::from_millis(50));

    let t_b = Instant::now();
    let info = host_arc.dispatch("feature.info", &json!({})).unwrap();
    let sess = host_arc.dispatch("telegram.server.session", &json!({})).unwrap();
    let saved = host_arc
        .dispatch(
            "feature.persistence.set",
            &json!({ "namespace": "bench", "key": "k1", "value": "fast" }),
        )
        .unwrap();
    let b_elapsed = t_b.elapsed();

    assert_eq!(info["desktopParityVersion"], "1.6.6");
    assert_eq!(sess["authenticated"], true);
    assert_eq!(saved["stored"], true);
    assert!(
        b_elapsed < Duration::from_millis(150),
        "Local SQLite + info calls were blocked by in-flight HTTP I/O! elapsed={b_elapsed:?}"
    );

    let (chats, a_elapsed) = thread_a.join().unwrap();
    assert_eq!(chats["chats"][0]["id"], 101);
    assert!(a_elapsed >= Duration::from_millis(350));

    let _ = std::fs::remove_dir_all(dir);
}

// ============================================================================
// 4. AC-4: Pinning & Core Chat Lifecycle /api/v1/* Contracts + 401 Wipe + Bridge
// ============================================================================

#[test]
fn test_pinning_chat_lifecycle_media_sync_401_wipe_and_device_agent_bridge() {
    let calls: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let calls_srv = Arc::clone(&calls);

    let srv = MockServer::start(move |req| {
        calls_srv
            .lock()
            .unwrap()
            .push((req.method.clone(), req.path.clone()));
        let body = req.json_body();

        match (req.method.as_str(), req.path.as_str()) {
            ("POST", "/api/v1/auth/login") => json_resp(
                "200 OK",
                json!({
                    "user_id": 700,
                    "username": "alice",
                    "first_name": "Alice",
                    "session_id": "sess_700",
                    "token": "tok_core_lifecycle_1234567890"
                }),
            ),
            ("GET", "/api/v1/auth/sessions") => json_resp(
                "200 OK",
                json!([{ "id": "sess_700", "device_name": "Fabushi Android", "is_current": true }]),
            ),
            ("DELETE", "/api/v1/auth/sessions/sess_other") => json_resp(
                "200 OK",
                json!({ "revoked": true, "session_id": "sess_other", "account_generation": 2 }),
            ),
            ("DELETE", "/api/v1/auth/sessions/others") => {
                json_resp("200 OK", json!({ "revoked_count": 1 }))
            }
            ("PUT", "/api/v1/auth/profile") => json_resp(
                "200 OK",
                json!({ "user_id": 700, "username": "alice_updated", "first_name": "Alice W", "bio": "Zen" }),
            ),
            ("GET", "/api/v1/users/me/settings") => {
                json_resp("200 OK", json!({ "show_online_status": true }))
            }
            ("PATCH", "/api/v1/users/me/settings") => {
                json_resp("200 OK", json!({ "show_online_status": false }))
            }
            ("GET", "/api/v1/chats") => json_resp(
                "200 OK",
                json!([{
                    "id": 101,
                    "title": "Design Team",
                    "kind": "group",
                    "is_pinned": false,
                    "is_muted": false,
                    "is_archived": false,
                    "last_message_date_ms": 1763800001000i64
                }]),
            ),
            ("POST", "/api/v1/chats/101/pin") => {
                if body["pinned"] == json!(false) && req.header("Authorization") == Some("Bearer force_500") {
                    return json_resp("500 Internal Server Error", json!({ "error": "db_unavailable" }));
                }
                json_resp(
                    "200 OK",
                    json!({ "chat_id": 101, "is_pinned": body["pinned"], "is_muted": false, "is_archived": false }),
                )
            }
            ("POST", "/api/v1/chats/101/mute") => json_resp(
                "200 OK",
                json!({ "chat_id": 101, "is_pinned": true, "is_muted": true, "mute_until_ms": 1799999999000i64, "is_archived": false }),
            ),
            ("POST", "/api/v1/chats/101/archive") => json_resp(
                "200 OK",
                json!({ "chat_id": 101, "is_pinned": true, "is_muted": true, "is_archived": true }),
            ),
            ("POST", "/api/v1/chats/101/pins") => json_resp(
                "200 OK",
                json!({ "chat_id": 101, "pinned_message_ids": [501], "pins": [{ "message_id": 501, "text": "Pinned" }] }),
            ),
            ("GET", "/api/v1/chats/101/pins") => json_resp(
                "200 OK",
                json!({ "chat_id": 101, "pins": [{ "message_id": 501, "text": "Pinned" }] }),
            ),
            ("DELETE", "/api/v1/chats/101/pins/501") => json_resp(
                "200 OK",
                json!({ "chat_id": 101, "pinned_message_ids": [] }),
            ),
            ("GET", p) if p.starts_with("/api/v1/chats/101/messages?") => json_resp(
                "200 OK",
                json!({ "messages": [{ "id": 500, "chat_id": 101, "text": "", "content": { "type": "Photo" } }], "has_more": false }),
            ),
            ("POST", "/api/v1/messages/send") => {
                json_resp("200 OK", json!({ "message_id": 502, "pts": 9, "ok": true }))
            }
            ("PUT", "/api/v1/chats/101/messages/502") => {
                json_resp("200 OK", json!({ "message_id": 502, "text": body["text"], "edited": true }))
            }
            ("DELETE", "/api/v1/chats/101/messages/502?revoke=true") => {
                json_resp("200 OK", json!({ "deleted": true, "message_id": 502 }))
            }
            ("POST", "/api/v1/chats/101/messages/501/reactions") => {
                json_resp("200 OK", json!({ "ok": true, "reactions": [{ "emoji": "🔥", "count": 1 }] }))
            }
            ("POST", "/api/v1/chats/101/read") => {
                json_resp("200 OK", json!({ "chat_id": 101, "unread_count": 0 }))
            }
            ("POST", "/api/v1/chats/101/clear") => json_resp("200 OK", json!({ "chat_id": 101, "cleared": true })),
            ("POST", "/api/v1/chats/101/ttl") => {
                json_resp("200 OK", json!({ "chat_id": 101, "ttl_seconds": body["ttl_seconds"] }))
            }
            ("PUT", "/api/v1/chats/101/draft") => {
                json_resp("200 OK", json!({ "chat_id": 101, "draft": { "text": body["text"] } }))
            }
            ("GET", "/api/v1/chats/101/draft?topic_id=0") => {
                json_resp("200 OK", json!({ "chat_id": 101, "draft": { "text": "WIP draft" } }))
            }
            ("DELETE", "/api/v1/chats/101/draft?topic_id=0") => {
                json_resp("200 OK", json!({ "chat_id": 101, "cleared": true }))
            }
            ("POST", "/api/v1/folders") => {
                json_resp("200 OK", json!({ "id": 7, "title": body["title"], "included_chat_ids": [101] }))
            }
            ("PUT", "/api/v1/folders/7") => {
                json_resp("200 OK", json!({ "id": 7, "title": body["title"], "included_chat_ids": [101] }))
            }
            ("GET", "/api/v1/folders") => {
                json_resp("200 OK", json!([{ "id": 7, "title": "Work v2" }]))
            }
            ("DELETE", "/api/v1/folders/7") => {
                json_resp("200 OK", json!({ "deleted": true, "folder_id": 7 }))
            }
            ("POST", "/api/v1/chats/101/polls") => json_resp(
                "200 OK",
                json!({ "success": true, "poll": { "id": "poll_1", "question": body["question"] } }),
            ),
            ("POST", "/api/v1/polls/poll_1/vote") => {
                json_resp("200 OK", json!({ "success": true, "poll": { "id": "poll_1", "voted": true } }))
            }
            ("POST", "/api/v1/polls/poll_1/close") => {
                json_resp("200 OK", json!({ "success": true, "poll": { "id": "poll_1", "closed": true } }))
            }
            ("GET", "/api/v1/contacts") => {
                json_resp("200 OK", json!([{ "user_id": 701, "username": "bob" }]))
            }
            ("DELETE", "/api/v1/contacts/701") => json_resp("200 OK", json!({ "deleted": true })),
            ("GET", "/api/v1/peers/resolve?username=bob") => {
                json_resp("200 OK", json!({ "user_id": 701, "username": "bob" }))
            }
            ("POST", p) if p.starts_with("/api/v1/media/upload") => json_resp(
                "200 OK",
                json!({ "file_id": "media_abc", "media_id": "media_abc", "size_bytes": 4, "status": "completed" }),
            ),
            ("GET", "/api/v1/media/media_abc") => (
                "200 OK".to_string(),
                "image/png".to_string(),
                vec![(
                    "Content-Disposition".to_string(),
                    "attachment; filename=\"lotus.png\"".to_string(),
                )],
                vec![0x89, 0x50, 0x4E, 0x47],
            ),
            ("GET", "/api/v1/chats/101/statistics") => {
                json_resp("200 OK", json!({ "chat_id": 101, "member_count": 42 }))
            }
            ("POST", "/api/v1/chats/101/leave") => {
                json_resp("200 OK", json!({ "chat_id": 101, "deleted": true }))
            }
            ("GET", "/api/v1/auth/me") => {
                json_resp("401 Unauthorized", json!({ "error": "session_revoked" }))
            }
            _ => json_resp("404 Not Found", json!({ "error": format!("unhandled {} {}", req.method, req.path) })),
        }
    });

    let (host, dir) = new_test_host("core_chat");
    host.dispatch(
        "telegram.server.login",
        &json!({ "endpoint": srv.url, "username": "alice", "password": "secret" }),
    )
    .unwrap();

    // Verify feature.auth.deviceAgentSession bridges telegram_server_session cleanly (E2E-T1-D7)
    let agent_sess = host
        .dispatch("feature.auth.deviceAgentSession", &json!({}))
        .unwrap();
    assert_eq!(agent_sess["accessToken"], "tok_core_lifecycle_1234567890");
    assert_eq!(agent_sess["source"], "telegram_server_session");

    // Sessions & profile & settings
    assert_eq!(
        host.dispatch("telegram.server.listSessions", &json!({})).unwrap()["sessions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.revokeSession",
            &json!({ "sessionId": "sess_other" })
        )
        .unwrap()["revoked"],
        true
    );
    assert_eq!(
        host.dispatch("telegram.server.revokeOtherSessions", &json!({}))
            .unwrap()["revoked_count"],
        1
    );
    let prof = host
        .dispatch(
            "telegram.server.updateProfile",
            &json!({ "firstName": "Alice W", "username": "alice_updated", "bio": "Zen" }),
        )
        .unwrap();
    assert_eq!(prof["username"], "alice_updated");
    assert_eq!(
        host.dispatch("telegram.server.getUserSettings", &json!({}))
            .unwrap()["show_online_status"],
        true
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.updateUserSettings",
            &json!({ "partial": true, "show_online_status": false })
        )
        .unwrap()["show_online_status"],
        false
    );

    // Chats, pinning, muting, archiving + 500 error propagation
    let chats = host.dispatch("telegram.server.listChats", &json!({})).unwrap();
    assert_eq!(chats["chats"][0]["id"], 101);
    assert_eq!(
        host.dispatch("telegram.server.pinChat", &json!({ "chatId": 101, "pinned": true }))
            .unwrap()["is_pinned"],
        true
    );
    assert!(host
        .dispatch(
            "telegram.server.pinChat",
            &json!({ "chatId": 101, "pinned": false, "token": "force_500" })
        )
        .unwrap_err()
        .contains("HTTP 500"));
    assert_eq!(
        host.dispatch("telegram.server.muteChat", &json!({ "chatId": 101, "muted": true }))
            .unwrap()["is_muted"],
        true
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.archiveChat",
            &json!({ "chatId": 101, "archived": true })
        )
        .unwrap()["is_archived"],
        true
    );

    // Message pins, paginated history, media-only send, edit, delete, reaction, read, clear, ttl
    assert_eq!(
        host.dispatch(
            "telegram.server.pinMessage",
            &json!({ "chatId": 101, "messageId": 501 })
        )
        .unwrap()["pinned_message_ids"][0],
        501
    );
    assert_eq!(
        host.dispatch("telegram.server.listPinnedMessages", &json!({ "chatId": 101 }))
            .unwrap()["pins"][0]["message_id"],
        501
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.unpinMessage",
            &json!({ "chatId": 101, "messageId": 501 })
        )
        .unwrap()["pinned_message_ids"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let hist = host
        .dispatch(
            "telegram.server.getMessages",
            &json!({ "chatId": 101, "limit": 20, "beforeId": 501, "topicId": 0 }),
        )
        .unwrap();
    assert_eq!(hist["messages"][0]["id"], 500);
    // Media-only message with blank text MUST be allowed
    let media_sent = host
        .dispatch(
            "telegram.server.sendMessage",
            &json!({ "chatId": 101, "text": "", "mediaId": "media_abc", "clientNonce": "n1" }),
        )
        .unwrap();
    assert_eq!(media_sent["message_id"], 502);
    assert_eq!(
        host.dispatch(
            "telegram.server.editMessage",
            &json!({ "chatId": 101, "messageId": 502, "text": "Edited" })
        )
        .unwrap()["edited"],
        true
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.deleteMessage",
            &json!({ "chatId": 101, "messageId": 502, "revoke": true })
        )
        .unwrap()["deleted"],
        true
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.sendReaction",
            &json!({ "chatId": 101, "messageId": 501, "emoji": "🔥", "action": "add" })
        )
        .unwrap()["ok"],
        true
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.markRead",
            &json!({ "chatId": 101, "maxId": 502 })
        )
        .unwrap()["unread_count"],
        0
    );
    assert_eq!(
        host.dispatch("telegram.server.clearChat", &json!({ "chatId": 101 }))
            .unwrap()["cleared"],
        true
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.setChatTtl",
            &json!({ "chatId": 101, "ttlSeconds": 86400 })
        )
        .unwrap()["ttl_seconds"],
        86400
    );

    // Drafts, folders, polls, contacts, peer resolve, media upload/download, stats, leave
    assert_eq!(
        host.dispatch(
            "telegram.server.setDraft",
            &json!({ "chatId": 101, "text": "WIP draft", "topicId": 0 })
        )
        .unwrap()["draft"]["text"],
        "WIP draft"
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.getDraft",
            &json!({ "chatId": 101, "topicId": 0 })
        )
        .unwrap()["draft"]["text"],
        "WIP draft"
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.clearDraft",
            &json!({ "chatId": 101, "topicId": 0 })
        )
        .unwrap()["cleared"],
        true
    );
    let f1 = host
        .dispatch(
            "telegram.server.saveFolder",
            &json!({ "title": "Work", "includedChatIds": [101] }),
        )
        .unwrap();
    assert_eq!(f1["id"], 7);
    let f2 = host
        .dispatch(
            "telegram.server.saveFolder",
            &json!({ "id": 7, "title": "Work v2", "includedChatIds": [101] }),
        )
        .unwrap();
    assert_eq!(f2["title"], "Work v2");
    assert_eq!(
        host.dispatch("telegram.server.listFolders", &json!({}))
            .unwrap()["folders"][0]["id"],
        7
    );
    assert_eq!(
        host.dispatch("telegram.server.deleteFolder", &json!({ "folderId": 7 }))
            .unwrap()["deleted"],
        true
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.createPoll",
            &json!({ "chatId": 101, "question": "Launch?", "options": ["Yes", "No"] })
        )
        .unwrap()["poll"]["id"],
        "poll_1"
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.votePoll",
            &json!({ "pollId": "poll_1", "optionIds": [0] })
        )
        .unwrap()["poll"]["voted"],
        true
    );
    assert_eq!(
        host.dispatch("telegram.server.closePoll", &json!({ "pollId": "poll_1" }))
            .unwrap()["poll"]["closed"],
        true
    );
    assert_eq!(
        host.dispatch("telegram.server.listContacts", &json!({}))
            .unwrap()["contacts"][0]["username"],
        "bob"
    );
    assert_eq!(
        host.dispatch("telegram.server.resolvePeer", &json!({ "username": "@bob" }))
            .unwrap()["user_id"],
        701
    );
    assert_eq!(
        host.dispatch("telegram.server.deleteContact", &json!({ "userId": 701 }))
            .unwrap()["deleted"],
        true
    );
    let up = host
        .dispatch(
            "telegram.server.uploadMedia",
            &json!({ "dataBase64": "iVBORw==", "filename": "lotus.png", "mimeType": "image/png" }),
        )
        .unwrap();
    assert_eq!(up["file_id"], "media_abc");
    let dl = host
        .dispatch("telegram.server.downloadMedia", &json!({ "fileId": "media_abc" }))
        .unwrap();
    assert_eq!(dl["mimeType"], "image/png");
    assert_eq!(dl["sizeBytes"], 4);
    assert_eq!(dl["filename"], "lotus.png");
    assert_eq!(
        host.dispatch("telegram.server.channelStats", &json!({ "chatId": 101 }))
            .unwrap()["member_count"],
        42
    );
    assert_eq!(
        host.dispatch("telegram.server.leaveChat", &json!({ "chatId": 101 }))
            .unwrap()["deleted"],
        true
    );

    // HTTP 401 on authenticated call MUST clear session automatically
    let e401 = host.dispatch("telegram.server.me", &json!({})).unwrap_err();
    assert!(e401.contains("HTTP 401"));
    assert_eq!(
        host.dispatch("telegram.server.session", &json!({}))
            .unwrap()["authenticated"],
        false
    );

    let _ = std::fs::remove_dir_all(dir);
}

// ============================================================================
// 5. AC-5: Real Bots & Coding Agents /api/v1/* Contracts
// ============================================================================

#[test]
fn test_real_bots_and_coding_agents_http_contracts() {
    let srv = MockServer::start(|req| {
        let body = req.json_body();
        match (req.method.as_str(), req.path.as_str()) {
            ("POST", "/api/v1/auth/login") => json_resp(
                "200 OK",
                json!({ "user_id": 800, "username": "botdev", "token": "tok_bots_12345678901234567890" }),
            ),
            ("GET", "/api/v1/bots/my") => json_resp(
                "200 OK",
                json!({ "bots": [{ "bot_user_id": 9001, "username": "ops_bot", "display_name": "Ops Bot" }], "total": 1 }),
            ),
            ("POST", "/api/v1/bots") => {
                if body["username"] == json!("ab") {
                    return json_resp("400 Bad Request", json!({ "error": "invalid_username" }));
                }
                json_resp(
                    "200 OK",
                    json!({ "bot": { "bot_user_id": 9002, "username": body["username"], "display_name": body["display_name"] }, "token": "9002:REAL_SRV_TOKEN" }),
                )
            }
            ("GET", "/api/v1/fabushi-bots") => json_resp(
                "200 OK",
                json!({ "bots": [{ "bot_user_id": 9100, "username": "muse_bot", "display_name": "Muse", "execution_state": "idle", "avatar_shape": "blob", "avatar_color": "blue", "companion_chat_id": 3001 }], "total": 1 }),
            ),
            ("POST", "/api/v1/fabushi-bots") => json_resp(
                "200 OK",
                json!({ "bot_user_id": 9101, "username": "coder_bot", "display_name": body["display_name"], "companion_chat_id": 3002, "avatar_shape": body["avatar_shape"], "avatar_color": body["avatar_color"] }),
            ),
            ("GET", "/api/v1/fabushi-bots/9101") => json_resp(
                "200 OK",
                json!({ "bot_user_id": 9101, "username": "coder_bot", "execution_state": "thinking" }),
            ),
            ("PATCH", "/api/v1/fabushi-bots/9101") => json_resp(
                "200 OK",
                json!({ "bot_user_id": 9101, "avatar_shape": body["avatar_shape"], "avatar_color": body["avatar_color"], "preferred_model": body["preferred_model"] }),
            ),
            ("PATCH", "/api/v1/fabushi-bots/9999") => {
                json_resp("403 Forbidden", json!({ "error": "forbidden_not_owner" }))
            }
            ("GET", "/api/v1/bots/profile/muse_bot") => json_resp(
                "200 OK",
                json!({ "bot_user_id": 9100, "username": "muse_bot", "ai_persona": "" }),
            ),
            ("POST", "/api/v1/botfather/nl-create") => json_resp(
                "200 OK",
                json!({ "bot_user_id": 9200, "companion_chat_id": 3003, "prompt": body["prompt"] }),
            ),
            ("POST", "/api/v1/chats/3002/callback") => json_resp(
                "200 OK",
                json!({ "ok": true, "callback_data": body["callback_data"], "toast": "Approved" }),
            ),
            ("POST", "/api/v1/coding-agents/probe") => json_resp(
                "200 OK",
                json!({ "providers": [
                    { "provider_kind": "claude_code", "installed": true },
                    { "provider_kind": "codex", "installed": false }
                ] }),
            ),
            ("PUT", "/api/v1/bots/9101/computer") => json_resp(
                "200 OK",
                json!({ "bot_user_id": 9101, "bound_node_id": body["bound_node_id"], "approval_mode": body["approval_mode"] }),
            ),
            ("GET", "/api/v1/bots/9101/computer") => json_resp(
                "200 OK",
                json!({ "bot_user_id": 9101, "bound_node_id": "cloud:42:prod-1" }),
            ),
            _ => json_resp("404 Not Found", json!({ "error": format!("unhandled {} {}", req.method, req.path) })),
        }
    });

    let (host, dir) = new_test_host("real_bots");
    host.dispatch(
        "telegram.server.login",
        &json!({ "endpoint": srv.url, "username": "botdev", "password": "secret" }),
    )
    .unwrap();

    assert_eq!(
        host.dispatch("telegram.server.listMyBots", &json!({})).unwrap()["total"],
        1
    );
    assert!(host
        .dispatch(
            "telegram.server.createBot",
            &json!({ "displayName": "Bad", "username": "ab" })
        )
        .unwrap_err()
        .contains("HTTP 400"));
    let created = host
        .dispatch(
            "telegram.server.createBot",
            &json!({ "displayName": "Good Bot", "username": "good_bot" }),
        )
        .unwrap();
    assert_eq!(created["token"], "9002:REAL_SRV_TOKEN");

    let fbots = host
        .dispatch("telegram.server.listFabushiBots", &json!({}))
        .unwrap();
    assert_eq!(fbots["bots"][0]["avatar_shape"], "blob");

    let new_fb = host
        .dispatch(
            "telegram.server.createFabushiBot",
            &json!({
                "displayName": "Coder",
                "avatarShape": "hex",
                "avatarColor": "violet",
                "codingAgent": {
                    "provider_kind": "claude_code",
                    "workspace_path": "/workspace",
                    "use_worktree": true,
                    "permission_mode": "ask"
                }
            }),
        )
        .unwrap();
    assert_eq!(new_fb["companion_chat_id"], 3002);

    assert_eq!(
        host.dispatch("telegram.server.getFabushiBot", &json!({ "botId": 9101 }))
            .unwrap()["execution_state"],
        "thinking"
    );
    let patched = host
        .dispatch(
            "telegram.server.patchFabushiBot",
            &json!({ "botId": 9101, "avatarShape": "cloud", "avatarColor": "cyan", "preferredModel": "gpt-5" }),
        )
        .unwrap();
    assert_eq!(patched["avatar_shape"], "cloud");
    assert!(host
        .dispatch(
            "telegram.server.patchFabushiBot",
            &json!({ "botId": 9999, "displayName": "Hijack" })
        )
        .unwrap_err()
        .contains("HTTP 403"));

    assert_eq!(
        host.dispatch(
            "telegram.server.getBotProfile",
            &json!({ "target": "@muse_bot" })
        )
        .unwrap()["username"],
        "muse_bot"
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.botfatherNlCreate",
            &json!({ "prompt": "Build a daily reminder bot", "avatarShape": "pebble" })
        )
        .unwrap()["companion_chat_id"],
        3003
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.sendCallbackQuery",
            &json!({ "chatId": 3002, "messageId": 88, "callbackData": "coding:approve:1" })
        )
        .unwrap()["ok"],
        true
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.probeCodingAgents",
            &json!({ "deviceId": "dev-1", "timeoutMs": 3000 })
        )
        .unwrap()["providers"][0]["provider_kind"],
        "claude_code"
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.setBotComputer",
            &json!({ "botId": 9101, "boundNodeId": "cloud:42:prod-1", "approvalMode": "auto_edit" })
        )
        .unwrap()["bound_node_id"],
        "cloud:42:prod-1"
    );
    assert_eq!(
        host.dispatch("telegram.server.getBotComputer", &json!({ "botId": 9101 }))
            .unwrap()["bound_node_id"],
        "cloud:42:prod-1"
    );

    // Verify tg_server_bots write-through hydrates feature.execute(bot.list)
    host.dispatch(
        "feature.execute",
        &json!({ "command": { "type": "bot.list", "requestId": "b_synced" } }),
    )
    .unwrap();
    let ev = host.dispatch("feature.receive", &json!({})).unwrap();
    assert!(!ev["bots"].as_array().unwrap().is_empty());

    let _ = std::fs::remove_dir_all(dir);
}

// ============================================================================
// 6. AC-6: Real Remote Control, Computers (19 RemoteComputerOp), Assist,
//    Sessions, WebRTC Signal & OAuth 2.1 MCP Grants
// ============================================================================

#[test]
fn test_real_remote_control_computers_assist_sessions_webrtc_and_mcp_grants() {
    let srv = MockServer::start(|req| {
        let body = req.json_body();
        match (req.method.as_str(), req.path.as_str()) {
            ("POST", "/api/v1/auth/login") => json_resp(
                "200 OK",
                json!({ "user_id": 42, "username": "remoter", "token": "tok_rem_12345678901234567890" }),
            ),
            ("GET", "/api/v1/computers") => json_resp(
                "200 OK",
                json!([{
                    "node_id": "cloud:42:prod-1",
                    "owner_user_id": 42,
                    "kind": "cloud_server",
                    "display_name": "Prod Linux",
                    "online": true,
                    "run_policy": "always_allow_authorized"
                }]),
            ),
            ("POST", "/api/v1/computers/local/register") => {
                assert_eq!(req.header("X-Device-Id"), Some("android-dev-1"));
                json_resp(
                    "200 OK",
                    json!({ "node_id": "local:42:android-dev-1", "display_name": body["display_name"], "online": true }),
                )
            }
            ("GET", "/api/v1/computers/local/poll") => {
                assert_eq!(req.header("X-Device-Id"), Some("android-dev-1"));
                json_resp("200 OK", json!([{ "command_id": "cmd_pending_1", "node_id": "local:42:android-dev-1" }]))
            }
            ("POST", "/api/v1/computers/commands/cmd_pending_1/approve") => {
                json_resp("200 OK", json!({ "ok": true }))
            }
            ("POST", "/api/v1/computers/commands/cmd_pending_1/complete") => {
                json_resp("200 OK", json!({ "ok": true }))
            }
            ("POST", "/api/v1/computers/cloud") => json_resp(
                "200 OK",
                json!({
                    "node_id": "cloud:42:prod-1",
                    "display_name": body["display_name"],
                    "host": body["host"],
                    "has_saved_credential": true,
                    "online": true
                }),
            ),
            ("PATCH", "/api/v1/computers/cloud:42:prod-1/policy") => json_resp(
                "200 OK",
                json!({ "node_id": "cloud:42:prod-1", "run_policy": body["run_policy"] }),
            ),
            ("POST", "/api/v1/computers/cloud:42:prod-1/test") => json_resp(
                "200 OK",
                json!({ "command_id": "cmd_t", "node_id": "cloud:42:prod-1", "ok": true, "stdout": "pong", "stderr": "", "entries": [], "executed_by": "ssh", "duration_ms": 9 }),
            ),
            ("POST", "/api/v1/computers/exec") => {
                let op_tag = body["operation"]["op"].as_str().unwrap_or("");
                json_resp(
                    "200 OK",
                    json!({
                        "command_id": format!("cmd_{op_tag}"),
                        "node_id": body["node_id"],
                        "ok": true,
                        "exit_code": 0,
                        "stdout": op_tag,
                        "stderr": "",
                        "entries": [],
                        "executed_by": "remote-daemon",
                        "duration_ms": 5
                    }),
                )
            }
            ("DELETE", "/api/v1/computers/cloud:42:prod-1") => {
                json_resp("200 OK", json!({ "ok": true }))
            }
            ("GET", p) if p.starts_with("/api/v1/remote/assist/info") => json_resp(
                "200 OK",
                json!({ "assist_id": "123456789", "allow_assist": true, "code_mode": "temporary", "current_code": "AbCd1234" }),
            ),
            ("POST", "/api/v1/remote/assist/reset-code") => json_resp(
                "200 OK",
                json!({ "assist_id": "123456789", "allow_assist": true, "code_mode": "temporary", "current_code": "ZzYy9876" }),
            ),
            ("POST", "/api/v1/remote/assist/set-code") => {
                let c = body["code"].as_str().unwrap_or("");
                if c.len() < 8 || c.len() > 16 || !c.chars().all(|ch| ch.is_ascii_alphanumeric()) {
                    return json_resp("400 Bad Request", json!({ "error": "invalid_assist_code" }));
                }
                json_resp(
                    "200 OK",
                    json!({ "assist_id": "123456789", "allow_assist": true, "code_mode": "custom", "current_code": c }),
                )
            }
            ("POST", "/api/v1/remote/assist/allow") => json_resp(
                "200 OK",
                json!({ "assist_id": "123456789", "allow_assist": body["allow_assist"], "code_mode": "custom" }),
            ),
            ("POST", "/api/v1/remote/assist/start") => json_resp(
                "200 OK",
                json!({ "session_id": "rs_99", "target_node_id": "cloud:42:prod-1", "target_device_id": "123456789", "temp_relay_token": "fbrelay_xyz", "requires_host_confirm": false }),
            ),
            ("GET", "/api/v1/remote/sessions") => json_resp(
                "200 OK",
                json!({ "sessions": [{ "session_id": "rs_99", "controller_kind": "assist_guest" }] }),
            ),
            ("POST", "/api/v1/remote/sessions/disconnect") => {
                json_resp("200 OK", json!({ "disconnected": true, "session_id": body["session_id"] }))
            }
            ("POST", "/api/v1/remote/sessions/disconnect-all") => {
                json_resp("200 OK", json!({ "disconnected_count": 2, "kill_switch": true }))
            }
            ("POST", "/api/v1/remote/webrtc/signal") => {
                assert_eq!(req.header("Authorization"), Some("Bearer fbrelay_xyz"));
                json_resp("200 OK", json!({ "accepted": true }))
            }
            ("GET", "/api/v1/mcp/grants") => json_resp(
                "200 OK",
                json!([{ "family_id": "fam_mcp_1", "client_id": "c1", "client_name": "Claude", "scopes": ["remote.read"], "allowed_node_ids": ["cloud:42:prod-1"], "created_at_ms": 1, "expires_at_ms": 2 }]),
            ),
            ("DELETE", "/api/v1/mcp/grants/fam_mcp_1") => {
                json_resp("200 OK", json!({ "revoked": true, "family_id": "fam_mcp_1" }))
            }
            ("GET", "/api/v1/mcp/audit?limit=25") => {
                json_resp("200 OK", json!({ "entries": [{ "action": "exec" }], "total": 1 }))
            }
            _ => json_resp("404 Not Found", json!({ "error": format!("unhandled {} {}", req.method, req.path) })),
        }
    });

    let (host, dir) = new_test_host("real_remote");
    host.dispatch(
        "telegram.server.login",
        &json!({ "endpoint": srv.url, "username": "remoter", "password": "secret" }),
    )
    .unwrap();

    assert_eq!(
        host.dispatch("telegram.server.listComputers", &json!({}))
            .unwrap()["computers"][0]["node_id"],
        "cloud:42:prod-1"
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.registerLocalComputer",
            &json!({ "deviceId": "android-dev-1", "displayName": "Pixel 9", "ipAddress": "10.0.0.5", "osName": "Android 15", "resolution": "1080x2400" })
        )
        .unwrap()["node_id"],
        "local:42:android-dev-1"
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.pollLocalCommands",
            &json!({ "deviceId": "android-dev-1" })
        )
        .unwrap()["commands"][0]["command_id"],
        "cmd_pending_1"
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.approveComputerCommand",
            &json!({ "commandId": "cmd_pending_1", "approved": true })
        )
        .unwrap()["ok"],
        true
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.completeComputerCommand",
            &json!({ "commandId": "cmd_pending_1", "nodeId": "local:42:android-dev-1", "ok": true, "stdout": "done" })
        )
        .unwrap()["ok"],
        true
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.upsertCloudComputer",
            &json!({ "displayName": "Prod Linux", "host": "10.0.0.99", "port": 22, "username": "ubuntu", "authMethod": "ssh_password", "secret": "top-secret-pw" })
        )
        .unwrap()["node_id"],
        "cloud:42:prod-1"
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.patchComputerPolicy",
            &json!({ "nodeId": "cloud:42:prod-1", "runPolicy": "allow_read_only" })
        )
        .unwrap()["run_policy"],
        "allow_read_only"
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.testComputer",
            &json!({ "nodeId": "cloud:42:prod-1" })
        )
        .unwrap()["ok"],
        true
    );

    // Exercise tagged RemoteComputerOp variants & reject malformed variant
    for op in [
        json!({ "op": "shell_exec", "command": "uptime" }),
        json!({ "op": "read_file", "path": "/etc/hostname" }),
        json!({ "op": "write_file", "path": "/tmp/a.txt", "content": "hi" }),
        json!({ "op": "list_dir", "path": "/tmp" }),
        json!({ "op": "capture_screen" }),
        json!({ "op": "clipboard_get" }),
        json!({ "op": "clipboard_set", "text": "copied" }),
        json!({ "op": "display_list" }),
        json!({ "op": "display_switch", "display_index": 0 }),
        json!({ "op": "privacy_screen_toggle", "enabled": true }),
        json!({ "op": "power_action", "action": { "kind": "lock" } }),
        json!({ "op": "accessibility_tree", "max_depth": 4 }),
        json!({ "op": "test_connection" }),
    ] {
        let expected_tag = op["op"].as_str().unwrap().to_string();
        let res = host
            .dispatch(
                "telegram.server.execComputer",
                &json!({ "nodeId": "cloud:42:prod-1", "operation": op }),
            )
            .unwrap();
        assert_eq!(res["ok"], true);
        assert_eq!(res["stdout"], expected_tag);
    }
    assert!(host
        .dispatch(
            "telegram.server.execComputer",
            &json!({ "nodeId": "cloud:42:prod-1", "operation": { "op": "not_a_real_variant" } })
        )
        .unwrap_err()
        .contains("Invalid RemoteComputerOp"));

    assert_eq!(
        host.dispatch(
            "telegram.server.deleteComputer",
            &json!({ "nodeId": "cloud:42:prod-1" })
        )
        .unwrap()["ok"],
        true
    );

    // UU-Remote Assist + boundary validation + sessions + WebRTC + MCP grants/audit
    assert_eq!(
        host.dispatch(
            "telegram.server.assistInfo",
            &json!({ "deviceId": "android-dev-1" })
        )
        .unwrap()["assist_id"],
        "123456789"
    );
    assert_eq!(
        host.dispatch("telegram.server.assistResetCode", &json!({}))
            .unwrap()["current_code"],
        "ZzYy9876"
    );
    assert!(host
        .dispatch("telegram.server.assistSetCode", &json!({ "code": "short7" }))
        .unwrap_err()
        .contains("HTTP 400"));
    assert_eq!(
        host.dispatch("telegram.server.assistSetCode", &json!({ "code": "12345678" }))
            .unwrap()["current_code"],
        "12345678"
    );
    assert_eq!(
        host.dispatch("telegram.server.assistSetCode", &json!({ "code": "AbCdEfGh12345678" }))
            .unwrap()["current_code"],
        "AbCdEfGh12345678"
    );
    assert_eq!(
        host.dispatch("telegram.server.assistAllow", &json!({ "allowAssist": true }))
            .unwrap()["allow_assist"],
        true
    );
    let started = host
        .dispatch(
            "telegram.server.assistStart",
            &json!({ "assistId": "123456789", "accessCode": "AbCdEfGh12345678", "mode": "desktop" }),
        )
        .unwrap();
    assert_eq!(started["temp_relay_token"], "fbrelay_xyz");

    assert_eq!(
        host.dispatch("telegram.server.listRemoteSessions", &json!({}))
            .unwrap()["sessions"][0]["session_id"],
        "rs_99"
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.webrtcSignal",
            &json!({ "relayToken": "fbrelay_xyz", "payload": { "kind": "offer", "sdp": "v=0..." } })
        )
        .unwrap()["accepted"],
        true
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.disconnectRemoteSession",
            &json!({ "sessionId": "rs_99" })
        )
        .unwrap()["disconnected"],
        true
    );
    assert_eq!(
        host.dispatch("telegram.server.disconnectAllRemoteSessions", &json!({}))
            .unwrap()["kill_switch"],
        true
    );
    assert_eq!(
        host.dispatch("telegram.server.listMcpGrants", &json!({}))
            .unwrap()["grants"][0]["family_id"],
        "fam_mcp_1"
    );
    assert_eq!(
        host.dispatch(
            "telegram.server.revokeMcpGrant",
            &json!({ "familyId": "fam_mcp_1" })
        )
        .unwrap()["revoked"],
        true
    );
    assert_eq!(
        host.dispatch("telegram.server.listMcpAudit", &json!({ "limit": 25 }))
            .unwrap()["total"],
        1
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_iter2_fabushi_miniapp_lock_order_no_abba_deadlock() {
    let (host, dir) = new_prod_host("miniapp_lock_order");
    let host = Arc::new(host);
    // Warm up tables once before concurrent blast
    host.dispatch("fabushiMiniapp.bootstrap", &json!({}))
        .unwrap();

    let barrier = Arc::new(std::sync::Barrier::new(6));
    let done = Arc::new(AtomicBool::new(false));
    let iterations = 60usize;

    let mut handles = Vec::with_capacity(6);

    // Thread 1: tickScene
    {
        let h = Arc::clone(&host);
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            for _ in 0..iterations {
                let res = h
                    .dispatch(
                        "fabushiMiniapp.tickScene",
                        &json!({ "dtMs": 16.0, "width": 420.0, "height": 780.0 }),
                    )
                    .expect("tickScene should succeed under concurrency");
                assert!(res.get("snapshot").is_some());
            }
        }));
    }

    // Thread 2: dispatchUi
    {
        let h = Arc::clone(&host);
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            for i in 0..iterations {
                let tab = if i % 2 == 0 { "zen_room" } else { "home" };
                let res = h
                    .dispatch(
                        "fabushiMiniapp.dispatchUi",
                        &json!({ "command": { "type": "select_tab", "tab": tab } }),
                    )
                    .expect("dispatchUi should succeed under concurrency");
                assert!(res.get("snapshot").is_some());
            }
        }));
    }

    // Thread 3: bootstrap
    {
        let h = Arc::clone(&host);
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            for _ in 0..iterations {
                let res = h
                    .dispatch("fabushiMiniapp.bootstrap", &json!({}))
                    .expect("bootstrap should succeed under concurrency");
                assert!(res.get("profile").is_some());
            }
        }));
    }

    // Thread 4: offerDharma
    {
        let h = Arc::clone(&host);
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            for _ in 0..iterations {
                let res = h
                    .dispatch(
                        "fabushiMiniapp.offerDharma",
                        &json!({ "countryCode": "CN", "packageKey": "heart_sutra", "count": 1 }),
                    )
                    .expect("offerDharma should succeed under concurrency");
                assert!(res["totalMeritPoints"].as_i64().unwrap_or(0) > 0);
            }
        }));
    }

    // Thread 5: renderHomeScene
    {
        let h = Arc::clone(&host);
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            for _ in 0..iterations {
                let res = h
                    .dispatch(
                        "fabushiMiniapp.renderHomeScene",
                        &json!({ "width": 420.0, "height": 780.0, "dtMs": 16.0 }),
                    )
                    .expect("renderHomeScene should succeed under concurrency");
                assert!(res.get("drawCommands").is_some());
            }
        }));
    }

    // Thread 6: renderZenScene
    {
        let h = Arc::clone(&host);
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            for _ in 0..iterations {
                let res = h
                    .dispatch(
                        "fabushiMiniapp.renderZenScene",
                        &json!({ "width": 420.0, "height": 780.0, "dtMs": 16.0 }),
                    )
                    .expect("renderZenScene should succeed under concurrency");
                assert!(res.get("drawCommands").is_some());
            }
        }));
    }

    let started = Instant::now();
    for handle in handles {
        handle.join().expect("worker thread panicked");
    }
    done.store(true, Ordering::SeqCst);
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "Concurrent fabushiMiniapp calls took too long ({:?}), possible lock contention/deadlock",
        started.elapsed()
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_iter2_wallet_send_ton_overflow_and_utf8_address_safety() {
    let (host, dir) = new_prod_host("wallet_overflow_utf8");
    let created = host
        .dispatch("wallet.createWallet", &json!({ "label": "Zero Balance" }))
        .unwrap();
    let valid_addr = created["addressNonBounceable"]
        .as_str()
        .unwrap()
        .to_string();

    // 1. Overflow & i64-wraparound amounts on a 0-balance wallet MUST fail closed and NEVER mint balance
    let i64_max_plus_1: u64 = (i64::MAX as u64) + 1;
    let fee_boundary_overflow: u64 = (i64::MAX as u64) - 5_500_000 + 1;
    for bad_amount in [fee_boundary_overflow, i64_max_plus_1, u64::MAX] {
        let err = host
            .dispatch(
                "wallet.sendTon",
                &json!({
                    "to": valid_addr,
                    "amountNanotons": bad_amount,
                    "broadcast": false,
                }),
            )
            .expect_err(&format!("sendTon must reject overflow amount {bad_amount}"));
        assert!(
            err.contains("overflow") || err.contains("supported range"),
            "unexpected error for {bad_amount}: {err}"
        );
    }

    // Balance and seqno MUST remain untouched (0 / 0)
    let status = host.dispatch("wallet.status", &json!({})).unwrap();
    assert_eq!(status["balanceNanotons"], 0);
    assert_eq!(status["seqno"], 0);

    // 2. Non-ASCII 64-byte raw TON addresses MUST return Err without panicking on UTF-8 char boundaries
    let emoji_raw_addr = format!("0:{}", "🔥".repeat(16)); // 64 UTF-8 bytes
    let cjk_raw_addr = format!("0:{}a", "中".repeat(21)); // 64 UTF-8 bytes
    for bad_addr in [&emoji_raw_addr, &cjk_raw_addr] {
        let err = host
            .dispatch(
                "wallet.sendTon",
                &json!({
                    "to": bad_addr,
                    "amountNanotons": 100,
                    "broadcast": false,
                }),
            )
            .expect_err("sendTon must reject non-ASCII raw address");
        assert!(
            err.contains("64 ASCII hex chars"),
            "unexpected error for non-ASCII raw address: {err}"
        );
    }

    // Verify SQLite mutex is healthy after malformed inputs
    let status_after = host.dispatch("wallet.status", &json!({})).unwrap();
    assert_eq!(status_after["balanceNanotons"], 0);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_iter2_wallet_ton_connect_emulate_overflow_and_emoji_safety() {
    let (host, dir) = new_prod_host("wallet_emulate_safety");
    host.dispatch("wallet.createWallet", &json!({})).unwrap();

    // 1. u64::MAX must return clean overflow Err without panicking or poisoning mutex
    let err = host
        .dispatch(
            "wallet.tonConnectEmulate",
            &json!({ "amountNanotons": u64::MAX }),
        )
        .expect_err("tonConnectEmulate must reject u64::MAX overflow");
    assert!(err.contains("overflow"), "unexpected error: {err}");

    // 2. (i64::MAX + 1) must NOT wrap to negative and falsely report sufficientBalance = true
    let large_amt: u64 = (i64::MAX as u64) + 1;
    let emu_large = host
        .dispatch(
            "wallet.tonConnectEmulate",
            &json!({ "amountNanotons": large_amt }),
        )
        .expect("tonConnectEmulate should handle large non-overflowing u64 safely");
    assert_eq!(emu_large["sufficientBalance"], false);
    assert_eq!(
        emu_large["totalDebitNanotons"].as_u64().unwrap(),
        large_amt + 5_500_000
    );

    // 3. Emoji & CJK 64-byte raw addresses must NOT panic; recipientChecksumValid must be false
    let emoji_raw_addr = format!("0:{}", "🔥".repeat(16));
    let cjk_raw_addr = format!("0:{}a", "中".repeat(21));
    for bad_addr in [&emoji_raw_addr, &cjk_raw_addr] {
        let emu = host
            .dispatch(
                "wallet.tonConnectEmulate",
                &json!({ "recipient": bad_addr, "amountNanotons": 100 }),
            )
            .expect("tonConnectEmulate should return structured result without panicking");
        assert_eq!(emu["recipientChecksumValid"], false);
        assert_eq!(emu["sufficientBalance"], false);
    }

    // Confirm host SQLite mutex is NOT poisoned
    let status = host.dispatch("wallet.status", &json!({})).unwrap();
    assert_eq!(status["balanceNanotons"], 0);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_iter2_passcode_char_count_unmute_cache_device_agent_bounds_and_wheel_merit() {
    let srv = MockServer::start(|req| {
        let body = req.json_body();
        match (req.method.as_str(), req.path.as_str()) {
            ("POST", "/api/v1/auth/login") => {
                let user = body["username"].as_str().unwrap_or("");
                if user == "short_tok_user" {
                    json_resp(
                        "200 OK",
                        json!({
                            "token": "short_tok_123",
                            "user_id": 88,
                            "username": "short_tok_user",
                            "session_id": "sess_short"
                        }),
                    )
                } else {
                    let long_sid = "s".repeat(250);
                    json_resp(
                        "200 OK",
                        json!({
                            "token": "tok_valid_device_agent_session_0123456789",
                            "user_id": 89,
                            "username": "valid_user",
                            "session_id": long_sid
                        }),
                    )
                }
            }
            ("POST", "/api/v1/chats/303/mute") => {
                let muted = body["muted"].as_bool().unwrap_or(true);
                if muted {
                    json_resp(
                        "200 OK",
                        json!({
                            "chat_id": 303,
                            "is_muted": true,
                            "mute_until_ms": body["mute_until_ms"]
                        }),
                    )
                } else {
                    json_resp(
                        "200 OK",
                        json!({
                            "chat_id": 303,
                            "is_muted": false,
                            "mute_until_ms": null
                        }),
                    )
                }
            }
            _ => json_resp(
                "404 Not Found",
                json!({ "error": format!("unhandled {} {}", req.method, req.path) }),
            ),
        }
    });

    let (host, dir) = new_prod_host("polish_regressions");

    // 1. passcodeSet must count Unicode chars, NOT UTF-8 bytes
    assert!(
        host.dispatch("telegram.auth.passcodeSet", &json!({ "passcode": "🔥" }))
            .is_err(),
        "single 4-byte emoji must be rejected"
    );
    assert!(
        host.dispatch("telegram.auth.passcodeSet", &json!({ "passcode": "🔥🔥🔥" }))
            .is_err(),
        "3 emoji chars must be rejected"
    );
    let ok_pass = host
        .dispatch(
            "telegram.auth.passcodeSet",
            &json!({ "passcode": "🔥🔥🔥🔥" }),
        )
        .expect("4 emoji chars must be accepted");
    assert_eq!(ok_pass["passcodeEnabled"], true);
    let verify_ok = host
        .dispatch(
            "telegram.auth.passcodeVerify",
            &json!({ "passcode": "🔥🔥🔥🔥" }),
        )
        .unwrap();
    assert_eq!(verify_ok["valid"], true);

    // 2. feature.auth.deviceAgentSession must reject < 24-char server tokens (fail-closed)
    host.dispatch(
        "telegram.server.login",
        &json!({ "endpoint": srv.url, "username": "short_tok_user", "password": "pw" }),
    )
    .unwrap();
    assert!(
        host.dispatch("feature.auth.deviceAgentSession", &json!({}))
            .is_err(),
        "tokens shorter than 24 chars must fail closed in deviceAgentSession"
    );

    // Realistic >= 24-char token succeeds and bounds deviceId (<= 128) & sessionId (<= 200)
    host.dispatch(
        "telegram.server.login",
        &json!({ "endpoint": srv.url, "username": "valid_user", "password": "pw" }),
    )
    .unwrap();
    let sess = host
        .dispatch("feature.auth.deviceAgentSession", &json!({}))
        .expect("valid >= 24-char server session should bridge");
    assert_eq!(
        sess["accessToken"],
        "tok_valid_device_agent_session_0123456789"
    );
    assert_eq!(sess["source"], "telegram_server_session");
    let dev_id = sess["deviceId"].as_str().unwrap();
    assert!(!dev_id.is_empty() && dev_id.len() <= 128);
    let sid = sess["sessionId"].as_str().unwrap();
    assert_eq!(sid.chars().count(), 200);

    // 3. muteChat(muted: false) must clear stale mute_until_ms in SQLite tg_server_chats cache
    let mute_until: i64 = 1_893_456_000_000;
    host.dispatch(
        "telegram.server.muteChat",
        &json!({ "chatId": 303, "muted": true, "muteUntilMs": mute_until }),
    )
    .unwrap();

    let db_path = dir.join("runtime.sqlite3");
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let (is_muted, cached_until, chat_json_str): (i64, Option<i64>, String) = conn
            .query_row(
                "SELECT is_muted, mute_until_ms, chat_json FROM tg_server_chats WHERE chat_id = 303",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(is_muted, 1);
        assert_eq!(cached_until, Some(mute_until));
        let parsed: Value = serde_json::from_str(&chat_json_str).unwrap();
        assert_eq!(parsed["mute_until_ms"], mute_until);
    }

    // Now unmute and verify stale mute_until_ms is cleared to NULL in both column and JSON
    host.dispatch(
        "telegram.server.muteChat",
        &json!({ "chatId": 303, "muted": false }),
    )
    .unwrap();
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let (is_muted, cached_until, chat_json_str): (i64, Option<i64>, String) = conn
            .query_row(
                "SELECT is_muted, mute_until_ms, chat_json FROM tg_server_chats WHERE chat_id = 303",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(is_muted, 0);
        assert_eq!(
            cached_until, None,
            "SQL mute_until_ms column must be cleared to NULL on unmute"
        );
        let parsed: Value = serde_json::from_str(&chat_json_str).unwrap();
        assert!(
            parsed["mute_until_ms"].is_null(),
            "chat_json.mute_until_ms must be null on unmute, got: {parsed}"
        );
    }

    // 4. route_miniapp_input /wheel must read real totalMeritPoints (324, not fallback 972)
    let wheel_resp = host
        .dispatch(
            "platform.request",
            &json!({
                "method": "POST",
                "path": "/v1/marketplace/plugins/global-dharma/route",
                "body": { "input": "/wheel" }
            }),
        )
        .unwrap();
    let reply_text = wheel_resp["data"]["message"].as_str().unwrap_or("");
    assert!(
        reply_text.contains("功德 324"),
        "expected real merit 324 (108 * 3), got: {reply_text}"
    );
    assert!(
        !reply_text.contains("功德 972"),
        "must not show stale hardcoded fallback 972: {reply_text}"
    );

    let _ = std::fs::remove_dir_all(dir);
}

