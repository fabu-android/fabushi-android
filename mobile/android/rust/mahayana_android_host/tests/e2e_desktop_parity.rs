//! Requirement-Driven Opaque-Box Rust E2E Test Suite (`TEST_INFRA.md` Tiers 1 & 2)
//!
//! Strictly tests public contracts of `mahayana_app_host` (`MahayanaAndroidHost` + C-ABI)
//! and `gpui_telegram_proto::miniapp_contracts` against a zero-dependency in-process
//! stateful `MockGpuiTelegramServer` over loopback HTTP/1.1 with real SQLite WAL
//! cold-restart persistence (`test_mode = false`).

use base64::Engine as _;
use gpui_telegram_proto::miniapp_contracts::{
    sanitize_webapp_custom_url, validate_cloud_storage_entry, CloudStorageItem,
    CLOUD_STORAGE_MAX_KEYS, CLOUD_STORAGE_MAX_KEY_BYTES, CLOUD_STORAGE_MAX_TOTAL_BYTES,
    CLOUD_STORAGE_MAX_VALUE_BYTES,
};
use mahayana_app_host::{
    mahayana_app_host_create, mahayana_app_host_destroy, mahayana_app_host_dispatch_with_handle,
    mahayana_app_host_free_string, MahayanaAndroidHost,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// ============================================================================
// RAII TempDirGuard & Loopback Helpers
// ============================================================================

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "fabushi_e2e_rust_{}_{}",
            tag,
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&path).expect("create isolated e2e temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn prod_host(&self) -> MahayanaAndroidHost {
        MahayanaAndroidHost::new(&self.path, false).expect("open prod MahayanaAndroidHost")
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Binds an ephemeral loopback port and immediately releases it to produce a
/// deterministic connection-refused URL for offline fail-closed tests.
fn closed_loopback_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral closed port");
    let addr = listener.local_addr().expect("closed port local_addr");
    drop(listener);
    format!("http://{addr}")
}

/// Opaque-box filesystem oracle: scans raw bytes of every file in `dir`
/// (including `runtime.sqlite3`, `-wal`, `-shm`) and proves `secret` never
/// appears in plaintext anywhere on disk.
fn assert_no_plaintext_bytes_on_disk(dir: &Path, secret: &str) {
    let needle = secret.as_bytes();
    assert!(!needle.is_empty(), "secret needle must be non-empty");
    let mut scanned_sqlite = false;
    let mut scanned_count = 0usize;
    for entry in std::fs::read_dir(dir).expect("read_dir for plaintext scan") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.ends_with(".sqlite3"))
            .unwrap_or(false)
        {
            scanned_sqlite = true;
        }
        let bytes = std::fs::read(&path).expect("read file bytes for plaintext scan");
        scanned_count += 1;
        assert!(
            !bytes.windows(needle.len()).any(|w| w == needle),
            "Plaintext secret '{}' leaked on disk in {:?}",
            secret,
            path
        );
    }
    assert!(
        scanned_sqlite && scanned_count >= 1,
        "Expected runtime.sqlite3 to exist in {:?} for on-disk plaintext scan (scanned={scanned_count})",
        dir
    );
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &input[i + 1..i + 3];
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn parse_query_params(query: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if query.is_empty() {
        return map;
    }
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        if let Some((k, v)) = pair.split_once('=') {
            map.insert(percent_decode(k), percent_decode(v));
        } else {
            map.insert(percent_decode(pair), String::new());
        }
    }
    map
}

fn is_valid_assist_code(code: &str) -> bool {
    let len = code.len();
    if !(8..=16).contains(&len) {
        return false;
    }
    if !code.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return false;
    }
    let has_letter = code.bytes().any(|b| b.is_ascii_alphabetic());
    let has_digit = code.bytes().any(|b| b.is_ascii_digit());
    has_letter && has_digit
}

fn is_valid_bot_username(username: &str) -> bool {
    let u = username.trim().trim_start_matches('@');
    if u.len() < 5 || u.len() > 32 {
        return false;
    }
    let bytes = u.as_bytes();
    if !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    if !bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_') {
        return false;
    }
    u.to_ascii_lowercase().ends_with("bot")
}

// ============================================================================
// In-Process Stateful MockGpuiTelegramServer (Zero New Dependencies)
// ============================================================================

#[derive(Debug, Clone)]
struct RecordedRequest {
    method: String,
    path: String,
    query: String,
    authorization: Option<String>,
    headers: HashMap<String, String>,
    body_raw: String,
    body_json: Value,
}

#[derive(Clone)]
struct FaultRule {
    method: Option<String>,
    path_prefix: String,
    status: u16,
    error_body: Value,
    delay_ms: u64,
    remaining_hits: Option<usize>,
    drop_connection: bool,
    on_delay_entered: Option<Arc<(Mutex<bool>, Condvar)>>,
}

struct MockResponse {
    status: u16,
    reason: &'static str,
    content_type: String,
    body: Vec<u8>,
}

impl MockResponse {
    fn json(status: u16, value: Value) -> Self {
        Self {
            status,
            reason: status_reason(status),
            content_type: "application/json; charset=utf-8".to_string(),
            body: value.to_string().into_bytes(),
        }
    }

    fn binary(status: u16, content_type: &str, bytes: Vec<u8>) -> Self {
        Self {
            status,
            reason: status_reason(status),
            content_type: content_type.to_string(),
            body: bytes,
        }
    }
}

fn status_reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        410 => "Gone",
        500 => "Internal Server Error",
        _ => "Status",
    }
}

struct AlipayAttemptState {
    poll_count: usize,
    complete_on_poll: usize,
    force_status_code: Option<u16>,
    force_status_label: Option<String>,
}

struct MockServerState {
    requests: Vec<RecordedRequest>,
    faults: Vec<FaultRule>,
    active_tokens: HashMap<String, i64>, // token -> user_id
    user_profiles: BTreeMap<i64, Value>,
    sessions: BTreeMap<String, Value>,
    alipay_attempts: HashMap<String, AlipayAttemptState>,

    chats: BTreeMap<i64, Value>,
    pinned_messages: BTreeMap<i64, Vec<i64>>,
    messages: BTreeMap<i64, Vec<Value>>,
    drafts: BTreeMap<i64, Value>,
    folders: BTreeMap<i64, Value>,
    contacts: BTreeMap<i64, Value>,
    polls: BTreeMap<String, Value>,
    media_blobs: BTreeMap<String, (String, Vec<u8>, Option<String>)>,
    pts: i64,
    next_chat_id: i64,
    next_msg_id: i64,
    next_folder_id: i64,
    next_bot_user_id: i64,

    my_bots: BTreeMap<i64, Value>,
    fabushi_bots: BTreeMap<String, Value>,
    bot_owners: HashMap<String, i64>,
    bot_computers: BTreeMap<String, Value>,
    coding_agent_probes: Vec<Value>,

    computers: BTreeMap<String, Value>,
    computer_test_override: Option<Value>,
    local_commands: Vec<Value>,
    assist_id: String,
    assist_code: String,
    assist_code_mode: String,
    assist_allow: bool,
    remote_sessions: BTreeMap<String, Value>,
    mcp_grants: BTreeMap<String, Value>,
    non_admin_pin_chats: HashSet<i64>,
    missing_contact_usernames: HashSet<String>,
}

impl MockServerState {
    fn new() -> Self {
        let mut user_profiles = BTreeMap::new();
        user_profiles.insert(
            42,
            json!({
                "user_id": 42,
                "username": "alice",
                "first_name": "Alice",
                "last_name": "Zen",
                "bio": "Original bio",
                "phone": "+15550000042"
            }),
        );
        user_profiles.insert(
            99,
            json!({
                "user_id": 99,
                "username": "bob",
                "first_name": "Bob",
                "last_name": "Peer",
                "bio": "Peer user"
            }),
        );

        let mut sessions = BTreeMap::new();
        sessions.insert(
            "sess_current_android".to_string(),
            json!({
                "session_id": "sess_current_android",
                "device_name": "Fabushi Android Pixel",
                "platform": "android",
                "is_current": true,
                "created_at_ms": 1_700_000_000_000i64
            }),
        );
        sessions.insert(
            "sess_desktop_mac".to_string(),
            json!({
                "session_id": "sess_desktop_mac",
                "device_name": "GPUI Desktop macOS",
                "platform": "macos",
                "is_current": false,
                "created_at_ms": 1_700_000_100_000i64
            }),
        );

        Self {
            requests: Vec::new(),
            faults: Vec::new(),
            active_tokens: HashMap::new(),
            user_profiles,
            sessions,
            alipay_attempts: HashMap::new(),
            chats: BTreeMap::new(),
            pinned_messages: BTreeMap::new(),
            messages: BTreeMap::new(),
            drafts: BTreeMap::new(),
            folders: BTreeMap::new(),
            contacts: BTreeMap::new(),
            polls: BTreeMap::new(),
            media_blobs: BTreeMap::new(),
            pts: 10,
            next_chat_id: 200,
            next_msg_id: 1000,
            next_folder_id: 1,
            next_bot_user_id: 9001,
            my_bots: BTreeMap::new(),
            fabushi_bots: BTreeMap::new(),
            bot_owners: HashMap::new(),
            bot_computers: BTreeMap::new(),
            coding_agent_probes: vec![
                json!({ "provider": "claude_code", "installed": true, "version": "1.0.21" }),
                json!({ "provider": "codex", "installed": false, "version": null }),
                json!({ "provider": "gemini_cli", "installed": false, "version": null }),
                json!({ "provider": "opencode", "installed": false, "version": null }),
                json!({ "provider": "aider", "installed": false, "version": null }),
                json!({ "provider": "goose", "installed": false, "version": null }),
            ],
            computers: BTreeMap::new(),
            computer_test_override: None,
            local_commands: Vec::new(),
            assist_id: "123456789".to_string(),
            assist_code: "InitCode1".to_string(),
            assist_code_mode: "temporary".to_string(),
            assist_allow: false,
            remote_sessions: BTreeMap::new(),
            mcp_grants: BTreeMap::new(),
            non_admin_pin_chats: HashSet::new(),
            missing_contact_usernames: HashSet::new(),
        }
    }

    fn issue_token(&mut self, user_id: i64) -> String {
        // Strictly >= 24 chars, ASCII, no whitespace (compatible with deviceAgentSession bridge)
        let tok = format!("tok_e2e_desktop_parity_{}", uuid::Uuid::new_v4().simple());
        assert!(tok.len() >= 24 && !tok.chars().any(char::is_whitespace));
        self.active_tokens.insert(tok.clone(), user_id);
        tok
    }

    fn seed_chat(&mut self, chat_id: i64, title: &str, is_pinned: bool, last_message_date_ms: i64) {
        self.chats.insert(
            chat_id,
            json!({
                "chat_id": chat_id,
                "id": chat_id,
                "title": title,
                "kind": "group",
                "chat_type": 1,
                "is_group": true,
                "is_channel": false,
                "is_bot": false,
                "is_pinned": is_pinned,
                "is_muted": false,
                "mute_until_ms": null,
                "is_archived": false,
                "unread_count": 3,
                "read_inbox_max_id": 0,
                "read_outbox_max_id": 0,
                "member_count": 5,
                "message_ttl_seconds": 0,
                "pts": self.pts,
                "last_message_id": 500,
                "last_message_date_ms": last_message_date_ms,
                "pinned_message_id": null
            }),
        );
    }
}

struct MockGpuiTelegramServer {
    addr: SocketAddr,
    url: String,
    state: Arc<Mutex<MockServerState>>,
    stop: Arc<AtomicBool>,
    workers: Arc<Mutex<Vec<thread::JoinHandle<()>>>>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockGpuiTelegramServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind MockGpuiTelegramServer");
        let addr = listener.local_addr().expect("mock server local_addr");
        let url = format!("http://{addr}");
        let state = Arc::new(Mutex::new(MockServerState::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let workers: Arc<Mutex<Vec<thread::JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));

        let state_bg = Arc::clone(&state);
        let stop_bg = Arc::clone(&stop);
        let workers_bg = Arc::clone(&workers);

        let handle = thread::spawn(move || {
            for incoming in listener.incoming() {
                if stop_bg.load(Ordering::SeqCst) {
                    break;
                }
                match incoming {
                    Ok(stream) => {
                        if stop_bg.load(Ordering::SeqCst) {
                            break;
                        }
                        let st = Arc::clone(&state_bg);
                        let stop_worker = Arc::clone(&stop_bg);
                        let worker = thread::spawn(move || handle_connection(stream, st, stop_worker));

                        // Reap already-finished worker threads and track active ones for deterministic Drop join.
                        let finished: Vec<thread::JoinHandle<()>> = {
                            let mut guard = workers_bg
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            let prev = std::mem::take(&mut *guard);
                            let mut done = Vec::new();
                            for h in prev {
                                if h.is_finished() {
                                    done.push(h);
                                } else {
                                    guard.push(h);
                                }
                            }
                            guard.push(worker);
                            done
                        };
                        for h in finished {
                            let _ = h.join();
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            addr,
            url,
            state,
            stop,
            workers,
            handle: Some(handle),
        }
    }

    fn url(&self) -> &str {
        &self.url
    }

    fn with_state<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut MockServerState) -> R,
    {
        let mut guard = self.state.lock().expect("lock mock server state");
        f(&mut guard)
    }

    fn add_fault(&self, rule: FaultRule) {
        self.with_state(|s| s.faults.push(rule));
    }

    fn recorded_requests(&self) -> Vec<RecordedRequest> {
        self.with_state(|s| s.requests.clone())
    }

    fn revoke_all_tokens(&self) {
        self.with_state(|s| s.active_tokens.clear());
    }
}

impl Drop for MockGpuiTelegramServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        let workers = {
            let mut guard = self
                .workers
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            std::mem::take(&mut *guard)
        };
        for worker in workers {
            let _ = worker.join();
        }
    }
}

/// Incremental HTTP/1.1 chunked transfer-encoding reader (`<hex>[;ext]\r\n<bytes>\r\n...0\r\n[trailers]\r\n`).
/// Never relies on substring scanning for `0\r\n\r\n`, so chunk payloads containing literal `0\r\n\r\n` decode intact.
fn read_chunked_body(stream: &mut TcpStream, buf: &mut Vec<u8>, mut pos: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut tmp = [0u8; 4096];

    let read_more = |stream: &mut TcpStream, buf: &mut Vec<u8>, tmp: &mut [u8; 4096]| -> bool {
        match stream.read(tmp) {
            Ok(0) | Err(_) => false,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                true
            }
        }
    };

    loop {
        // 1. Locate end of `<hex-size>[;chunk-ext]\r\n` line
        let line_len = loop {
            if let Some(rel) = buf[pos..].windows(2).position(|w| w == b"\r\n") {
                break rel;
            }
            if !read_more(stream, buf, &mut tmp) {
                return out;
            }
        };
        let size_line = String::from_utf8_lossy(&buf[pos..pos + line_len]);
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        let Ok(chunk_size) = usize::from_str_radix(size_hex, 16) else {
            return out;
        };
        pos += line_len + 2;

        // 2. Final zero-length chunk: consume optional trailer lines until empty `\r\n`
        if chunk_size == 0 {
            loop {
                while buf.len() < pos + 2 {
                    if !read_more(stream, buf, &mut tmp) {
                        return out;
                    }
                }
                if &buf[pos..pos + 2] == b"\r\n" {
                    break;
                }
                let trailer_len = loop {
                    if let Some(rel) = buf[pos..].windows(2).position(|w| w == b"\r\n") {
                        break rel;
                    }
                    if !read_more(stream, buf, &mut tmp) {
                        return out;
                    }
                };
                pos += trailer_len + 2;
            }
            break;
        }

        // 3. Read exact `chunk_size` payload bytes followed by `\r\n`
        let expected_data_end = pos + chunk_size;
        let needed_with_crlf = expected_data_end + 2;
        while buf.len() < needed_with_crlf {
            if !read_more(stream, buf, &mut tmp) {
                break;
            }
        }
        let data_end = expected_data_end.min(buf.len());
        out.extend_from_slice(&buf[pos..data_end]);
        pos = data_end;
        if data_end < expected_data_end {
            break;
        }
        if buf.len() >= pos + 2 && &buf[pos..pos + 2] == b"\r\n" {
            pos += 2;
        }
    }

    out
}

fn read_http_request(stream: &mut TcpStream) -> Option<RecordedRequest> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_nodelay(true);

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
    let method = parts.next().unwrap_or("GET").to_ascii_uppercase();
    let target = parts.next().unwrap_or("/");
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.to_string(), String::new()),
    };

    let mut headers = HashMap::new();
    let mut content_len: Option<usize> = None;
    let mut is_chunked = false;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim().to_string();
            if key == "content-length" {
                content_len = val.parse::<usize>().ok();
            } else if key == "transfer-encoding" && val.to_ascii_lowercase().contains("chunked") {
                is_chunked = true;
            }
            headers.insert(key, val);
        }
    }

    let body_bytes = if is_chunked {
        read_chunked_body(stream, &mut buf, header_end)
    } else {
        let expected = content_len.unwrap_or(0);
        while buf.len() < header_end + expected {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
            }
        }
        let end = (header_end + expected).min(buf.len());
        buf[header_end..end].to_vec()
    };

    let body_raw = String::from_utf8_lossy(&body_bytes).to_string();
    let body_json = serde_json::from_str::<Value>(&body_raw).unwrap_or(Value::Null);
    let authorization = headers.get("authorization").cloned();

    Some(RecordedRequest {
        method,
        path,
        query,
        authorization,
        headers,
        body_raw,
        body_json,
    })
}

fn write_http_response(stream: &mut TcpStream, resp: &MockResponse) {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        resp.status,
        resp.reason,
        resp.content_type,
        resp.body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&resp.body);
    let _ = stream.flush();
}

fn handle_connection(
    mut stream: TcpStream,
    state: Arc<Mutex<MockServerState>>,
    stop: Arc<AtomicBool>,
) {
    let Some(req) = read_http_request(&mut stream) else {
        return;
    };

    // Phase 1: Record request and inspect matching fault rules under lock, then DROP lock before sleeping!
    let matched_fault: Option<(u64, bool, u16, Value, Option<Arc<(Mutex<bool>, Condvar)>>)> = {
        let mut st = state.lock().expect("lock state for fault check");
        st.requests.push(req.clone());

        let mut hit = None;
        let mut idx_to_remove = None;
        for (idx, rule) in st.faults.iter_mut().enumerate() {
            let method_ok = rule
                .method
                .as_deref()
                .map(|m| m.eq_ignore_ascii_case(&req.method))
                .unwrap_or(true);
            let path_ok = req.path.starts_with(&rule.path_prefix);
            let hits_ok = rule.remaining_hits.map(|h| h > 0).unwrap_or(true);
            if method_ok && path_ok && hits_ok {
                if let Some(ref mut rem) = rule.remaining_hits {
                    *rem = rem.saturating_sub(1);
                    if *rem == 0 {
                        idx_to_remove = Some(idx);
                    }
                }
                hit = Some((
                    rule.delay_ms,
                    rule.drop_connection,
                    rule.status,
                    rule.error_body.clone(),
                    rule.on_delay_entered.clone(),
                ));
                break;
            }
        }
        if let Some(idx) = idx_to_remove {
            st.faults.remove(idx);
        }
        hit
    };

    // Sleep OUTSIDE MockServerState mutex!
    if let Some((delay_ms, drop_conn, fault_status, fault_body, on_delay_entered)) = matched_fault {
        if let Some(signal) = on_delay_entered {
            let (lock, cvar) = &*signal;
            if let Ok(mut entered) = lock.lock() {
                *entered = true;
                cvar.notify_all();
            }
        }
        if delay_ms > 0 {
            let deadline = Instant::now() + Duration::from_millis(delay_ms);
            while Instant::now() < deadline && !stop.load(Ordering::SeqCst) {
                let rem = deadline.saturating_duration_since(Instant::now());
                thread::sleep(rem.min(Duration::from_millis(10)));
            }
        }
        if drop_conn || stop.load(Ordering::SeqCst) {
            return;
        }
        if fault_status > 0 {
            write_http_response(&mut stream, &MockResponse::json(fault_status, fault_body));
            return;
        }
    }

    // Phase 2: Route stateful request under lock
    let resp = {
        let mut st = state.lock().expect("lock state for route dispatch");
        route_request(&mut st, &req)
    };
    write_http_response(&mut stream, &resp);
}

fn is_public_route(method: &str, path: &str) -> bool {
    (method == "GET" && path == "/api/v1/status")
        || (method == "POST"
            && matches!(
                path,
                "/api/v1/auth/login"
                    | "/api/v1/auth/register"
                    | "/api/v1/auth/email/send-code"
                    | "/api/v1/auth/email/register"
                    | "/api/v1/auth/alipay/start"
            ))
        || (method == "GET" && path.starts_with("/api/v1/auth/alipay/poll/"))
}

fn route_request(st: &mut MockServerState, req: &RecordedRequest) -> MockResponse {
    let method = req.method.as_str();
    let path = req.path.as_str();
    let query = parse_query_params(&req.query);

    // Public routes
    if method == "GET" && path == "/api/v1/status" {
        return MockResponse::json(200, json!({ "status": "ok", "version": "1.6.6" }));
    }

    if method == "POST" && path == "/api/v1/auth/login" {
        let username = req.body_json["username"].as_str().unwrap_or("").trim();
        let password = req.body_json["password"].as_str().unwrap_or("");
        if username.is_empty() || password.is_empty() {
            return MockResponse::json(
                400,
                json!({ "error": "invalid_credentials", "message": "Username and password required" }),
            );
        }
        let user_id = if username == "bob" { 99 } else { 42 };
        let token = st.issue_token(user_id);
        let profile = st
            .user_profiles
            .get(&user_id)
            .cloned()
            .unwrap_or_else(|| json!({ "user_id": user_id, "username": username, "first_name": username }));
        return MockResponse::json(
            200,
            json!({
                "token": token,
                "user_id": user_id,
                "username": profile["username"],
                "first_name": profile["first_name"],
                "session_id": "sess_current_android"
            }),
        );
    }

    if method == "POST" && path == "/api/v1/auth/register" {
        let username = req.body_json["username"].as_str().unwrap_or("newuser");
        let first_name = req.body_json["first_name"].as_str().unwrap_or(username);
        let user_id = 42;
        st.user_profiles.insert(
            user_id,
            json!({
                "user_id": user_id,
                "username": username,
                "first_name": first_name,
                "bio": ""
            }),
        );
        let token = st.issue_token(user_id);
        return MockResponse::json(
            201,
            json!({
                "token": token,
                "user_id": user_id,
                "username": username,
                "first_name": first_name,
                "session_id": "sess_registered"
            }),
        );
    }

    if method == "POST" && path == "/api/v1/auth/email/send-code" {
        let email = req.body_json["email"].as_str().unwrap_or("");
        if !email.contains('@') {
            return MockResponse::json(
                400,
                json!({ "error": "invalid_email", "message": "Invalid email address" }),
            );
        }
        return MockResponse::json(200, json!({ "sent": true, "email": email, "ttl_seconds": 300 }));
    }

    if method == "POST" && path == "/api/v1/auth/email/register" {
        let username = req.body_json["username"].as_str().unwrap_or("emailuser");
        let first_name = req.body_json["first_name"].as_str().unwrap_or(username);
        let code = req.body_json["code"].as_str().unwrap_or("");
        if code.is_empty() {
            return MockResponse::json(
                400,
                json!({ "error": "invalid_code", "message": "Verification code required" }),
            );
        }
        let user_id = 42;
        let token = st.issue_token(user_id);
        return MockResponse::json(
            201,
            json!({
                "token": token,
                "user_id": user_id,
                "username": username,
                "first_name": first_name,
                "session_id": "sess_email_reg"
            }),
        );
    }

    if method == "POST" && path == "/api/v1/auth/alipay/start" {
        let attempt_id = format!("ali_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
        st.alipay_attempts.insert(
            attempt_id.clone(),
            AlipayAttemptState {
                poll_count: 0,
                complete_on_poll: 2,
                force_status_code: None,
                force_status_label: None,
            },
        );
        return MockResponse::json(
            200,
            json!({
                "attempt_id": attempt_id,
                "login_url": format!("https://openauth.alipay.com/oauth2/publicAppAuthorize.htm?state={attempt_id}"),
                "expires_in": 180
            }),
        );
    }

    if method == "GET" {
        if let Some(attempt_id) = path.strip_prefix("/api/v1/auth/alipay/poll/") {
            let outcome = if let Some(att) = st.alipay_attempts.get_mut(attempt_id) {
                att.poll_count += 1;
                if let Some(code) = att.force_status_code {
                    Err((code, att.force_status_label.clone().unwrap_or_else(|| "expired".into())))
                } else if let Some(ref lbl) = att.force_status_label {
                    Ok((lbl.clone(), false))
                } else if att.poll_count >= att.complete_on_poll {
                    Ok(("completed".to_string(), true))
                } else {
                    Ok(("pending".to_string(), false))
                }
            } else {
                Err((404, "attempt_not_found".to_string()))
            };

            return match outcome {
                Err((code, err)) => MockResponse::json(
                    code,
                    json!({ "error": err, "message": format!("Alipay attempt {err}") }),
                ),
                Ok((status_lbl, true)) => {
                    let token = st.issue_token(42);
                    MockResponse::json(
                        200,
                        json!({
                            "status": status_lbl,
                            "token": token,
                            "user_id": 42,
                            "username": "alice_alipay",
                            "first_name": "Alice Alipay",
                            "session_id": "sess_alipay_01"
                        }),
                    )
                }
                Ok((status_lbl, false)) => {
                    MockResponse::json(200, json!({ "status": status_lbl }))
                }
            };
        }
    }

    // Protected route enforcement
    let auth_user_id: i64 = if !is_public_route(method, path) {
        let token = req
            .authorization
            .as_deref()
            .and_then(|h| h.strip_prefix("Bearer ").or_else(|| h.strip_prefix("bearer ")))
            .map(str::trim);
        match token.and_then(|t| st.active_tokens.get(t).copied()) {
            Some(uid) => uid,
            None => {
                return MockResponse::json(
                    401,
                    json!({
                        "error": "unauthorized",
                        "message": "Invalid or expired Bearer token"
                    }),
                );
            }
        }
    } else {
        42
    };

    // ---- Auth / Profile / Sessions ----
    if method == "GET" && path == "/api/v1/auth/me" {
        let profile = st
            .user_profiles
            .get(&auth_user_id)
            .cloned()
            .unwrap_or_else(|| json!({ "user_id": auth_user_id, "username": "alice", "first_name": "Alice" }));
        return MockResponse::json(200, profile);
    }

    if method == "PUT" && (path == "/api/v1/auth/profile" || path == "/api/v1/users/me") {
        let entry = st
            .user_profiles
            .entry(auth_user_id)
            .or_insert_with(|| json!({ "user_id": auth_user_id }));
        if let Some(obj) = entry.as_object_mut() {
            for k in ["first_name", "last_name", "bio", "username", "phone"] {
                if let Some(v) = req.body_json.get(k) {
                    obj.insert(k.to_string(), v.clone());
                }
            }
        }
        return MockResponse::json(200, entry.clone());
    }

    if method == "GET" && path == "/api/v1/auth/sessions" {
        let list: Vec<Value> = st.sessions.values().cloned().collect();
        return MockResponse::json(200, json!({ "sessions": list }));
    }

    if method == "DELETE" && path == "/api/v1/auth/sessions/others" {
        let before = st.sessions.len();
        st.sessions
            .retain(|_, v| v["is_current"].as_bool().unwrap_or(false));
        let revoked = before.saturating_sub(st.sessions.len());
        return MockResponse::json(200, json!({ "ok": true, "revoked_count": revoked }));
    }

    if method == "DELETE" {
        if let Some(sid) = path.strip_prefix("/api/v1/auth/sessions/") {
            let sid = percent_decode(sid);
            if st.sessions.remove(&sid).is_some() {
                return MockResponse::json(200, json!({ "ok": true, "revoked_session_id": sid }));
            }
            return MockResponse::json(
                404,
                json!({ "error": "session_not_found", "message": "Session not found" }),
            );
        }
    }

    // ---- Chats & Pinning ----
    if method == "GET" && path == "/api/v1/chats" {
        let mut chats: Vec<Value> = st.chats.values().cloned().collect();
        chats.sort_by(|a, b| {
            let pa = a["is_pinned"].as_bool().unwrap_or(false);
            let pb = b["is_pinned"].as_bool().unwrap_or(false);
            pb.cmp(&pa).then_with(|| {
                let ta = a["last_message_date_ms"].as_i64().unwrap_or(0);
                let tb = b["last_message_date_ms"].as_i64().unwrap_or(0);
                tb.cmp(&ta)
            })
        });
        return MockResponse::json(200, json!({ "chats": chats, "total": chats.len() }));
    }

    if method == "POST" && (path == "/api/v1/chats/create" || path == "/api/v1/chats") {
        let title = req.body_json["title"].as_str().unwrap_or("").trim();
        if title.is_empty() {
            return MockResponse::json(
                400,
                json!({ "error": "invalid_title", "message": "Chat title is required" }),
            );
        }
        let cid = st.next_chat_id;
        st.next_chat_id += 1;
        let kind = req.body_json["kind"]
            .as_str()
            .or_else(|| req.body_json["chat_type"].as_str())
            .unwrap_or("group");
        let chat = json!({
            "chat_id": cid,
            "id": cid,
            "title": title,
            "kind": kind,
            "chat_type": 1,
            "is_group": kind == "group",
            "is_channel": kind == "channel",
            "is_bot": false,
            "is_pinned": false,
            "is_muted": false,
            "mute_until_ms": null,
            "is_archived": false,
            "unread_count": 0,
            "message_ttl_seconds": 0,
            "pts": st.pts,
            "last_message_date_ms": 1_700_005_000_000i64
        });
        st.chats.insert(cid, chat.clone());
        return MockResponse::json(201, chat);
    }

    // Chat sub-routes: /api/v1/chats/{id}/...
    if let Some(rest) = path.strip_prefix("/api/v1/chats/") {
        let parts: Vec<&str> = rest.split('/').collect();
        if let Some(cid) = parts.first().and_then(|s| s.parse::<i64>().ok()) {
            match (method, parts.as_slice()) {
                ("POST", [_, "pin"]) => {
                    let pinned = req.body_json["pinned"].as_bool().unwrap_or(true);
                    if let Some(chat) = st.chats.get_mut(&cid) {
                        chat["is_pinned"] = json!(pinned);
                        return MockResponse::json(
                            200,
                            json!({
                                "chat_id": cid,
                                "is_pinned": chat["is_pinned"],
                                "is_muted": chat["is_muted"],
                                "mute_until_ms": chat["mute_until_ms"],
                                "is_archived": chat["is_archived"]
                            }),
                        );
                    }
                    return MockResponse::json(404, json!({ "error": "chat_not_found" }));
                }
                ("POST", [_, "mute"]) => {
                    let muted = req.body_json["muted"].as_bool().unwrap_or(true);
                    let mute_until_ms = if muted {
                        req.body_json.get("mute_until_ms").cloned().unwrap_or(Value::Null)
                    } else {
                        Value::Null
                    };
                    if let Some(chat) = st.chats.get_mut(&cid) {
                        chat["is_muted"] = json!(muted);
                        chat["mute_until_ms"] = mute_until_ms.clone();
                        return MockResponse::json(
                            200,
                            json!({
                                "chat_id": cid,
                                "is_muted": muted,
                                "mute_until_ms": mute_until_ms
                            }),
                        );
                    }
                    return MockResponse::json(404, json!({ "error": "chat_not_found" }));
                }
                ("POST", [_, "archive"]) => {
                    let archived = req.body_json["archived"].as_bool().unwrap_or(true);
                    if let Some(chat) = st.chats.get_mut(&cid) {
                        chat["is_archived"] = json!(archived);
                        return MockResponse::json(
                            200,
                            json!({ "chat_id": cid, "is_archived": archived }),
                        );
                    }
                    return MockResponse::json(404, json!({ "error": "chat_not_found" }));
                }
                ("POST", [_, "read"]) => {
                    let max_id = req.body_json["max_id"].as_i64().unwrap_or(0);
                    if let Some(chat) = st.chats.get_mut(&cid) {
                        chat["unread_count"] = json!(0);
                        chat["read_inbox_max_id"] = json!(max_id);
                    }
                    st.pts += 1;
                    return MockResponse::json(
                        200,
                        json!({
                            "success": true,
                            "chat_id": cid,
                            "pts": st.pts,
                            "read_inbox_max_id": max_id,
                            "unread_count": 0
                        }),
                    );
                }
                ("POST", [_, "clear"]) => {
                    st.messages.entry(cid).or_default().clear();
                    return MockResponse::json(200, json!({ "ok": true, "chat_id": cid }));
                }
                ("POST", [_, "leave"]) => {
                    st.chats.remove(&cid);
                    st.pinned_messages.remove(&cid);
                    return MockResponse::json(200, json!({ "ok": true, "chat_id": cid }));
                }
                ("POST", [_, "ttl"]) => {
                    let ttl = req.body_json["ttl_seconds"].as_u64().unwrap_or(0);
                    if let Some(chat) = st.chats.get_mut(&cid) {
                        chat["message_ttl_seconds"] = json!(ttl);
                    }
                    return MockResponse::json(
                        200,
                        json!({ "ok": true, "chat_id": cid, "ttl_seconds": ttl }),
                    );
                }
                ("GET", [_, "statistics"]) => {
                    let msg_count = st.messages.get(&cid).map(Vec::len).unwrap_or(0);
                    let pin_count = st.pinned_messages.get(&cid).map(Vec::len).unwrap_or(0);
                    return MockResponse::json(
                        200,
                        json!({
                            "chat_id": cid,
                            "subscribers": 7,
                            "total_messages": msg_count,
                            "pinned_messages": pin_count
                        }),
                    );
                }
                ("GET", [_, "pins"]) => {
                    let ids = st.pinned_messages.get(&cid).cloned().unwrap_or_default();
                    let pins: Vec<Value> = ids
                        .iter()
                        .map(|mid| json!({ "message_id": mid, "chat_id": cid }))
                        .collect();
                    return MockResponse::json(
                        200,
                        json!({
                            "chat_id": cid,
                            "pinned_message_ids": ids,
                            "pins": pins
                        }),
                    );
                }
                ("POST", [_, "pins"]) => {
                    if st.non_admin_pin_chats.contains(&cid) {
                        return MockResponse::json(
                            403,
                            json!({
                                "error": "forbidden",
                                "message": "Admin rights required to pin messages"
                            }),
                        );
                    }
                    let mid = req.body_json["message_id"].as_i64().unwrap_or(0);
                    if mid <= 0 {
                        return MockResponse::json(
                            400,
                            json!({ "error": "invalid_message_id", "message": "message_id must be > 0" }),
                        );
                    }
                    let entry = st.pinned_messages.entry(cid).or_default();
                    if !entry.contains(&mid) {
                        entry.push(mid);
                    }
                    let ids = entry.clone();
                    return MockResponse::json(
                        200,
                        json!({ "chat_id": cid, "pinned_message_ids": ids }),
                    );
                }
                ("DELETE", [_, "pins"]) => {
                    let ids = if let Some(mid_str) = query.get("message_id") {
                        let mid = mid_str.parse::<i64>().unwrap_or(0);
                        let entry = st.pinned_messages.entry(cid).or_default();
                        entry.retain(|x| *x != mid);
                        entry.clone()
                    } else {
                        st.pinned_messages.insert(cid, Vec::new());
                        Vec::new()
                    };
                    return MockResponse::json(
                        200,
                        json!({ "chat_id": cid, "pinned_message_ids": ids }),
                    );
                }
                ("DELETE", [_, "pins", mid_s]) => {
                    let mid = mid_s.parse::<i64>().unwrap_or(0);
                    let entry = st.pinned_messages.entry(cid).or_default();
                    entry.retain(|x| *x != mid);
                    let ids = entry.clone();
                    return MockResponse::json(
                        200,
                        json!({ "chat_id": cid, "pinned_message_ids": ids }),
                    );
                }
                ("GET", [_, "messages"]) => {
                    let before_id = query.get("before_id").and_then(|s| s.parse::<i64>().ok());
                    let limit = query
                        .get("limit")
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(50);
                    let mut all = st.messages.get(&cid).cloned().unwrap_or_default();
                    all.sort_by_key(|m| m["id"].as_i64().unwrap_or(0));
                    if let Some(bid) = before_id {
                        all.retain(|m| m["id"].as_i64().unwrap_or(0) < bid);
                    }
                    let has_more = all.len() > limit;
                    if all.len() > limit {
                        let start = all.len() - limit;
                        all = all[start..].to_vec();
                    }
                    return MockResponse::json(
                        200,
                        json!({
                            "chat_id": cid,
                            "messages": all,
                            "has_more": has_more,
                            "pts": st.pts,
                            "current_pts": st.pts
                        }),
                    );
                }
                ("PUT", [_, "messages", mid_s]) => {
                    let mid = mid_s.parse::<i64>().unwrap_or(0);
                    let new_text = req.body_json["text"].as_str().unwrap_or("");
                    if let Some(list) = st.messages.get_mut(&cid) {
                        if let Some(msg) = list.iter_mut().find(|m| m["id"].as_i64() == Some(mid)) {
                            msg["text"] = json!(new_text);
                            msg["is_edited"] = json!(true);
                        }
                    }
                    st.pts += 1;
                    return MockResponse::json(
                        200,
                        json!({
                            "success": true,
                            "chat_id": cid,
                            "message_id": mid,
                            "pts": st.pts,
                            "text": new_text,
                            "edited_at_ms": 1_700_009_000_000i64
                        }),
                    );
                }
                ("DELETE", [_, "messages", mid_s]) => {
                    let mid = mid_s.parse::<i64>().unwrap_or(0);
                    if let Some(list) = st.messages.get_mut(&cid) {
                        list.retain(|m| m["id"].as_i64() != Some(mid));
                    }
                    if let Some(pins) = st.pinned_messages.get_mut(&cid) {
                        pins.retain(|p| *p != mid);
                    }
                    st.pts += 1;
                    return MockResponse::json(
                        200,
                        json!({
                            "success": true,
                            "chat_id": cid,
                            "deleted_ids": [mid],
                            "revoke": query.get("revoke").map(|v| v == "true").unwrap_or(true),
                            "pts": st.pts
                        }),
                    );
                }
                ("POST", [_, "messages", mid_s, "reactions"]) => {
                    let mid = mid_s.parse::<i64>().unwrap_or(0);
                    let emoji = req.body_json["emoji"].as_str().unwrap_or("👍");
                    let mut counts = json!({});
                    if let Some(list) = st.messages.get_mut(&cid) {
                        if let Some(msg) = list.iter_mut().find(|m| m["id"].as_i64() == Some(mid)) {
                            let map = msg
                                .as_object_mut()
                                .unwrap()
                                .entry("reactions")
                                .or_insert_with(|| json!({}));
                            if let Some(rmap) = map.as_object_mut() {
                                let cur = rmap.get(emoji).and_then(Value::as_i64).unwrap_or(0);
                                rmap.insert(emoji.to_string(), json!(cur + 1));
                                counts = Value::Object(rmap.clone());
                            }
                        }
                    }
                    st.pts += 1;
                    return MockResponse::json(
                        200,
                        json!({
                            "success": true,
                            "chat_id": cid,
                            "message_id": mid,
                            "pts": st.pts,
                            "reactions": counts,
                            "reactions_authoritative": true
                        }),
                    );
                }
                ("GET", [_, "draft"]) => {
                    let draft = st.drafts.get(&cid).cloned().unwrap_or(Value::Null);
                    return MockResponse::json(200, json!({ "chat_id": cid, "draft": draft }));
                }
                ("PUT", [_, "draft"]) => {
                    let draft = json!({
                        "chat_id": cid,
                        "text": req.body_json["text"].as_str().unwrap_or(""),
                        "reply_to_msg_id": req.body_json.get("reply_to_msg_id").cloned().unwrap_or(Value::Null),
                        "updated_at_ms": 1_700_008_000_000i64
                    });
                    st.drafts.insert(cid, draft.clone());
                    return MockResponse::json(200, json!({ "chat_id": cid, "draft": draft }));
                }
                ("DELETE", [_, "draft"]) => {
                    st.drafts.remove(&cid);
                    return MockResponse::json(200, json!({ "ok": true, "chat_id": cid }));
                }
                ("POST", [_, "polls"]) => {
                    let pid = format!("poll_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
                    let question = req.body_json["question"].as_str().unwrap_or("");
                    let options = req.body_json["options"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    let poll = json!({
                        "poll_id": pid,
                        "chat_id": cid,
                        "question": question,
                        "options": options,
                        "closed": false,
                        "votes": {}
                    });
                    st.polls.insert(pid.clone(), poll.clone());
                    return MockResponse::json(201, poll);
                }
                ("POST", [_, "callback"]) => {
                    let mid = req.body_json["message_id"].as_i64().unwrap_or(0);
                    let data = req.body_json["callback_data"].as_str().unwrap_or("");
                    return MockResponse::json(
                        200,
                        json!({
                            "ok": true,
                            "chat_id": cid,
                            "message_id": mid,
                            "toast": format!("Executed {data}"),
                            "updated_text": format!("Callback applied: {data}"),
                            "inline_keyboard": [[{ "text": "Done", "callback_data": "done" }]]
                        }),
                    );
                }
                _ => {}
            }
        }
    }

    // ---- Send Message ----
    if method == "POST" && path == "/api/v1/messages/send" {
        let cid = req.body_json["chat_id"].as_i64().unwrap_or(101);
        let text = req.body_json["text"].as_str().unwrap_or("").to_string();
        let reply_to = req.body_json.get("reply_to").and_then(Value::as_i64);
        let client_nonce = req.body_json.get("client_nonce").and_then(Value::as_str);

        if let Some(nonce) = client_nonce {
            if let Some(existing) = st
                .messages
                .get(&cid)
                .and_then(|list| list.iter().find(|m| m["client_nonce"].as_str() == Some(nonce)))
                .cloned()
            {
                return MockResponse::json(
                    200,
                    json!({
                        "message_id": existing["id"],
                        "id": existing["id"],
                        "chat_id": cid,
                        "text": existing["text"],
                        "reply_to": existing["reply_to"],
                        "duplicate": true,
                        "status": "sent",
                        "pts": st.pts
                    }),
                );
            }
        }

        let mid = st.next_msg_id;
        st.next_msg_id += 1;
        st.pts += 1;
        let msg = json!({
            "id": mid,
            "message_id": mid,
            "chat_id": cid,
            "sender_id": auth_user_id,
            "text": text,
            "reply_to": reply_to,
            "client_nonce": client_nonce,
            "date_ms": 1_700_006_000_000i64 + mid,
            "status": "sent",
            "reactions": {}
        });
        st.messages.entry(cid).or_default().push(msg.clone());
        return MockResponse::json(
            200,
            json!({
                "message_id": mid,
                "id": mid,
                "chat_id": cid,
                "sender_id": auth_user_id,
                "text": text,
                "reply_to": reply_to,
                "client_nonce": client_nonce,
                "duplicate": false,
                "status": "sent",
                "pts": st.pts
            }),
        );
    }

    // ---- Polls vote / close ----
    if method == "POST" && path.starts_with("/api/v1/polls/") {
        if let Some(rest) = path.strip_prefix("/api/v1/polls/") {
            let parts: Vec<&str> = rest.split('/').collect();
            if parts.len() == 2 {
                let pid = percent_decode(parts[0]);
                if let Some(poll) = st.polls.get_mut(&pid) {
                    if parts[1] == "vote" {
                        poll["votes"][auth_user_id.to_string()] =
                            req.body_json["option_ids"].clone();
                        return MockResponse::json(200, poll.clone());
                    } else if parts[1] == "close" {
                        poll["closed"] = json!(true);
                        return MockResponse::json(200, poll.clone());
                    }
                }
                return MockResponse::json(404, json!({ "error": "poll_not_found" }));
            }
        }
    }

    // ---- Folders ----
    if method == "GET" && path == "/api/v1/folders" {
        let list: Vec<Value> = st.folders.values().cloned().collect();
        return MockResponse::json(200, json!({ "folders": list }));
    }
    if method == "POST" && path == "/api/v1/folders" {
        let fid = st.next_folder_id;
        st.next_folder_id += 1;
        let mut f = req.body_json.clone();
        f["id"] = json!(fid);
        st.folders.insert(fid, f.clone());
        return MockResponse::json(201, f);
    }
    if let Some(fid_s) = path.strip_prefix("/api/v1/folders/") {
        if let Ok(fid) = fid_s.parse::<i64>() {
            if method == "PUT" {
                let mut f = req.body_json.clone();
                f["id"] = json!(fid);
                st.folders.insert(fid, f.clone());
                return MockResponse::json(200, f);
            } else if method == "DELETE" {
                st.folders.remove(&fid);
                return MockResponse::json(200, json!({ "ok": true, "deleted_id": fid }));
            }
        }
    }

    // ---- Contacts & Peer Resolve ----
    if method == "GET" && path == "/api/v1/contacts" {
        let list: Vec<Value> = st.contacts.values().cloned().collect();
        return MockResponse::json(200, json!({ "contacts": list }));
    }
    if method == "POST" && (path == "/api/v1/contacts/add" || path == "/api/v1/contacts") {
        let username = req.body_json["username"].as_str().unwrap_or("").trim();
        if st.missing_contact_usernames.contains(username) {
            return MockResponse::json(
                404,
                json!({ "error": "user_not_found", "message": format!("User @{username} not found") }),
            );
        }
        let uid = 500 + st.contacts.len() as i64;
        let contact = json!({
            "user_id": uid,
            "username": username,
            "first_name": req.body_json["first_name"].as_str().unwrap_or(username),
            "phone": req.body_json["phone"].as_str().unwrap_or("")
        });
        st.contacts.insert(uid, contact.clone());
        return MockResponse::json(201, contact);
    }
    if method == "DELETE" {
        if let Some(uid_s) = path.strip_prefix("/api/v1/contacts/") {
            if let Ok(uid) = uid_s.parse::<i64>() {
                st.contacts.remove(&uid);
                return MockResponse::json(200, json!({ "ok": true, "user_id": uid }));
            }
        }
    }
    if method == "GET" && path == "/api/v1/peers/resolve" {
        let u = query.get("username").cloned().unwrap_or_default();
        return MockResponse::json(200, json!({ "username": u, "user_id": 99, "kind": "user" }));
    }

    // ---- Media CAS Upload / Download ----
    if method == "POST" && path == "/api/v1/media/upload" {
        let b64 = req.body_json["data_base64"].as_str().unwrap_or("");
        let bytes = match base64::engine::general_purpose::STANDARD.decode(b64) {
            Ok(b) => b,
            Err(_) => {
                return MockResponse::json(
                    400,
                    json!({ "error": "invalid_base64", "message": "Invalid base64 payload" }),
                );
            }
        };
        let mime = req.body_json["mime_type"]
            .as_str()
            .unwrap_or("application/octet-stream")
            .to_string();
        let filename = req.body_json["filename"].as_str().map(ToString::to_string);
        let file_id = format!("med_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
        let size = bytes.len();
        st.media_blobs.insert(file_id.clone(), (mime.clone(), bytes, filename.clone()));
        return MockResponse::json(
            201,
            json!({
                "file_id": file_id,
                "mime_type": mime,
                "size_bytes": size,
                "filename": filename
            }),
        );
    }
    if method == "GET" {
        if let Some(file_id) = path.strip_prefix("/api/v1/media/") {
            let file_id = percent_decode(file_id);
            if let Some((mime, bytes, _)) = st.media_blobs.get(&file_id) {
                return MockResponse::binary(200, mime, bytes.clone());
            }
            return MockResponse::json(404, json!({ "error": "media_not_found" }));
        }
    }

    // ---- Search & PTS Sync Difference ----
    if method == "GET" && path == "/api/v1/search/messages" {
        let q = query.get("q").cloned().unwrap_or_default().to_ascii_lowercase();
        let mut hits = Vec::new();
        for msgs in st.messages.values() {
            for m in msgs {
                if m["text"]
                    .as_str()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .contains(&q)
                {
                    hits.push(m.clone());
                }
            }
        }
        return MockResponse::json(200, json!({ "messages": hits, "total": hits.len() }));
    }

    if (method == "POST" || method == "GET") && path == "/api/v1/sync/difference" {
        let old_pts = req.body_json["pts"].as_i64().unwrap_or(0);
        st.pts = st.pts.max(old_pts) + 2;
        let synced_msg = json!({
            "id": st.next_msg_id,
            "chat_id": 101,
            "sender_id": 99,
            "text": "Synced PTS delta message",
            "date_ms": 1_700_010_000_000i64
        });
        st.next_msg_id += 1;
        return MockResponse::json(
            200,
            json!({
                "new_pts": st.pts,
                "current_pts": st.pts,
                "has_more": false,
                "snapshot_required": false,
                "messages": [synced_msg],
                "updates": []
            }),
        );
    }

    // ---- Bots & Coding Agents ----
    if method == "GET" && path == "/api/v1/bots/my" {
        let list: Vec<Value> = st.my_bots.values().cloned().collect();
        return MockResponse::json(200, json!({ "bots": list, "total": list.len() }));
    }
    if method == "POST" && path == "/api/v1/bots" {
        let username = req.body_json["username"].as_str().unwrap_or("");
        if !is_valid_bot_username(username) {
            return MockResponse::json(
                400,
                json!({
                    "error": "invalid_bot_username",
                    "message": "Bot username must start with a letter, be 5-32 [A-Za-z0-9_], and end with 'bot'"
                }),
            );
        }
        let display_name = req.body_json["display_name"].as_str().unwrap_or(username);
        let bid = st.next_bot_user_id;
        st.next_bot_user_id += 1;
        let bot_token = format!("srv_bot_token_{bid}_real_secret");
        let bot = json!({
            "bot_user_id": bid,
            "username": username,
            "display_name": display_name,
            "bot_token": bot_token,
            "execution_state": "idle",
            "avatar_shape": req.body_json["avatar_shape"].as_str().unwrap_or("circle"),
            "avatar_color": req.body_json["avatar_color"].as_str().unwrap_or("#007AFF"),
            "companion_chat_id": 7000 + bid,
            "ai_persona": req.body_json["ai_persona"].as_str().unwrap_or("Private persona")
        });
        st.my_bots.insert(bid, bot.clone());
        st.bot_owners.insert(username.to_ascii_lowercase(), auth_user_id);
        return MockResponse::json(201, bot);
    }
    if method == "GET" && path == "/api/v1/fabushi-bots" {
        let list: Vec<Value> = st.fabushi_bots.values().cloned().collect();
        return MockResponse::json(200, json!({ "bots": list, "total": list.len() }));
    }
    if method == "POST" && path == "/api/v1/fabushi-bots" {
        let display_name = req.body_json["display_name"].as_str().unwrap_or("Fabushi AI");
        let username = req.body_json["username"]
            .as_str()
            .unwrap_or("fabushi_ai_bot");
        let bot_id = format!("fbot_{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
        let bid = st.next_bot_user_id;
        st.next_bot_user_id += 1;
        let bot = json!({
            "id": bot_id,
            "bot_id": bot_id,
            "bot_user_id": bid,
            "display_name": display_name,
            "username": username,
            "bot_token": format!("srv_fbot_{bid}_secret"),
            "companion_chat_id": 8000 + bid,
            "execution_state": "idle",
            "avatar_shape": req.body_json["avatar_shape"].as_str().unwrap_or("squircle"),
            "avatar_color": req.body_json["avatar_color"].as_str().unwrap_or("#5856D6"),
            "preferred_model": req.body_json["preferred_model"].as_str().unwrap_or("gpt-4o"),
            "approval_mode": req.body_json["approval_mode"].as_str().unwrap_or("ask")
        });
        st.fabushi_bots.insert(bot_id.clone(), bot.clone());
        st.bot_owners.insert(bot_id.clone(), auth_user_id);
        st.bot_owners.insert(username.to_ascii_lowercase(), auth_user_id);
        return MockResponse::json(201, bot);
    }
    if let Some(bot_id) = path.strip_prefix("/api/v1/fabushi-bots/") {
        let bot_id = percent_decode(bot_id);
        if method == "GET" {
            if let Some(bot) = st.fabushi_bots.get(&bot_id) {
                return MockResponse::json(200, bot.clone());
            }
            return MockResponse::json(404, json!({ "error": "bot_not_found" }));
        }
        if method == "PATCH" {
            let owner = st.bot_owners.get(&bot_id).copied();
            if owner.is_some() && owner != Some(auth_user_id) {
                return MockResponse::json(
                    403,
                    json!({ "error": "forbidden", "message": "Only the bot owner can patch this bot" }),
                );
            }
            if let Some(bot) = st.fabushi_bots.get_mut(&bot_id) {
                if let Some(obj) = bot.as_object_mut() {
                    if let Some(patch_obj) = req.body_json.as_object() {
                        for (k, v) in patch_obj {
                            obj.insert(k.clone(), v.clone());
                        }
                    }
                }
                return MockResponse::json(200, bot.clone());
            }
            return MockResponse::json(404, json!({ "error": "bot_not_found" }));
        }
    }
    if method == "GET" {
        if let Some(target) = path.strip_prefix("/api/v1/bots/profile/") {
            let target_key = percent_decode(target).to_ascii_lowercase();
            let found = st
                .my_bots
                .values()
                .find(|b| {
                    b["username"]
                        .as_str()
                        .map(|u| u.eq_ignore_ascii_case(&target_key))
                        .unwrap_or(false)
                })
                .cloned();
            if let Some(mut bot) = found {
                let is_owner = st.bot_owners.get(&target_key).copied() == Some(auth_user_id);
                if !is_owner {
                    if let Some(obj) = bot.as_object_mut() {
                        obj.remove("ai_persona");
                    }
                }
                return MockResponse::json(200, bot);
            }
            return MockResponse::json(404, json!({ "error": "bot_not_found" }));
        }
    }
    if method == "POST" && path == "/api/v1/botfather/nl-create" {
        let prompt = req.body_json["prompt"].as_str().unwrap_or("");
        let bid = st.next_bot_user_id;
        st.next_bot_user_id += 1;
        return MockResponse::json(
            201,
            json!({
                "bot_user_id": bid,
                "username": format!("nl_helper_{bid}_bot"),
                "display_name": "NL Created Bot",
                "bot_token": format!("srv_nl_token_{bid}"),
                "companion_chat_id": 8500 + bid,
                "prompt": prompt
            }),
        );
    }
    if method == "POST" && path == "/api/v1/coding-agents/probe" {
        return MockResponse::json(
            200,
            json!({
                "device_id": req.headers.get("x-device-id").cloned().unwrap_or_default(),
                "providers": st.coding_agent_probes
            }),
        );
    }
    if let Some(rest) = path.strip_prefix("/api/v1/bots/") {
        if let Some(bot_id) = rest.strip_suffix("/computer") {
            let bot_id = percent_decode(bot_id);
            if method == "GET" {
                let cfg = st.bot_computers.get(&bot_id).cloned().unwrap_or_else(|| {
                    json!({
                        "bot_id": bot_id,
                        "bound_node_id": null,
                        "allow_cross_device_control": false
                    })
                });
                return MockResponse::json(200, cfg);
            } else if method == "PUT" {
                let mut cfg = req.body_json.clone();
                cfg["bot_id"] = json!(bot_id);
                if cfg["clear_bound_node"].as_bool() == Some(true) {
                    cfg["bound_node_id"] = Value::Null;
                }
                st.bot_computers.insert(bot_id.clone(), cfg.clone());
                return MockResponse::json(200, cfg);
            }
        }
    }

    // ---- Remote Control, Computers, Assist, Sessions, MCP Grants ----
    if method == "GET" && path == "/api/v1/computers" {
        let list: Vec<Value> = st.computers.values().cloned().collect();
        return MockResponse::json(200, json!({ "computers": list }));
    }
    if method == "POST" && path == "/api/v1/computers/cloud" {
        let host = req.body_json["host"].as_str().unwrap_or("").trim();
        if host.is_empty() {
            return MockResponse::json(
                400,
                json!({ "error": "invalid_host", "message": "Cloud computer host is required" }),
            );
        }
        let node_id = req.body_json["node_id"]
            .as_str()
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("cloud:{auth_user_id}:prod-1"));
        let has_secret = req.body_json["secret"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        let node = json!({
            "node_id": node_id,
            "display_name": req.body_json["display_name"].as_str().unwrap_or("Cloud Box"),
            "kind": "cloud_ssh",
            "host": host,
            "port": req.body_json["port"].as_u64().unwrap_or(22),
            "username": req.body_json["username"].as_str().unwrap_or("root"),
            "os_name": "Ubuntu 24.04",
            "resolution": "1920x1080",
            "online": true,
            "run_policy": "allow_all",
            "has_saved_credential": has_secret
        });
        st.computers.insert(node_id.clone(), node.clone());
        return MockResponse::json(200, node);
    }
    if method == "POST" && path == "/api/v1/computers/local/register" {
        let device_id = req.body_json["device_id"].as_str().unwrap_or("android-dev-1");
        let node_id = format!("local:{device_id}");
        let node = json!({
            "node_id": node_id,
            "device_id": device_id,
            "display_name": req.body_json["display_name"].as_str().unwrap_or("Fabushi Android"),
            "os_name": req.body_json["os_name"].as_str().unwrap_or("Android"),
            "resolution": req.body_json["resolution"].as_str().unwrap_or("1080x2400"),
            "online": true,
            "run_policy": "ask"
        });
        st.computers.insert(node_id.clone(), node.clone());
        return MockResponse::json(200, node);
    }
    if method == "GET" && path == "/api/v1/computers/local/poll" {
        return MockResponse::json(200, json!({ "commands": st.local_commands }));
    }
    if method == "POST" && path == "/api/v1/computers/exec" {
        let node_id = req.body_json["node_id"].as_str().unwrap_or("");
        if let Some(node) = st.computers.get(node_id) {
            if node["run_policy"].as_str() == Some("disabled") {
                return MockResponse::json(
                    403,
                    json!({
                        "error": "policy_disabled",
                        "message": "Remote execution is disabled for this computer"
                    }),
                );
            }
        }
        let op = &req.body_json["operation"];
        let cmd_str = op["command"].as_str().unwrap_or(op["op"].as_str().unwrap_or("op"));
        return MockResponse::json(
            200,
            json!({
                "ok": true,
                "node_id": node_id,
                "exit_code": 0,
                "stdout": format!("REAL_SERVER_OUTPUT:{cmd_str}"),
                "stderr": "",
                "duration_ms": 42
            }),
        );
    }
    if let Some(rest) = path.strip_prefix("/api/v1/computers/commands/") {
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() == 2 {
            let cmd_id = percent_decode(parts[0]);
            if method == "POST" && parts[1] == "approve" {
                let approved = req.body_json["approved"].as_bool().unwrap_or(true);
                return MockResponse::json(
                    200,
                    json!({ "ok": true, "command_id": cmd_id, "approved": approved }),
                );
            }
            if method == "POST" && parts[1] == "complete" {
                st.local_commands
                    .retain(|c| c["command_id"].as_str() != Some(cmd_id.as_str()));
                return MockResponse::json(
                    200,
                    json!({
                        "ok": true,
                        "command_id": cmd_id,
                        "stdout": req.body_json["stdout"],
                        "exit_code": req.body_json["exit_code"]
                    }),
                );
            }
        }
    }
    if let Some(rest) = path.strip_prefix("/api/v1/computers/") {
        if let Some(node_id) = rest.strip_suffix("/policy") {
            if method == "PATCH" {
                let node_id = percent_decode(node_id);
                if let Some(node) = st.computers.get_mut(&node_id) {
                    if let Some(rp) = req.body_json.get("run_policy") {
                        node["run_policy"] = rp.clone();
                    }
                    if let Some(dn) = req.body_json.get("display_name") {
                        node["display_name"] = dn.clone();
                    }
                    return MockResponse::json(200, node.clone());
                }
                return MockResponse::json(404, json!({ "error": "computer_not_found" }));
            }
        } else if let Some(node_id) = rest.strip_suffix("/test") {
            if method == "POST" {
                let node_id = percent_decode(node_id);
                if let Some(override_resp) = st.computer_test_override.clone() {
                    if override_resp["ok"].as_bool() == Some(false) {
                        if let Some(node) = st.computers.get_mut(&node_id) {
                            node["online"] = json!(false);
                        }
                    }
                    return MockResponse::json(200, override_resp);
                }
                return MockResponse::json(
                    200,
                    json!({
                        "ok": true,
                        "node_id": node_id,
                        "latency_ms": 23,
                        "stdout": "SSH_OK"
                    }),
                );
            }
        } else if method == "DELETE" && !rest.contains('/') {
            let node_id = percent_decode(rest);
            st.computers.remove(&node_id);
            return MockResponse::json(200, json!({ "ok": true, "deleted_node_id": node_id }));
        }
    }

    // ---- Remote Assist (Spec §8.4 RemoteAssistConfigDto) ----
    if method == "GET" && path == "/api/v1/remote/assist/info" {
        return MockResponse::json(
            200,
            json!({
                "assist_id": st.assist_id,
                "allow_assist": st.assist_allow,
                "code_mode": st.assist_code_mode,
                "current_code": st.assist_code,
                "expires_at_ms": Value::Null
            }),
        );
    }
    if method == "POST" && path == "/api/v1/remote/assist/allow" {
        st.assist_allow = req.body_json["allow_assist"].as_bool().unwrap_or(false);
        return MockResponse::json(
            200,
            json!({
                "assist_id": st.assist_id,
                "allow_assist": st.assist_allow,
                "code_mode": st.assist_code_mode,
                "current_code": st.assist_code,
                "expires_at_ms": Value::Null
            }),
        );
    }
    if method == "POST" && path == "/api/v1/remote/assist/reset-code" {
        st.assist_code = "Reset99A".to_string();
        st.assist_code_mode = "temporary".to_string();
        return MockResponse::json(
            200,
            json!({
                "assist_id": st.assist_id,
                "allow_assist": st.assist_allow,
                "code_mode": st.assist_code_mode,
                "current_code": st.assist_code,
                "expires_at_ms": Value::Null
            }),
        );
    }
    if method == "POST" && path == "/api/v1/remote/assist/set-code" {
        let code = req.body_json["code"].as_str().unwrap_or("");
        if !is_valid_assist_code(code) {
            return MockResponse::json(
                400,
                json!({
                    "error": "invalid_access_code",
                    "message": "Access code must be 8-16 ASCII alphanumeric characters containing both letters and digits"
                }),
            );
        }
        st.assist_code = code.to_string();
        st.assist_code_mode = "custom".to_string();
        return MockResponse::json(
            200,
            json!({
                "assist_id": st.assist_id,
                "allow_assist": st.assist_allow,
                "code_mode": st.assist_code_mode,
                "current_code": st.assist_code,
                "expires_at_ms": Value::Null
            }),
        );
    }
    if method == "POST" && path == "/api/v1/remote/assist/start" {
        let assist_id = req.body_json["assist_id"].as_str().unwrap_or("");
        let access_code = req.body_json["access_code"].as_str().unwrap_or("");
        if assist_id != st.assist_id {
            return MockResponse::json(
                404,
                json!({ "error": "assist_id_not_found", "message": "Unknown assist device ID" }),
            );
        }
        if !st.assist_allow {
            return MockResponse::json(
                403,
                json!({ "error": "assist_disabled", "message": "Remote assist is disabled on target" }),
            );
        }
        if access_code != st.assist_code {
            return MockResponse::json(
                403,
                json!({ "error": "invalid_access_code", "message": "Incorrect access code" }),
            );
        }
        let sid = format!("rsess_{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
        let relay_tok = format!("relay_tok_{}", uuid::Uuid::new_v4().simple());
        st.remote_sessions.insert(
            sid.clone(),
            json!({
                "session_id": sid,
                "assist_id": assist_id,
                "requester_name": req.body_json["requester_name"].as_str().unwrap_or("Android"),
                "status": "active"
            }),
        );
        return MockResponse::json(
            200,
            json!({
                "session_id": sid,
                "temp_relay_token": relay_tok,
                "assist_id": assist_id
            }),
        );
    }

    // ---- Remote Sessions & WebRTC Signal ----
    if method == "GET" && path == "/api/v1/remote/sessions" {
        let list: Vec<Value> = st.remote_sessions.values().cloned().collect();
        return MockResponse::json(200, json!({ "sessions": list }));
    }
    if method == "POST" && path == "/api/v1/remote/sessions/disconnect" {
        let sid = req.body_json["session_id"].as_str().unwrap_or("");
        st.remote_sessions.remove(sid);
        return MockResponse::json(200, json!({ "ok": true, "disconnected": sid }));
    }
    if method == "POST" && path == "/api/v1/remote/sessions/disconnect-all" {
        let n = st.remote_sessions.len();
        st.remote_sessions.clear();
        return MockResponse::json(200, json!({ "ok": true, "disconnected_count": n }));
    }
    if method == "POST" && path == "/api/v1/remote/webrtc/signal" {
        return MockResponse::json(200, json!({ "ok": true, "relayed": true }));
    }

    // ---- OAuth 2.1 MCP Grants & Audit ----
    if method == "GET" && path == "/api/v1/mcp/grants" {
        let list: Vec<Value> = st.mcp_grants.values().cloned().collect();
        return MockResponse::json(200, json!({ "grants": list }));
    }
    if method == "DELETE" {
        if let Some(fam_id) = path.strip_prefix("/api/v1/mcp/grants/") {
            let fam_id = percent_decode(fam_id);
            st.mcp_grants.remove(&fam_id);
            return MockResponse::json(200, json!({ "ok": true, "revoked_family_id": fam_id }));
        }
    }
    if method == "GET" && path == "/api/v1/mcp/audit" {
        return MockResponse::json(200, json!({ "entries": [] }));
    }

    MockResponse::json(
        404,
        json!({
            "error": "not_found",
            "message": format!("No mock route for {method} {path}")
        }),
    )
}

fn configure_and_login(host: &MahayanaAndroidHost, server: &MockGpuiTelegramServer, user: &str) -> String {
    host.dispatch("telegram.server.configureEndpoint", &json!({ "endpoint": server.url() }))
        .expect("configureEndpoint");
    let login = host
        .dispatch(
            "telegram.server.login",
            &json!({ "username": user, "password": "secret-password-123" }),
        )
        .expect("login");
    login["token"].as_str().expect("login token").to_string()
}

// ============================================================================
// TIER 1 — FEATURE COVERAGE (27 Rust Tests)
// ============================================================================

#[test]
fn e2e_t1_a1() {
    let dir = TempDirGuard::new("t1_a1");
    let host = dir.prod_host();
    assert!(!host.is_test_mode());

    // 1. Parity metadata
    let info = host.dispatch("feature.info", &json!({})).expect("feature.info");
    assert_eq!(info["desktopParityVersion"], "1.6.6");
    assert_eq!(info["desktopParityCommit"], "6ca754c");
    assert_eq!(info["sqliteWal"], true);

    // 2. Zero fake cloud nodes (only local-android-mika standby)
    let nodes = host.dispatch("computer.listNodes", &json!({})).expect("computer.listNodes");
    let node_arr = nodes["nodes"].as_array().expect("nodes array");
    assert_eq!(node_arr.len(), 1);
    assert_eq!(node_arr[0]["nodeId"], "local-android-mika");
    assert_eq!(node_arr[0]["status"], "standby");
    assert!(!node_arr.iter().any(|n| {
        matches!(
            n["nodeId"].as_str(),
            Some("cloud-ubuntu-cua") | Some("cloud-ephemeral-sandbox")
        )
    }));

    // 3. Hermes Kanban/Approvals/Skills start completely empty
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

    // 4. Zero fake stories & accounts
    assert!(host.dispatch("telegram.messaging.listStories", &json!({})).unwrap()["stories"]
        .as_array()
        .unwrap()
        .is_empty());
    let accounts = host.dispatch("telegram.auth.accountsList", &json!({})).unwrap();
    assert!(accounts["accounts"].as_array().unwrap().is_empty());

    // 5. Zero fake starter conversations/actors/messages/folders in production mode
    let access = host.dispatch("feature.messaging.access.issue", &json!({})).unwrap();
    let actor_id = access["actorId"].as_str().unwrap();
    let sync = host
        .dispatch(
            "feature.messaging.execute",
            &json!({
                "requestId": "req-cold-sync",
                "envelope": {
                    "protocolVersion": 2,
                    "context": {
                        "requestId": "req-cold-sync",
                        "deviceId": "android:e2e",
                        "actorId": actor_id,
                        "sessionId": "s1",
                        "sentAtMs": 1_700_000_000_000i64
                    },
                    "command": { "type": "sync", "cursor": null, "limit": 500 }
                }
            }),
        )
        .unwrap();
    let batch = &sync["envelopes"][0]["event"];
    assert!(batch["conversations"].as_array().unwrap().is_empty());
    assert!(batch["actors"].as_array().unwrap().is_empty());
    assert!(batch["messages"].as_array().unwrap().is_empty());
    assert!(batch["folders"].as_array().unwrap().is_empty());

    // 6. Wallet zero fake collectibles
    let _w = host.dispatch("wallet.createWallet", &json!({})).unwrap();
    assert!(host.dispatch("wallet.collectibles", &json!({})).unwrap()["collectibles"]
        .as_array()
        .unwrap()
        .is_empty());

    // 7. Zero hardcoded surface bots in feature.execute(bot.list)
    host.dispatch(
        "feature.execute",
        &json!({ "command": { "type": "bot.list", "requestId": "b-cold" } }),
    )
    .unwrap();
    let ev = host.dispatch("feature.receive", &json!({})).unwrap();
    assert_eq!(ev["type"], "bot.listed");
    assert!(ev["bots"].as_array().unwrap().is_empty());

    // 8. Production test-auth bypasses fail closed
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

    // 9. C-ABI handle lifecycle smoke check
    let c_dir = CString::new(dir.path().to_str().unwrap()).unwrap();
    let handle = unsafe { mahayana_app_host_create(c_dir.as_ptr()) };
    assert!(handle > 0, "C-ABI create handle must be > 0");
    let c_req = CString::new(json!({ "method": "feature.info", "params": {} }).to_string()).unwrap();
    let c_resp_ptr = unsafe { mahayana_app_host_dispatch_with_handle(handle, c_req.as_ptr()) };
    assert!(!c_resp_ptr.is_null());
    let c_resp_str = unsafe { CStr::from_ptr(c_resp_ptr).to_str().unwrap().to_string() };
    unsafe { mahayana_app_host_free_string(c_resp_ptr) };
    mahayana_app_host_destroy(handle);
    let parsed: Value = serde_json::from_str(&c_resp_str).unwrap();
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["desktopParityVersion"], "1.6.6");
}

#[test]
fn e2e_t1_a3() {
    let dir = TempDirGuard::new("t1_a3");
    let server = MockGpuiTelegramServer::start();

    let token = {
        let host = dir.prod_host();
        let tok = configure_and_login(&host, &server, "alice");
        assert!(tok.len() >= 24);

        let me = host.dispatch("telegram.server.me", &json!({})).expect("me");
        assert_eq!(me["user_id"], 42);
        assert_eq!(me["username"], "alice");

        // Verify deviceAgentSession bridge uses the authenticated Telegram token
        let bridge = host
            .dispatch("feature.auth.deviceAgentSession", &json!({}))
            .expect("bridged deviceAgentSession");
        assert_eq!(bridge["accessToken"], tok);
        assert_eq!(bridge["source"], "telegram_server_session");
        tok
    };

    // Cold reopen on identical SQLite WAL directory (test_mode = false)
    let reopened = dir.prod_host();
    let session = reopened.dispatch("telegram.server.session", &json!({})).expect("session");
    assert_eq!(session["authenticated"], true);
    assert_eq!(session["userId"], 42);
    assert_eq!(session["token"], token);
    assert_eq!(session["endpoint"], server.url());

    // Verify server recorded Authorization: Bearer <token> on /api/v1/auth/me
    let reqs = server.recorded_requests();
    let me_req = reqs
        .iter()
        .find(|r| r.method == "GET" && r.path == "/api/v1/auth/me")
        .expect("recorded GET /api/v1/auth/me");
    assert_eq!(
        me_req.authorization.as_deref(),
        Some(format!("Bearer {token}").as_str())
    );
}

#[test]
fn e2e_t1_a4() {
    let dir = TempDirGuard::new("t1_a4");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();

    host.dispatch("telegram.server.configureEndpoint", &json!({ "endpoint": server.url() }))
        .unwrap();

    // 1. Direct register
    let reg = host
        .dispatch(
            "telegram.server.register",
            &json!({ "username": "reg_alice", "password": "pw123456", "firstName": "RegAlice" }),
        )
        .expect("register");
    assert!(reg["token"].as_str().unwrap().len() >= 24);
    host.dispatch("telegram.server.logout", &json!({})).unwrap();

    // 2. Email send-code & email register
    let sent = host
        .dispatch("telegram.server.sendEmailCode", &json!({ "email": "alice@example.com" }))
        .expect("sendEmailCode");
    assert_eq!(sent["sent"], true);
    let ereg = host
        .dispatch(
            "telegram.server.registerEmail",
            &json!({
                "email": "alice@example.com",
                "code": "654321",
                "username": "email_alice",
                "password": "pw123456",
                "firstName": "EmailAlice"
            }),
        )
        .expect("registerEmail");
    assert!(ereg["token"].as_str().unwrap().len() >= 24);
    host.dispatch("telegram.server.logout", &json!({})).unwrap();

    // 3. Alipay start -> poll pending (must NOT persist session) -> poll completed (persists)
    let ali = host.dispatch("telegram.server.alipayStart", &json!({})).expect("alipayStart");
    let attempt_id = ali["attempt_id"].as_str().expect("attempt_id");

    let poll1 = host
        .dispatch("telegram.server.alipayPoll", &json!({ "attemptId": attempt_id }))
        .expect("alipayPoll pending");
    assert_eq!(poll1["status"], "pending");
    let sess_pending = host.dispatch("telegram.server.session", &json!({})).unwrap();
    assert_eq!(sess_pending["authenticated"], false);

    let poll2 = host
        .dispatch("telegram.server.alipayPoll", &json!({ "attemptId": attempt_id }))
        .expect("alipayPoll completed");
    assert_eq!(poll2["status"], "completed");
    let sess_done = host.dispatch("telegram.server.session", &json!({})).unwrap();
    assert_eq!(sess_done["authenticated"], true);
    assert_eq!(sess_done["username"], "alice_alipay");

    // Verify exact routes recorded
    let reqs = server.recorded_requests();
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == "/api/v1/auth/register"));
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == "/api/v1/auth/email/send-code"));
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == "/api/v1/auth/email/register"));
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == "/api/v1/auth/alipay/start"));
    assert!(reqs
        .iter()
        .any(|r| r.method == "GET" && r.path == format!("/api/v1/auth/alipay/poll/{attempt_id}")));
}

#[test]
fn e2e_t1_a5() {
    let dir = TempDirGuard::new("t1_a5");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    let s1 = host.dispatch("telegram.server.listSessions", &json!({})).expect("listSessions");
    let arr1 = s1["sessions"].as_array().unwrap();
    assert_eq!(arr1.len(), 2);

    host.dispatch("telegram.server.revokeSession", &json!({ "sessionId": "sess_desktop_mac" }))
        .expect("revokeSession");

    let s2 = host.dispatch("telegram.server.listSessions", &json!({})).expect("listSessions after");
    let arr2 = s2["sessions"].as_array().unwrap();
    assert_eq!(arr2.len(), 1);
    assert_eq!(arr2[0]["session_id"], "sess_current_android");

    let upd = host
        .dispatch(
            "telegram.server.updateProfile",
            &json!({ "firstName": "Alice Updated", "bio": "Apple iOS Parity" }),
        )
        .expect("updateProfile");
    assert_eq!(upd["first_name"], "Alice Updated");
    assert_eq!(upd["bio"], "Apple iOS Parity");

    let session = host.dispatch("telegram.server.session", &json!({})).unwrap();
    assert_eq!(session["firstName"], "Alice Updated");

    let reqs = server.recorded_requests();
    assert!(reqs.iter().any(|r| r.method == "GET" && r.path == "/api/v1/auth/sessions"));
    assert!(reqs
        .iter()
        .any(|r| r.method == "DELETE" && r.path == "/api/v1/auth/sessions/sess_desktop_mac"));
    assert!(reqs.iter().any(|r| r.method == "PUT" && r.path == "/api/v1/auth/profile"));
}

#[test]
fn e2e_t1_a6() {
    let dir = TempDirGuard::new("t1_a6");
    let server = MockGpuiTelegramServer::start();
    server.with_state(|s| s.seed_chat(101, "Alpha", true, 1_700_000_000_000));

    {
        let host = dir.prod_host();
        configure_and_login(&host, &server, "alice");
        host.dispatch("telegram.server.listChats", &json!({})).unwrap();
        assert_eq!(
            host.dispatch("telegram.server.session", &json!({})).unwrap()["authenticated"],
            true
        );

        let lo = host.dispatch("telegram.server.logout", &json!({})).expect("logout");
        assert_eq!(lo["loggedOut"], true);

        let sess = host.dispatch("telegram.server.session", &json!({})).unwrap();
        assert_eq!(sess["authenticated"], false);
        assert_eq!(sess["token"], "");
        assert!(sess["userId"].is_null());

        // Bridged deviceAgentSession & authed RPCs must fail closed
        assert!(host.dispatch("feature.auth.deviceAgentSession", &json!({})).is_err());
        assert!(host
            .dispatch("telegram.server.listChats", &json!({}))
            .unwrap_err()
            .contains("AUTH_REQUIRED"));
    }

    // Cold restart must remain signed out
    let reopened = dir.prod_host();
    let sess = reopened.dispatch("telegram.server.session", &json!({})).unwrap();
    assert_eq!(sess["authenticated"], false);
}

#[test]
fn e2e_t1_b1() {
    let dir = TempDirGuard::new("t1_b1");
    let server = MockGpuiTelegramServer::start();
    server.with_state(|s| {
        s.seed_chat(101, "Pinned Originally", true, 1_700_000_100_000);
        s.seed_chat(102, "Older Chat", false, 1_700_000_050_000);
        s.seed_chat(103, "Newest Chat", false, 1_700_000_200_000);
    });

    {
        let host = dir.prod_host();
        configure_and_login(&host, &server, "alice");
        let initial = host.dispatch("telegram.server.listChats", &json!({})).unwrap();
        let ids: Vec<i64> = initial["chats"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["chat_id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, vec![101, 103, 102]);

        // Pin 102 and unpin 101
        let p1 = host
            .dispatch("telegram.server.pinChat", &json!({ "chatId": 102, "pinned": true }))
            .expect("pinChat 102 true");
        assert_eq!(p1["is_pinned"], true);

        let p2 = host
            .dispatch("telegram.server.pinChat", &json!({ "chatId": 101, "pinned": false }))
            .expect("pinChat 101 false");
        assert_eq!(p2["is_pinned"], false);
    }

    // Cold reopen and verify server-persisted ordering survives restart without reverting
    let reopened = dir.prod_host();
    let after = reopened.dispatch("telegram.server.listChats", &json!({})).unwrap();
    let arr = after["chats"].as_array().unwrap();
    let ids_after: Vec<i64> = arr.iter().map(|c| c["chat_id"].as_i64().unwrap()).collect();
    // 102 is pinned (first), then non-pinned sorted strictly DESC by last_message_date_ms: 103 (200k) > 101 (100k)
    assert_eq!(ids_after, vec![102, 103, 101]);
    assert_eq!(arr[0]["is_pinned"], true);
    assert_eq!(arr[1]["is_pinned"], false);
    assert_eq!(arr[2]["is_pinned"], false);

    let reqs = server.recorded_requests();
    assert!(reqs.iter().any(|r| {
        r.method == "POST"
            && r.path == "/api/v1/chats/102/pin"
            && !r.body_raw.is_empty()
            && r.body_json["pinned"] == true
    }));
    assert!(reqs.iter().any(|r| {
        r.method == "POST" && r.path == "/api/v1/chats/101/pin" && r.body_json["pinned"] == false
    }));
}

#[test]
fn e2e_t1_b2() {
    let dir = TempDirGuard::new("t1_b2");
    let server = MockGpuiTelegramServer::start();
    server.with_state(|s| s.seed_chat(101, "Pins Room", false, 1_700_000_000_000));
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    let p1 = host
        .dispatch("telegram.server.pinMessage", &json!({ "chatId": 101, "messageId": 501 }))
        .expect("pinMessage 501");
    assert_eq!(p1["pinned_message_ids"], json!([501]));

    let p2 = host
        .dispatch("telegram.server.pinMessage", &json!({ "chatId": 101, "messageId": 502 }))
        .expect("pinMessage 502");
    assert_eq!(p2["pinned_message_ids"], json!([501, 502]));

    let listed = host
        .dispatch("telegram.server.listPinnedMessages", &json!({ "chatId": 101 }))
        .expect("listPinnedMessages");
    assert_eq!(listed["pinned_message_ids"], json!([501, 502]));

    // Unpin single message via DELETE /api/v1/chats/101/pins/501
    let u1 = host
        .dispatch("telegram.server.unpinMessage", &json!({ "chatId": 101, "messageId": 501 }))
        .expect("unpinMessage 501");
    assert_eq!(u1["pinned_message_ids"], json!([502]));

    // Unpin all via DELETE /api/v1/chats/101/pins
    let u_all = host
        .dispatch("telegram.server.unpinMessage", &json!({ "chatId": 101 }))
        .expect("unpinMessage all");
    assert_eq!(u_all["pinned_message_ids"], json!([]));

    let reqs = server.recorded_requests();
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == "/api/v1/chats/101/pins"));
    assert!(reqs.iter().any(|r| r.method == "GET" && r.path == "/api/v1/chats/101/pins"));
    assert!(reqs.iter().any(|r| r.method == "DELETE" && r.path == "/api/v1/chats/101/pins/501"));
    assert!(reqs.iter().any(|r| r.method == "DELETE" && r.path == "/api/v1/chats/101/pins"));
}

#[test]
fn e2e_t1_b3() {
    let dir = TempDirGuard::new("t1_b3");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    let created = host
        .dispatch(
            "telegram.server.createChat",
            &json!({ "title": "Lifecycle Group", "kind": "group" }),
        )
        .expect("createChat");
    let cid = created["chat_id"].as_i64().unwrap();

    let muted = host
        .dispatch(
            "telegram.server.muteChat",
            &json!({ "chatId": cid, "muted": true, "muteUntilMs": 1_800_000_000_000i64 }),
        )
        .expect("muteChat");
    assert_eq!(muted["is_muted"], true);
    assert_eq!(muted["mute_until_ms"], 1_800_000_000_000i64);

    let unmuted = host
        .dispatch("telegram.server.muteChat", &json!({ "chatId": cid, "muted": false }))
        .expect("unmuteChat");
    assert_eq!(unmuted["is_muted"], false);
    assert!(unmuted["mute_until_ms"].is_null());

    let archived = host
        .dispatch("telegram.server.archiveChat", &json!({ "chatId": cid, "archived": true }))
        .expect("archiveChat");
    assert_eq!(archived["is_archived"], true);

    let read = host
        .dispatch("telegram.server.markRead", &json!({ "chatId": cid, "maxId": 500 }))
        .expect("markRead");
    assert_eq!(read["unread_count"], 0);

    let ttl = host
        .dispatch("telegram.server.setChatTtl", &json!({ "chatId": cid, "ttlSeconds": 86400 }))
        .expect("setChatTtl");
    assert_eq!(ttl["ttl_seconds"], 86400);

    host.dispatch("telegram.server.clearChat", &json!({ "chatId": cid })).expect("clearChat");
    host.dispatch("telegram.server.leaveChat", &json!({ "chatId": cid })).expect("leaveChat");

    let reqs = server.recorded_requests();
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == "/api/v1/chats/create"));
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == format!("/api/v1/chats/{cid}/mute")));
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == format!("/api/v1/chats/{cid}/archive")));
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == format!("/api/v1/chats/{cid}/read")));
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == format!("/api/v1/chats/{cid}/ttl")));
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == format!("/api/v1/chats/{cid}/clear")));
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == format!("/api/v1/chats/{cid}/leave")));
}

#[test]
fn e2e_t1_b4() {
    let dir = TempDirGuard::new("t1_b4");
    let server = MockGpuiTelegramServer::start();
    server.with_state(|s| s.seed_chat(101, "Sync Room", false, 1_700_000_000_000));
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    // 1. Drafts (PUT / GET / DELETE)
    host.dispatch(
        "telegram.server.setDraft",
        &json!({ "chatId": 101, "text": "Draft text 中文", "replyToId": 501 }),
    )
    .expect("setDraft");
    let got_draft = host.dispatch("telegram.server.getDraft", &json!({ "chatId": 101 })).expect("getDraft");
    assert_eq!(got_draft["draft"]["text"], "Draft text 中文");
    assert_eq!(got_draft["draft"]["reply_to_msg_id"], 501);
    host.dispatch("telegram.server.clearDraft", &json!({ "chatId": 101 })).expect("clearDraft");
    let cleared = host.dispatch("telegram.server.getDraft", &json!({ "chatId": 101 })).unwrap();
    assert!(cleared["draft"].is_null());

    // 2. Folders (POST create -> PUT update -> GET -> DELETE)
    let f_new = host
        .dispatch("telegram.server.saveFolder", &json!({ "title": "Work", "includedChatIds": [101] }))
        .expect("saveFolder create");
    let fid = f_new["id"].as_i64().unwrap();
    host.dispatch("telegram.server.saveFolder", &json!({ "id": fid, "title": "Work Updated" }))
        .expect("saveFolder update");
    let folders = host.dispatch("telegram.server.listFolders", &json!({})).expect("listFolders");
    assert_eq!(folders["folders"][0]["title"], "Work Updated");
    host.dispatch("telegram.server.deleteFolder", &json!({ "folderId": fid })).expect("deleteFolder");
    assert!(host.dispatch("telegram.server.listFolders", &json!({})).unwrap()["folders"]
        .as_array()
        .unwrap()
        .is_empty());

    // 3. Contacts (POST /api/v1/contacts/add -> GET -> DELETE)
    let added = host
        .dispatch(
            "telegram.server.addContact",
            &json!({ "username": "@dharma_peer", "firstName": "Dharma" }),
        )
        .expect("addContact");
    let uid = added["user_id"].as_i64().unwrap();
    let contacts = host.dispatch("telegram.server.listContacts", &json!({})).expect("listContacts");
    assert_eq!(contacts["contacts"].as_array().unwrap().len(), 1);
    host.dispatch("telegram.server.deleteContact", &json!({ "userId": uid })).expect("deleteContact");
    assert!(host.dispatch("telegram.server.listContacts", &json!({})).unwrap()["contacts"]
        .as_array()
        .unwrap()
        .is_empty());

    // 4. PTS syncDifference (POST /api/v1/sync/difference)
    let diff = host
        .dispatch("telegram.server.syncDifference", &json!({ "liveNetwork": true, "pts": 10 }))
        .expect("syncDifference");
    assert_eq!(diff["onlineReachable"], true);
    assert!(diff["new_pts"].as_i64().unwrap() > 10);
    assert!(diff["ingestedMessages"].as_i64().unwrap() >= 1);

    let reqs = server.recorded_requests();
    assert!(reqs.iter().any(|r| {
        r.method == "PUT" && r.path == "/api/v1/chats/101/draft" && r.body_json["reply_to_msg_id"] == 501
    }));
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == "/api/v1/contacts/add"));
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == "/api/v1/sync/difference"));
}

#[test]
fn e2e_t1_b5() {
    let dir = TempDirGuard::new("t1_b5");
    let server = MockGpuiTelegramServer::start();
    server.with_state(|s| s.seed_chat(101, "Msg Room", false, 1_700_000_000_000));
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    // 1. Send with replyToId + clientNonce -> wire key must be "reply_to" & "client_nonce"
    let sent = host
        .dispatch(
            "telegram.server.sendMessage",
            &json!({
                "chatId": 101,
                "text": "Hello 世界",
                "replyToId": 500,
                "clientNonce": "nonce-e2e-b5"
            }),
        )
        .expect("sendMessage");
    let mid = sent["message_id"].as_i64().unwrap();
    assert_eq!(sent["status"], "sent");
    assert_eq!(sent["reply_to"], 500);

    // 2. Edit (PUT)
    let edited = host
        .dispatch(
            "telegram.server.editMessage",
            &json!({ "chatId": 101, "messageId": mid, "text": "Edited 世界" }),
        )
        .expect("editMessage");
    assert_eq!(edited["text"], "Edited 世界");

    // 3. Reaction (POST)
    let react = host
        .dispatch(
            "telegram.server.sendReaction",
            &json!({ "chatId": 101, "messageId": mid, "emoji": "🔥" }),
        )
        .expect("sendReaction");
    assert_eq!(react["reactions"]["🔥"], 1);

    // 4. Delete (DELETE)
    let del = host
        .dispatch(
            "telegram.server.deleteMessage",
            &json!({ "chatId": 101, "messageId": mid, "revoke": true }),
        )
        .expect("deleteMessage");
    assert_eq!(del["deleted_ids"], json!([mid]));

    // 5. Polls (create + vote + close)
    let poll = host
        .dispatch(
            "telegram.server.createPoll",
            &json!({ "chatId": 101, "question": "UI Style?", "options": ["Apple iOS", "Other"] }),
        )
        .expect("createPoll");
    let pid = poll["poll_id"].as_str().unwrap();
    let voted = host
        .dispatch("telegram.server.votePoll", &json!({ "pollId": pid, "optionIds": [0] }))
        .expect("votePoll");
    assert_eq!(voted["votes"]["42"], json!([0]));
    let closed = host
        .dispatch("telegram.server.closePoll", &json!({ "pollId": pid }))
        .expect("closePoll");
    assert_eq!(closed["closed"], true);

    let reqs = server.recorded_requests();
    assert!(reqs.iter().any(|r| {
        r.method == "POST"
            && r.path == "/api/v1/messages/send"
            && r.body_json["reply_to"] == 500
            && r.body_json["client_nonce"] == "nonce-e2e-b5"
    }));
    assert!(reqs.iter().any(|r| r.method == "PUT" && r.path == format!("/api/v1/chats/101/messages/{mid}")));
    assert!(reqs
        .iter()
        .any(|r| r.method == "POST" && r.path == format!("/api/v1/chats/101/messages/{mid}/reactions")));
    assert!(reqs
        .iter()
        .any(|r| r.method == "DELETE" && r.path == format!("/api/v1/chats/101/messages/{mid}")));
}

#[test]
fn e2e_t1_b6() {
    let dir = TempDirGuard::new("t1_b6");
    let server = MockGpuiTelegramServer::start();
    server.with_state(|s| {
        s.seed_chat(101, "Pagination Room", false, 1_700_000_000_000);
        let list = s.messages.entry(101).or_default();
        for id in 1..=10i64 {
            list.push(json!({ "id": id, "chat_id": 101, "text": format!("msg-{id}") }));
        }
    });
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    // 1. Paginated getMessages(?before_id=8&limit=3) -> returns [5, 6, 7], has_more=true
    let page = host
        .dispatch(
            "telegram.server.getMessages",
            &json!({ "chatId": 101, "beforeId": 8, "limit": 3 }),
        )
        .expect("getMessages paginated");
    let ids: Vec<i64> = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, vec![5, 6, 7]);
    assert_eq!(page["has_more"], true);

    // 2. Upload & download exact arbitrary binary bytes (non-UTF8 PNG-like bytes)
    let raw_bytes: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0xFF, 0x42];
    let b64_in = base64::engine::general_purpose::STANDARD.encode(&raw_bytes);
    let uploaded = host
        .dispatch(
            "telegram.server.uploadMedia",
            &json!({
                "dataBase64": b64_in,
                "filename": "photo.png",
                "mimeType": "image/png",
                "chatId": 101
            }),
        )
        .expect("uploadMedia");
    let file_id = uploaded["file_id"].as_str().unwrap();

    let downloaded = host
        .dispatch("telegram.server.downloadMedia", &json!({ "fileId": file_id }))
        .expect("downloadMedia");
    let b64_out = downloaded["dataBase64"].as_str().expect("dataBase64");
    let roundtrip = base64::engine::general_purpose::STANDARD.decode(b64_out).unwrap();
    assert_eq!(roundtrip, raw_bytes);
    assert_eq!(downloaded["sizeBytes"], raw_bytes.len());

    let reqs = server.recorded_requests();
    assert!(reqs
        .iter()
        .any(|r| r.method == "GET" && r.path == "/api/v1/chats/101/messages" && r.query.contains("before_id=8")));
}

#[test]
fn e2e_t1_c1() {
    let dir = TempDirGuard::new("t1_c1");
    let server = MockGpuiTelegramServer::start();
    server.with_state(|s| {
        s.my_bots.insert(
            9010,
            json!({
                "bot_user_id": 9010,
                "username": "real_server_bot",
                "display_name": "Real Server Bot",
                "execution_state": "idle",
                "avatar_shape": "hexagon",
                "avatar_color": "#34C759",
                "companion_chat_id": 7010
            }),
        );
        s.fabushi_bots.insert(
            "fbot_real_1".to_string(),
            json!({
                "id": "fbot_real_1",
                "bot_user_id": 9011,
                "username": "real_fabushi_bot",
                "display_name": "Real Fabushi Bot",
                "execution_state": "thinking",
                "avatar_shape": "squircle",
                "avatar_color": "#007AFF",
                "companion_chat_id": 8011
            }),
        );
    });

    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    let my = host.dispatch("telegram.server.listMyBots", &json!({})).expect("listMyBots");
    assert_eq!(my["total"], 1);
    assert_eq!(my["bots"][0]["username"], "real_server_bot");
    assert_eq!(my["bots"][0]["execution_state"], "idle");
    assert_eq!(my["bots"][0]["companion_chat_id"], 7010);

    let fb = host
        .dispatch("telegram.server.listFabushiBots", &json!({}))
        .expect("listFabushiBots");
    assert_eq!(fb["total"], 1);
    assert_eq!(fb["bots"][0]["avatar_shape"], "squircle");
    assert_eq!(fb["bots"][0]["execution_state"], "thinking");
}

#[test]
fn e2e_t1_c2() {
    let dir = TempDirGuard::new("t1_c2");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    let b1 = host
        .dispatch(
            "telegram.server.createBot",
            &json!({ "displayName": "Assistant", "username": "helper_alpha_bot" }),
        )
        .expect("createBot");
    let tok1 = b1["bot_token"].as_str().unwrap();
    assert!(tok1.starts_with("srv_bot_token_"));
    assert!(!tok1.starts_with("tg_bot_"));

    let b2 = host
        .dispatch(
            "telegram.server.createFabushiBot",
            &json!({ "displayName": "Coder AI", "username": "coder_ai_bot", "avatarShape": "diamond" }),
        )
        .expect("createFabushiBot");
    assert!(b2["companion_chat_id"].as_i64().unwrap() > 0);
    assert!(!b2["bot_token"].as_str().unwrap().starts_with("tg_bot_"));

    let b3 = host
        .dispatch(
            "telegram.server.botfatherNlCreate",
            &json!({ "prompt": "Create a daily reminder bot", "avatarColor": "#FF9500" }),
        )
        .expect("botfatherNlCreate");
    assert!(b3["bot_token"].as_str().unwrap().starts_with("srv_nl_token_"));

    let reqs = server.recorded_requests();
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == "/api/v1/bots"));
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == "/api/v1/fabushi-bots"));
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == "/api/v1/botfather/nl-create"));
}

#[test]
fn e2e_t1_c3() {
    let dir = TempDirGuard::new("t1_c3");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    let created = host
        .dispatch(
            "telegram.server.createFabushiBot",
            &json!({ "displayName": "Muse Bot", "username": "muse_patch_bot" }),
        )
        .unwrap();
    let bot_id = created["id"].as_str().unwrap();

    let patched = host
        .dispatch(
            "telegram.server.patchFabushiBot",
            &json!({
                "botId": bot_id,
                "avatarShape": "star",
                "avatarColor": "#AF52DE",
                "preferredModel": "claude-sonnet-4",
                "approvalMode": "auto"
            }),
        )
        .expect("patchFabushiBot");
    assert_eq!(patched["avatar_shape"], "star");
    assert_eq!(patched["avatar_color"], "#AF52DE");
    assert_eq!(patched["preferred_model"], "claude-sonnet-4");
    assert_eq!(patched["approval_mode"], "auto");

    // Re-fetch confirms persisted server state
    let fetched = host
        .dispatch("telegram.server.getFabushiBot", &json!({ "botId": bot_id }))
        .expect("getFabushiBot");
    assert_eq!(fetched["avatar_shape"], "star");
    assert_eq!(fetched["avatar_color"], "#AF52DE");

    let reqs = server.recorded_requests();
    assert!(reqs
        .iter()
        .any(|r| r.method == "PATCH" && r.path == format!("/api/v1/fabushi-bots/{bot_id}")));
}

#[test]
fn e2e_t1_c4() {
    let dir = TempDirGuard::new("t1_c4");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    host.dispatch(
        "telegram.server.createBot",
        &json!({ "displayName": "Inspector Bot", "username": "inspector_e2e_bot", "aiPersona": "Owner Secret" }),
    )
    .unwrap();

    let prof = host
        .dispatch("telegram.server.getBotProfile", &json!({ "target": "@inspector_e2e_bot" }))
        .expect("getBotProfile");
    assert_eq!(prof["username"], "inspector_e2e_bot");
    assert_eq!(prof["ai_persona"], "Owner Secret");

    let cb = host
        .dispatch(
            "telegram.server.sendCallbackQuery",
            &json!({ "chatId": 101, "messageId": 555, "callbackData": "approve:42" }),
        )
        .expect("sendCallbackQuery");
    assert_eq!(cb["toast"], "Executed approve:42");
    assert_eq!(cb["updated_text"], "Callback applied: approve:42");
    assert!(cb["inline_keyboard"].as_array().is_some());

    let reqs = server.recorded_requests();
    assert!(reqs
        .iter()
        .any(|r| r.method == "GET" && r.path == "/api/v1/bots/profile/inspector_e2e_bot"));
    assert!(reqs
        .iter()
        .any(|r| r.method == "POST" && r.path == "/api/v1/chats/101/callback"));
}

#[test]
fn e2e_t1_c5() {
    let dir = TempDirGuard::new("t1_c5");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    let probe = host
        .dispatch(
            "telegram.server.probeCodingAgents",
            &json!({ "deviceId": "android-e2e-1", "timeoutMs": 3000 }),
        )
        .expect("probeCodingAgents");
    assert_eq!(probe["providers"].as_array().unwrap().len(), 6);

    let set_res = host
        .dispatch(
            "telegram.server.setBotComputer",
            &json!({
                "botId": "fbot_42",
                "boundNodeId": "cloud:42:prod-1",
                "allowCrossDeviceControl": true
            }),
        )
        .expect("setBotComputer");
    assert_eq!(set_res["bound_node_id"], "cloud:42:prod-1");

    let get_res = host
        .dispatch("telegram.server.getBotComputer", &json!({ "botId": "fbot_42" }))
        .expect("getBotComputer");
    assert_eq!(get_res["bound_node_id"], "cloud:42:prod-1");

    let reqs = server.recorded_requests();
    assert!(reqs.iter().any(|r| {
        r.method == "POST"
            && r.path == "/api/v1/coding-agents/probe"
            && r.headers.get("x-device-id").map(String::as_str) == Some("android-e2e-1")
    }));
    assert!(reqs.iter().any(|r| r.method == "PUT" && r.path == "/api/v1/bots/fbot_42/computer"));
    assert!(reqs.iter().any(|r| r.method == "GET" && r.path == "/api/v1/bots/fbot_42/computer"));
}

#[test]
fn e2e_t1_d1() {
    let dir = TempDirGuard::new("t1_d1");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    let initial = host.dispatch("telegram.server.listComputers", &json!({})).expect("listComputers");
    assert!(initial["computers"].as_array().unwrap().is_empty());

    let upserted = host
        .dispatch(
            "telegram.server.upsertCloudComputer",
            &json!({
                "nodeId": "cloud:42:prod-1",
                "displayName": "Prod Ubuntu",
                "host": "10.0.0.42",
                "port": 2222,
                "username": "ubuntu",
                "secret": "super-secret-ssh-key"
            }),
        )
        .expect("upsertCloudComputer");
    assert_eq!(upserted["node_id"], "cloud:42:prod-1");
    assert_eq!(upserted["has_saved_credential"], true);
    assert!(upserted.get("secret").is_none(), "raw SSH secret must never be echoed");

    let patched = host
        .dispatch(
            "telegram.server.patchComputerPolicy",
            &json!({ "nodeId": "cloud:42:prod-1", "runPolicy": "ask" }),
        )
        .expect("patchComputerPolicy");
    assert_eq!(patched["run_policy"], "ask");

    let tested = host
        .dispatch("telegram.server.testComputer", &json!({ "nodeId": "cloud:42:prod-1" }))
        .expect("testComputer");
    assert_eq!(tested["ok"], true);

    host.dispatch("telegram.server.deleteComputer", &json!({ "nodeId": "cloud:42:prod-1" }))
        .expect("deleteComputer");
    let after = host.dispatch("telegram.server.listComputers", &json!({})).unwrap();
    assert!(after["computers"].as_array().unwrap().is_empty());

    let reqs = server.recorded_requests();
    assert!(reqs
        .iter()
        .any(|r| r.method == "PATCH" && r.path == "/api/v1/computers/cloud:42:prod-1/policy"));
    assert!(reqs
        .iter()
        .any(|r| r.method == "POST" && r.path == "/api/v1/computers/cloud:42:prod-1/test"));
    assert!(reqs
        .iter()
        .any(|r| r.method == "DELETE" && r.path == "/api/v1/computers/cloud:42:prod-1"));
}

#[test]
fn e2e_t1_d2() {
    let dir = TempDirGuard::new("t1_d2");
    let server = MockGpuiTelegramServer::start();
    server.with_state(|s| {
        s.local_commands.push(json!({
            "command_id": "cmd_local_77",
            "operation": { "op": "capture_screen" }
        }));
    });
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    host.dispatch(
        "telegram.server.upsertCloudComputer",
        &json!({ "nodeId": "cloud:42:prod-1", "displayName": "Exec Node", "host": "10.0.0.42" }),
    )
    .unwrap();

    // 1. Real execComputer with tagged RemoteComputerOp
    let exec_res = host
        .dispatch(
            "telegram.server.execComputer",
            &json!({
                "nodeId": "cloud:42:prod-1",
                "operation": { "op": "shell_exec", "command": "uname -a" }
            }),
        )
        .expect("execComputer");
    assert_eq!(exec_res["exit_code"], 0);
    assert_eq!(exec_res["stdout"], "REAL_SERVER_OUTPUT:uname -a");
    assert_ne!(exec_res["stdout"], "执行完成 (0)");

    // 2. Register local computer (POST) + Poll local commands (GET!) + Approve + Complete
    let reg = host
        .dispatch(
            "telegram.server.registerLocalComputer",
            &json!({ "deviceId": "android-mika-01", "displayName": "Pixel 9" }),
        )
        .expect("registerLocalComputer");
    assert_eq!(reg["device_id"], "android-mika-01");

    let polled = host
        .dispatch("telegram.server.pollLocalCommands", &json!({ "deviceId": "android-mika-01" }))
        .expect("pollLocalCommands");
    assert_eq!(polled["commands"][0]["command_id"], "cmd_local_77");

    let approved = host
        .dispatch(
            "telegram.server.approveComputerCommand",
            &json!({ "commandId": "cmd_local_77", "approved": true }),
        )
        .expect("approveComputerCommand");
    assert_eq!(approved["approved"], true);

    let completed = host
        .dispatch(
            "telegram.server.completeComputerCommand",
            &json!({
                "commandId": "cmd_local_77",
                "ok": true,
                "exitCode": 0,
                "stdout": "frame_captured"
            }),
        )
        .expect("completeComputerCommand");
    assert_eq!(completed["stdout"], "frame_captured");

    let reqs = server.recorded_requests();
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == "/api/v1/computers/local/register"));
    assert!(reqs.iter().any(|r| {
        r.method == "GET"
            && r.path == "/api/v1/computers/local/poll"
            && r.headers.get("x-device-id").map(String::as_str) == Some("android-mika-01")
    }));
}

#[test]
fn e2e_t1_d3() {
    let dir = TempDirGuard::new("t1_d3");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    let info = host.dispatch("telegram.server.assistInfo", &json!({})).expect("assistInfo");
    assert_eq!(info["assist_id"], "123456789");
    assert_eq!(info["allow_assist"], false);
    assert_eq!(info["code_mode"], "temporary");
    assert_eq!(info["current_code"], "InitCode1");

    let allow = host
        .dispatch("telegram.server.assistAllow", &json!({ "allowAssist": true }))
        .expect("assistAllow");
    assert_eq!(allow["allow_assist"], true);

    let reset = host.dispatch("telegram.server.assistResetCode", &json!({})).expect("assistResetCode");
    assert_eq!(reset["current_code"], "Reset99A");
    assert_eq!(reset["code_mode"], "temporary");

    let set_c = host
        .dispatch("telegram.server.assistSetCode", &json!({ "code": "Abc12345" }))
        .expect("assistSetCode");
    assert_eq!(set_c["current_code"], "Abc12345");
    assert_eq!(set_c["code_mode"], "custom");

    let started = host
        .dispatch(
            "telegram.server.assistStart",
            &json!({ "assistId": "123456789", "accessCode": "Abc12345" }),
        )
        .expect("assistStart");
    assert!(started["session_id"].as_str().unwrap().starts_with("rsess_"));
    assert!(started["temp_relay_token"].as_str().unwrap().starts_with("relay_tok_"));
}

#[test]
fn e2e_t1_d4() {
    let dir = TempDirGuard::new("t1_d4");
    let server = MockGpuiTelegramServer::start();
    server.with_state(|s| {
        s.remote_sessions.insert("rsess_1".into(), json!({ "session_id": "rsess_1", "status": "active" }));
        s.remote_sessions.insert("rsess_2".into(), json!({ "session_id": "rsess_2", "status": "active" }));
        s.mcp_grants.insert(
            "fam_oauth_1".into(),
            json!({ "family_id": "fam_oauth_1", "client_name": "Claude Desktop", "scopes": ["computer:read"] }),
        );
    });
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    let rs1 = host.dispatch("telegram.server.listRemoteSessions", &json!({})).unwrap();
    assert_eq!(rs1["sessions"].as_array().unwrap().len(), 2);

    host.dispatch("telegram.server.disconnectRemoteSession", &json!({ "sessionId": "rsess_1" }))
        .unwrap();
    let rs2 = host.dispatch("telegram.server.listRemoteSessions", &json!({})).unwrap();
    assert_eq!(rs2["sessions"].as_array().unwrap().len(), 1);

    host.dispatch("telegram.server.disconnectAllRemoteSessions", &json!({})).unwrap();
    let rs3 = host.dispatch("telegram.server.listRemoteSessions", &json!({})).unwrap();
    assert!(rs3["sessions"].as_array().unwrap().is_empty());

    let mg1 = host.dispatch("telegram.server.listMcpGrants", &json!({})).unwrap();
    assert_eq!(mg1["grants"].as_array().unwrap().len(), 1);

    host.dispatch("telegram.server.revokeMcpGrant", &json!({ "familyId": "fam_oauth_1" }))
        .unwrap();
    let mg2 = host.dispatch("telegram.server.listMcpGrants", &json!({})).unwrap();
    assert!(mg2["grants"].as_array().unwrap().is_empty());

    // Local fake OAuth grant minting MUST fail closed
    let err = host
        .dispatch("remote.issueMcpGrant", &json!({ "clientName": "Fake" }))
        .unwrap_err();
    assert!(err.contains("OAUTH_FLOW_REQUIRED"), "got: {err}");
}

#[test]
fn e2e_t1_e1() {
    let dir = TempDirGuard::new("t1_e1");
    let server = MockGpuiTelegramServer::start();

    {
        let host = dir.prod_host();
        // Default endpoint unified to desktop v1.6.6 IP
        let st = host.dispatch("telegram.server.status", &json!({})).unwrap();
        assert_eq!(st["endpoint"], "http://34.46.77.133:8080");

        // configureEndpoint
        let r1 = host
            .dispatch("telegram.server.configureEndpoint", &json!({ "endpoint": "http://127.0.0.1:9999" }))
            .unwrap();
        assert_eq!(r1["endpoint"], "http://127.0.0.1:9999");

        // setEndpoint alias
        let r2 = host
            .dispatch("telegram.server.setEndpoint", &json!({ "endpoint": server.url() }))
            .unwrap();
        assert_eq!(r2["endpoint"], server.url());

        // Login and flush outbox -> MUST hit /api/v1/messages/send (NOT /v1/messages/send)
        host.dispatch(
            "telegram.server.login",
            &json!({ "username": "alice", "password": "pw" }),
        )
        .unwrap();
        host.dispatch(
            "telegram.server.enqueueOutbox",
            &json!({ "payload": { "chat_id": 101, "text": "Outbox message" } }),
        )
        .unwrap();
        let flushed = host.dispatch("telegram.server.flushOutbox", &json!({})).expect("flushOutbox");
        assert_eq!(flushed["flushedCount"], 1);
        assert_eq!(flushed["pendingCount"], 0);
    }

    // Endpoint persists across cold restart
    let reopened = dir.prod_host();
    assert_eq!(
        reopened.dispatch("telegram.server.status", &json!({})).unwrap()["endpoint"],
        server.url()
    );

    let reqs = server.recorded_requests();
    assert!(reqs.iter().any(|r| r.method == "POST" && r.path == "/api/v1/messages/send"));
    assert!(!reqs.iter().any(|r| r.path == "/v1/messages/send"));
}

#[test]
fn e2e_t1_e2() {
    let dir = TempDirGuard::new("t1_e2");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();

    let actor = "human:account:owner";
    let conv_ev = host
        .dispatch(
            "feature.messaging.execute",
            &json!({
                "requestId": "req-ch1",
                "envelope": {
                    "protocolVersion": 2,
                    "context": {
                        "requestId": "req-ch1",
                        "deviceId": "android:e2e",
                        "actorId": actor,
                        "sessionId": "s1",
                        "sentAtMs": 1_700_000_000_000i64
                    },
                    "command": {
                        "type": "createConversation",
                        "conversation": {
                            "id": "conv_stats_e2e",
                            "kind": "channel",
                            "title": "Stats Channel",
                            "participants": [
                                { "actorId": actor, "role": "owner", "joinedAtMs": 1000 },
                                { "actorId": "human:account:sub2", "role": "member", "joinedAtMs": 1000 }
                            ]
                        }
                    }
                }
            }),
        )
        .unwrap();
    let conv_id = conv_ev["envelopes"][0]["event"]["conversation"]["id"].as_str().unwrap().to_string();

    let m1 = host
        .dispatch(
            "feature.messaging.execute",
            &json!({
                "requestId": "req-m1",
                "envelope": {
                    "protocolVersion": 2,
                    "context": {
                        "requestId": "req-m1",
                        "deviceId": "android:e2e",
                        "actorId": actor,
                        "sessionId": "s1",
                        "sentAtMs": 1_700_000_000_001i64
                    },
                    "command": {
                        "type": "sendMessage",
                        "conversationId": conv_id,
                        "content": { "type": "text", "data": { "text": { "text": "Post 1", "entities": [] } } }
                    }
                }
            }),
        )
        .unwrap();
    let m1_id = m1["envelopes"][0]["event"]["message"]["id"].as_str().unwrap().to_string();

    host.dispatch(
        "feature.messaging.execute",
        &json!({
            "requestId": "req-m2",
            "envelope": {
                "protocolVersion": 2,
                "context": {
                    "requestId": "req-m2",
                    "deviceId": "android:e2e",
                    "actorId": actor,
                    "sessionId": "s1",
                    "sentAtMs": 1_700_000_000_002i64
                },
                "command": {
                    "type": "sendMessage",
                    "conversationId": conv_id,
                    "content": { "type": "text", "data": { "text": { "text": "Post 2", "entities": [] } } }
                }
            }
        }),
    )
    .unwrap();

    host.dispatch(
        "feature.messaging.execute",
        &json!({
            "requestId": "req-pin",
            "envelope": {
                "protocolVersion": 2,
                "context": {
                    "requestId": "req-pin",
                    "deviceId": "android:e2e",
                    "actorId": actor,
                    "sessionId": "s1",
                    "sentAtMs": 1_700_000_000_003i64
                },
                "command": {
                    "type": "pinMessage",
                    "conversationId": conv_id,
                    "messageId": m1_id,
                    "pinned": true
                }
            }
        }),
    )
    .unwrap();

    let local_stats = host
        .dispatch("telegram.messaging.channelStats", &json!({ "conversationId": conv_id }))
        .expect("local channelStats must not crash on SQLite columns");
    assert_eq!(local_stats["subscribers"], 2);
    assert_eq!(local_stats["totalMessages"], 2);
    assert_eq!(local_stats["pinnedMessages"], 1);

    // Server channelStats hits GET /api/v1/chats/101/statistics
    configure_and_login(&host, &server, "alice");
    let srv_stats = host
        .dispatch("telegram.server.channelStats", &json!({ "chatId": 101 }))
        .expect("server channelStats");
    assert_eq!(srv_stats["subscribers"], 7);
    assert!(server
        .recorded_requests()
        .iter()
        .any(|r| r.method == "GET" && r.path == "/api/v1/chats/101/statistics"));
}

#[test]
fn e2e_t1_e3() {
    let dir = TempDirGuard::new("t1_e3");

    {
        let host = dir.prod_host();
        let up = host
            .dispatch(
                "magpie.upsertProvider",
                &json!({ "presetId": "openai", "apiKey": "sk-real-e2e-12345", "enabled": true }),
            )
            .expect("magpie.upsertProvider");
        assert_eq!(up["presetId"], "openai");
        assert_eq!(up["configuredForLive"], true);

        let uri_res = host
            .dispatch(
                "magpie.importUri",
                &json!({ "uri": "magpie://import?preset=deepseek&key=sk-ds-real-999" }),
            )
            .expect("magpie.importUri");
        assert_eq!(uri_res["provider"]["configuredForLive"], true);

        let env_res = host
            .dispatch(
                "magpie.importEnv",
                &json!({ "envText": "ANTHROPIC_API_KEY=sk-ant-real-777\n" }),
            )
            .expect("magpie.importEnv");
        assert!(env_res["importedCount"].as_u64().unwrap() >= 1);
    }

    // Cold restart verifies persistence & honesty of all other presets (both configuredForLive AND hasApiKey)
    let reopened = dir.prod_host();
    let list = reopened.dispatch("magpie.providers", &json!({})).expect("magpie.providers");
    assert_eq!(list["liveReady"], true);
    let providers = list["providers"].as_array().unwrap();
    let expected_configured: HashSet<&str> = ["openai", "deepseek", "anthropic"].into_iter().collect();
    for preset_id in &expected_configured {
        let p = providers
            .iter()
            .find(|p| p["presetId"].as_str() == Some(*preset_id))
            .unwrap_or_else(|| panic!("missing preset '{preset_id}' after cold restart"));
        assert_eq!(
            p["configuredForLive"], true,
            "preset '{preset_id}' must have configuredForLive == true after cold restart"
        );
        assert_eq!(
            p["hasApiKey"], true,
            "preset '{preset_id}' must have hasApiKey == true after cold restart"
        );
    }
    // Unconfigured presets remain strictly configuredForLive == false and hasApiKey == false
    for p in providers
        .iter()
        .filter(|p| !expected_configured.contains(p["presetId"].as_str().unwrap_or("")))
    {
        assert_eq!(p["configuredForLive"], false, "unconfigured preset {:?}", p["presetId"]);
        assert_eq!(p["hasApiKey"], false, "unconfigured preset {:?}", p["presetId"]);
    }
}

#[test]
fn e2e_t1_e4() {
    let dir = TempDirGuard::new("t1_e4");
    let host = dir.prod_host();

    let created = host
        .dispatch("wallet.createWallet", &json!({ "initialNanotons": 2_000_000_000i64 }))
        .expect("wallet.createWallet");
    let addr = created["address"].as_str().expect("real Ed25519 v4r2 address");
    assert!(!addr.is_empty());
    assert!(!addr.starts_with("UQBk9x_mahayana_v4r2_"));

    let status = host.dispatch("wallet.status", &json!({})).expect("wallet.status");
    assert_eq!(status["hasWallet"], true);
    assert_eq!(status["address"], addr);
    assert!(status["balanceTon"].as_str().is_some());
    assert_eq!(status["balanceNanotons"], 2_000_000_000i64);

    let sent = host
        .dispatch(
            "wallet.sendTon",
            &json!({
                "to": addr,
                "amountNanotons": 100_000_000u64,
                "comment": "e2e-transfer",
                "broadcast": false
            }),
        )
        .expect("wallet.sendTon");
    assert_eq!(sent["amountNanotons"], 100_000_000u64);
    assert!(sent["newBalanceNanotons"].as_i64().unwrap() < 2_000_000_000i64);
    assert_eq!(sent["seqno"], 1);
}

#[test]
fn e2e_t1_e5() {
    let dir = TempDirGuard::new("t1_e5");
    let secret = "E2E_PIN_9876_NEVER_PLAINTEXT";
    {
        let host = dir.prod_host();
        let set_res = host
            .dispatch("telegram.auth.passcodeSet", &json!({ "passcode": secret }))
            .expect("passcodeSet");
        assert_eq!(set_res["passcodeEnabled"], true);
        assert_eq!(set_res["kdf"], "Argon2id");
    }

    // Opaque-box on-disk scan after host drop (SQLite flushed/closed): plaintext passcode MUST NOT appear on disk!
    assert_no_plaintext_bytes_on_disk(dir.path(), secret);

    // Verify Argon2id hash survives cold restart
    let reopened = dir.prod_host();
    let ok_res = reopened
        .dispatch("telegram.auth.passcodeVerify", &json!({ "passcode": secret }))
        .expect("passcodeVerify right");
    assert_eq!(ok_res["valid"], true);

    let bad_res = reopened
        .dispatch("telegram.auth.passcodeVerify", &json!({ "passcode": "0000" }))
        .expect("passcodeVerify wrong");
    assert_eq!(bad_res["valid"], false);
}

#[test]
fn e2e_t1_e6() {
    let dir = TempDirGuard::new("t1_e6");
    let host = dir.prod_host();

    // 1. Hermes memoryUpdate + runSlashCommand + approvalsDecide (unknown rejected cleanly)
    let mem = host
        .dispatch(
            "hermes.memoryUpdate",
            &json!({ "target": "SOUL.md", "content": "# Sovereign Soul\nZero fake data." }),
        )
        .expect("hermes.memoryUpdate");
    assert_eq!(mem["target"], "SOUL.md");
    assert!(mem["content"].as_str().unwrap().contains("Zero fake data."));

    let slash = host
        .dispatch("hermes.runSlashCommand", &json!({ "input": "/help" }))
        .expect("hermes.runSlashCommand");
    assert_eq!(slash["command"], "/help");

    let apr_err = host
        .dispatch(
            "hermes.approvalsDecide",
            &json!({ "approvalId": "apr_missing_e2e", "decision": "denied" }),
        )
        .unwrap_err();
    assert!(apr_err.contains("not found"), "got: {apr_err}");

    // 2. Fabushi MiniApp bootstrap / spinPrayerWheel / recordPractice / aiDharmaChat
    let boot = host.dispatch("fabushiMiniapp.bootstrap", &json!({})).expect("bootstrap");
    assert!(boot["profile"]["total_transfer_count"].as_u64().is_some());

    let spin = host
        .dispatch("fabushiMiniapp.spinPrayerWheel", &json!({ "rotations": 12 }))
        .expect("spinPrayerWheel");
    assert_eq!(spin["rotationsAdded"], 12);

    let prac = host
        .dispatch(
            "fabushiMiniapp.recordPractice",
            &json!({ "durationSeconds": 180, "chantCount": 21 }),
        )
        .expect("recordPractice");
    assert_eq!(prac["durationSeconds"], 180);
    assert_eq!(prac["chantCount"], 21);

    // Unconfigured Magpie -> aiDharmaChat fails closed with PROVIDER_NOT_CONFIGURED
    let ai_err = host
        .dispatch("fabushiMiniapp.aiDharmaChat", &json!({ "prompt": "Explain mindfulness" }))
        .unwrap_err();
    assert!(ai_err.contains("PROVIDER_NOT_CONFIGURED"), "got: {ai_err}");
}

#[test]
fn e2e_t1_e7() {
    let dir = TempDirGuard::new("t1_e7");
    let host = dir.prod_host();

    // 1. E2EE createSecretChat + sendEncrypted
    let secret = host
        .dispatch(
            "telegram.e2ee.createSecretChat",
            &json!({ "peerId": "peer_42", "chatId": "sec_chat_42" }),
        )
        .expect("createSecretChat");
    assert_eq!(secret["chatId"], "sec_chat_42");
    assert_eq!(secret["peerId"], "peer_42");

    let enc = host
        .dispatch(
            "telegram.e2ee.sendEncrypted",
            &json!({ "chatId": "sec_chat_42", "plaintext": "Confidential Dharma message" }),
        )
        .expect("sendEncrypted");
    assert_eq!(enc["verifiedRoundtrip"], true);
    assert!(!enc["ciphertextBase64"].as_str().unwrap().is_empty());

    // 2. accountAdd + accountSwitch with expectedGeneration fence
    host.dispatch(
        "telegram.auth.accountAdd",
        &json!({ "accountId": "acc_1", "username": "alice", "displayName": "Alice" }),
    )
    .unwrap();
    let stale_err = host
        .dispatch(
            "telegram.auth.accountSwitch",
            &json!({ "accountId": "acc_1", "expectedGeneration": 999 }),
        )
        .unwrap_err();
    assert!(stale_err.contains("GenerationFence mismatch"), "got: {stale_err}");
    let sw = host
        .dispatch(
            "telegram.auth.accountSwitch",
            &json!({ "accountId": "acc_1", "expectedGeneration": 1 }),
        )
        .expect("accountSwitch");
    assert_eq!(sw["activeAccountId"], "acc_1");
    assert_eq!(sw["generation"], 2);

    // 3. createTopic with iconEmoji
    let topic = host
        .dispatch(
            "telegram.messaging.createTopic",
            &json!({ "conversationId": "conv_forum_1", "title": "Announcements", "iconEmoji": "📌" }),
        )
        .expect("createTopic");
    assert_eq!(topic["title"], "Announcements");
    assert_eq!(topic["iconEmoji"], "📌");

    // 4. mika.registerTaskEvent + taskHistory
    let ev = host
        .dispatch(
            "mika.registerTaskEvent",
            &json!({ "taskId": "task_99", "stepIndex": 2, "phase": "acting", "summary": "Clicked submit" }),
        )
        .expect("mika.registerTaskEvent");
    assert_eq!(ev["recorded"], true);
    let hist = host.dispatch("mika.taskHistory", &json!({})).expect("mika.taskHistory");
    assert_eq!(hist["events"][0]["taskId"], "task_99");
    assert_eq!(hist["events"][0]["stepIndex"], 2);

    // 5. Calls: startCall/endCall fail closed with CALL_TRANSPORT_UNAVAILABLE; vadFeedPcm succeeds
    assert!(host
        .dispatch("telegram.calls.startCall", &json!({ "peerId": "42" }))
        .unwrap_err()
        .contains("CALL_TRANSPORT_UNAVAILABLE"));
    assert!(host
        .dispatch("telegram.calls.endCall", &json!({ "callId": "c1" }))
        .unwrap_err()
        .contains("CALL_TRANSPORT_UNAVAILABLE"));
    let vad = host
        .dispatch(
            "telegram.calls.vadFeedPcm",
            &json!({ "pcmSamples": [0, 5000, -5000, 6000, -6000], "reset": true }),
        )
        .expect("vadFeedPcm");
    assert!(vad.get("framesEvaluated").is_some());
    assert!(vad.get("speechDetected").is_some());
}

// ============================================================================
// TIER 2 — BOUNDARY & CORNER CASES (22 Rust Tests)
// ============================================================================

#[test]
fn e2e_t2_a1() {
    let dir = TempDirGuard::new("t2_a1");
    let server = MockGpuiTelegramServer::start();
    server.with_state(|s| s.seed_chat(101, "Alpha", true, 1_700_000_000_000));

    {
        let host = dir.prod_host();
        configure_and_login(&host, &server, "alice");
        host.dispatch("telegram.server.listChats", &json!({})).unwrap();
        assert_eq!(
            host.dispatch("telegram.server.session", &json!({})).unwrap()["authenticated"],
            true
        );

        // Revoke token server-side -> /api/v1/auth/me returns 401 -> MUST wipe SQLite session & caches
        server.revoke_all_tokens();
        let err = host.dispatch("telegram.server.me", &json!({})).unwrap_err();
        assert!(err.contains("HTTP 401"), "expected HTTP 401, got: {err}");

        let sess = host.dispatch("telegram.server.session", &json!({})).unwrap();
        assert_eq!(sess["authenticated"], false);
        assert_eq!(sess["token"], "");
        assert!(sess["userId"].is_null());
        assert!(host.dispatch("feature.auth.deviceAgentSession", &json!({})).is_err());
    }

    // Cold reopen confirms 401 wipe persisted
    let reopened = dir.prod_host();
    assert_eq!(
        reopened.dispatch("telegram.server.session", &json!({})).unwrap()["authenticated"],
        false
    );
}

#[test]
fn e2e_t2_a2() {
    let dir = TempDirGuard::new("t2_a2");
    let host = dir.prod_host();

    // 1. Connection refused (offline) on login & register
    let closed = closed_loopback_url();
    host.dispatch("telegram.server.configureEndpoint", &json!({ "endpoint": closed }))
        .unwrap();
    let e_login = host
        .dispatch("telegram.server.login", &json!({ "username": "alice", "password": "pw" }))
        .unwrap_err();
    assert!(e_login.contains("SERVER_UNREACHABLE"), "got: {e_login}");
    let e_reg = host
        .dispatch("telegram.server.register", &json!({ "username": "alice", "password": "pw" }))
        .unwrap_err();
    assert!(e_reg.contains("SERVER_UNREACHABLE"), "got: {e_reg}");

    // 2. HTTP 500 on listChats and getMessages fails closed with zero fake rows
    let server = MockGpuiTelegramServer::start();
    configure_and_login(&host, &server, "alice");
    server.add_fault(FaultRule {
        method: Some("GET".into()),
        path_prefix: "/api/v1/chats".into(),
        status: 500,
        error_body: json!({ "error": "internal_error", "message": "Database down" }),
        delay_ms: 0,
        remaining_hits: Some(2),
        drop_connection: false,
        on_delay_entered: None,
    });
    let e_chats = host.dispatch("telegram.server.listChats", &json!({})).unwrap_err();
    assert!(e_chats.contains("HTTP 500"), "got: {e_chats}");
    let e_msgs = host
        .dispatch("telegram.server.getMessages", &json!({ "chatId": 101 }))
        .unwrap_err();
    assert!(e_msgs.contains("HTTP 500"), "got: {e_msgs}");
}

#[test]
fn e2e_t2_a3() {
    let dir = TempDirGuard::new("t2_a3");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    // Authenticated empty account -> server returns [] -> zero starter seeds invented
    let chats = host.dispatch("telegram.server.listChats", &json!({})).expect("listChats");
    assert!(chats["chats"].as_array().unwrap().is_empty());
    assert_eq!(chats["total"], 0);
}

#[test]
fn e2e_t2_a4() {
    let dir = TempDirGuard::new("t2_a4");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();
    host.dispatch("telegram.server.configureEndpoint", &json!({ "endpoint": server.url() }))
        .unwrap();

    // 1. 404 unknown attempt
    let e404 = host
        .dispatch("telegram.server.alipayPoll", &json!({ "attemptId": "missing_attempt" }))
        .unwrap_err();
    assert!(e404.contains("HTTP 404"), "got: {e404}");

    // 2. 410 gone attempt
    server.with_state(|s| {
        s.alipay_attempts.insert(
            "ali_gone".into(),
            AlipayAttemptState {
                poll_count: 0,
                complete_on_poll: 99,
                force_status_code: Some(410),
                force_status_label: Some("gone".into()),
            },
        );
        s.alipay_attempts.insert(
            "ali_expired".into(),
            AlipayAttemptState {
                poll_count: 0,
                complete_on_poll: 99,
                force_status_code: None,
                force_status_label: Some("expired".into()),
            },
        );
    });
    let e410 = host
        .dispatch("telegram.server.alipayPoll", &json!({ "attemptId": "ali_gone" }))
        .unwrap_err();
    assert!(e410.contains("HTTP 410"), "got: {e410}");

    // 3. 200 status="expired" must NOT persist session
    let exp = host
        .dispatch("telegram.server.alipayPoll", &json!({ "attemptId": "ali_expired" }))
        .expect("alipayPoll expired");
    assert_eq!(exp["status"], "expired");

    let sess = host.dispatch("telegram.server.session", &json!({})).unwrap();
    assert_eq!(sess["authenticated"], false);
}

#[test]
fn e2e_t2_a6() {
    let dir = TempDirGuard::new("t2_a6");
    let host = dir.prod_host();

    let providers = host.dispatch("magpie.providers", &json!({})).expect("magpie.providers");
    assert_eq!(providers["liveReady"], false);
    for p in providers["providers"].as_array().unwrap() {
        assert_eq!(p["configuredForLive"], false);
        assert_eq!(p["hasApiKey"], false);
    }

    let chat_err = host
        .dispatch(
            "magpie.chatComplete",
            &json!({ "messages": [{ "role": "user", "content": "Hello" }] }),
        )
        .unwrap_err();
    assert!(chat_err.contains("PROVIDER_NOT_CONFIGURED"), "got: {chat_err}");

    let hermes_err = host
        .dispatch("hermes.sendTurn", &json!({ "text": "Run /moa" }))
        .unwrap_err();
    assert!(hermes_err.contains("PROVIDER_NOT_CONFIGURED"), "got: {hermes_err}");
}

#[test]
fn e2e_t2_b1() {
    let dir = TempDirGuard::new("t2_b1");
    let server = MockGpuiTelegramServer::start();
    server.with_state(|s| {
        s.seed_chat(101, "Restricted Group", false, 1_700_000_000_000);
        s.non_admin_pin_chats.insert(101);
    });
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    let err = host
        .dispatch("telegram.server.pinMessage", &json!({ "chatId": 101, "messageId": 501 }))
        .unwrap_err();
    assert!(err.contains("HTTP 403"), "got: {err}");
    assert!(err.contains("Admin rights required"), "got: {err}");

    let pins = host
        .dispatch("telegram.server.listPinnedMessages", &json!({ "chatId": 101 }))
        .unwrap();
    assert!(pins["pinned_message_ids"].as_array().unwrap().is_empty());
}

#[test]
fn e2e_t2_b2() {
    let dir = TempDirGuard::new("t2_b2");
    let server = MockGpuiTelegramServer::start();
    server.with_state(|s| s.seed_chat(101, "Pin Rollback", false, 1_700_000_000_000));
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    // 1. HTTP 500 on pinChat fails closed
    server.add_fault(FaultRule {
        method: Some("POST".into()),
        path_prefix: "/api/v1/chats/101/pin".into(),
        status: 500,
        error_body: json!({ "error": "server_error", "message": "Pin failed" }),
        delay_ms: 0,
        remaining_hits: Some(1),
        drop_connection: false,
        on_delay_entered: None,
    });
    let e500 = host
        .dispatch("telegram.server.pinChat", &json!({ "chatId": 101, "pinned": true }))
        .unwrap_err();
    assert!(e500.contains("HTTP 500"), "got: {e500}");

    // 2. Offline connection refused fails closed
    host.dispatch(
        "telegram.server.configureEndpoint",
        &json!({ "endpoint": closed_loopback_url() }),
    )
    .unwrap();
    let e_off = host
        .dispatch("telegram.server.pinChat", &json!({ "chatId": 101, "pinned": true }))
        .unwrap_err();
    assert!(e_off.contains("SERVER_UNREACHABLE"), "got: {e_off}");

    // Reconnect and verify authoritative is_pinned remained false
    host.dispatch("telegram.server.configureEndpoint", &json!({ "endpoint": server.url() }))
        .unwrap();
    let chats = host.dispatch("telegram.server.listChats", &json!({})).unwrap();
    assert_eq!(chats["chats"][0]["is_pinned"], false);
}

#[test]
fn e2e_t2_b6() {
    let dir = TempDirGuard::new("t2_b6");
    let server = MockGpuiTelegramServer::start();
    server.with_state(|s| {
        s.seed_chat(101, "Empty History", false, 1_700_000_000_000);
        s.missing_contact_usernames.insert("ghost_user".into());
    });
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    let err = host
        .dispatch("telegram.server.addContact", &json!({ "username": "ghost_user" }))
        .unwrap_err();
    assert!(err.contains("HTTP 404"), "got: {err}");
    assert!(host.dispatch("telegram.server.listContacts", &json!({})).unwrap()["contacts"]
        .as_array()
        .unwrap()
        .is_empty());

    let page = host
        .dispatch(
            "telegram.server.getMessages",
            &json!({ "chatId": 101, "beforeId": 1, "limit": 20 }),
        )
        .expect("empty pagination page");
    assert!(page["messages"].as_array().unwrap().is_empty());
    assert_eq!(page["has_more"], false);
}

#[test]
fn e2e_t2_c1() {
    let dir = TempDirGuard::new("t2_c1");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    for invalid in ["ab", "9bot", "my_agent", "bad$bot"] {
        let err = host
            .dispatch(
                "telegram.server.createBot",
                &json!({ "displayName": "Bad Bot", "username": invalid }),
            )
            .unwrap_err();
        assert!(err.contains("HTTP 400"), "username '{invalid}' -> {err}");
    }

    let bots = host.dispatch("telegram.server.listMyBots", &json!({})).unwrap();
    assert!(bots["bots"].as_array().unwrap().is_empty());
}

#[test]
fn e2e_t2_c2() {
    let dir = TempDirGuard::new("t2_c2");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();

    // Owner (alice = 42) creates both a regular bot & a fabushi bot
    configure_and_login(&host, &server, "alice");
    host.dispatch(
        "telegram.server.createBot",
        &json!({
            "displayName": "Private Bot",
            "username": "private_owner_bot",
            "aiPersona": "TOP_SECRET_SYSTEM_PROMPT"
        }),
    )
    .unwrap();
    let fb = host
        .dispatch(
            "telegram.server.createFabushiBot",
            &json!({ "displayName": "Owner Fabushi", "username": "owner_fabushi_bot", "avatarColor": "#007AFF" }),
        )
        .unwrap();
    let fbot_id = fb["id"].as_str().unwrap().to_string();

    // Non-owner (bob = 99) logs in
    host.dispatch("telegram.server.logout", &json!({})).unwrap();
    configure_and_login(&host, &server, "bob");

    // Non-owner getBotProfile strips ai_persona
    let profile = host
        .dispatch("telegram.server.getBotProfile", &json!({ "target": "private_owner_bot" }))
        .expect("non-owner getBotProfile");
    assert_eq!(profile["username"], "private_owner_bot");
    assert!(profile.get("ai_persona").is_none() || profile["ai_persona"].is_null());

    // Non-owner PATCH returns 403 and fails closed without mutating bot
    let patch_err = host
        .dispatch(
            "telegram.server.patchFabushiBot",
            &json!({ "botId": fbot_id, "avatarColor": "#FF3B30" }),
        )
        .unwrap_err();
    assert!(patch_err.contains("HTTP 403"), "got: {patch_err}");
    let fetched = host
        .dispatch("telegram.server.getFabushiBot", &json!({ "botId": fbot_id }))
        .unwrap();
    assert_eq!(fetched["avatar_color"], "#007AFF");
}

#[test]
fn e2e_t2_c3() {
    let dir = TempDirGuard::new("t2_c3");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    // Switch endpoint to guaranteed-offline port
    host.dispatch(
        "telegram.server.configureEndpoint",
        &json!({ "endpoint": closed_loopback_url() }),
    )
    .unwrap();

    for (method, params) in [
        (
            "telegram.server.createBot",
            json!({ "displayName": "Off Bot", "username": "offline_test_bot" }),
        ),
        (
            "telegram.server.createFabushiBot",
            json!({ "displayName": "Off FBot", "username": "offline_fbot" }),
        ),
        (
            "telegram.server.botfatherNlCreate",
            json!({ "prompt": "Create offline bot" }),
        ),
    ] {
        let err = host.dispatch(method, &params).unwrap_err();
        assert!(err.contains("SERVER_UNREACHABLE"), "{method} -> {err}");
        assert!(!err.contains("tg_bot_"));
    }
}

#[test]
fn e2e_t2_c4() {
    let dir = TempDirGuard::new("t2_c4");
    let server = MockGpuiTelegramServer::start();
    server.with_state(|s| {
        for p in &mut s.coding_agent_probes {
            p["installed"] = json!(false);
            p["version"] = Value::Null;
        }
    });
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    let probe = host
        .dispatch("telegram.server.probeCodingAgents", &json!({ "deviceId": "android-1" }))
        .expect("probeCodingAgents");
    let providers = probe["providers"].as_array().unwrap();
    assert_eq!(providers.len(), 6);
    assert!(providers.iter().all(|p| p["installed"] == false));
    assert!(!probe.to_string().contains("原生编程智能体已就绪"));
}

#[test]
fn e2e_t2_d1() {
    let dir = TempDirGuard::new("t2_d1");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    // 1. Establish valid 8-char baseline code
    let baseline = host
        .dispatch("telegram.server.assistSetCode", &json!({ "code": "Abcd1234" }))
        .expect("8-char valid baseline code");
    assert_eq!(baseline["current_code"], "Abcd1234");
    assert_eq!(baseline["code_mode"], "custom");

    // 2. Spec §8.4: 8-16 ASCII alphanumeric with BOTH letters AND digits — all invalid codes must fail closed with HTTP 400
    for bad in [
        "",                  // empty
        "short67",           // 7 chars (< 8)
        "Abcdefgh123456789", // 17 chars (> 16)
        "abc!@#$%1",         // non-alphanumeric
        "12345678",          // digits-only (no letters)
        "abcdefgh",          // letters-only (no digits)
    ] {
        let reqs_before = server.recorded_requests().len();
        let err = host
            .dispatch("telegram.server.assistSetCode", &json!({ "code": bad }))
            .expect_err(&format!("code '{bad}' MUST be rejected"));
        assert!(
            err.contains("HTTP 400"),
            "code '{bad}' must propagate server HTTP 400 fail-closed error, got: {err}"
        );
        let reqs = server.recorded_requests();
        assert!(
            reqs[reqs_before..].iter().any(|r| {
                r.method == "POST"
                    && r.path == "/api/v1/remote/assist/set-code"
                    && r.body_json["code"].as_str() == Some(bad)
            }),
            "expected POST /api/v1/remote/assist/set-code recorded for invalid code '{bad}'"
        );
    }

    // 3. Verify baseline code remained completely unchanged after all 6 rejected attempts
    let info_after = host
        .dispatch("telegram.server.assistInfo", &json!({}))
        .expect("assistInfo after invalid attempts");
    assert_eq!(info_after["current_code"], "Abcd1234");
    assert_eq!(info_after["code_mode"], "custom");

    // 4. Valid upper boundary: 16 chars with both letters & digits
    let ok16 = host
        .dispatch("telegram.server.assistSetCode", &json!({ "code": "AbCdEfGh12345678" }))
        .expect("16-char valid code");
    assert_eq!(ok16["current_code"], "AbCdEfGh12345678");
    assert_eq!(ok16["code_mode"], "custom");
}

#[test]
fn e2e_t2_d2() {
    let dir = TempDirGuard::new("t2_d2");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    // 1. allow_assist == false -> 403
    let e_dis = host
        .dispatch(
            "telegram.server.assistStart",
            &json!({ "assistId": "123456789", "accessCode": "InitCode1" }),
        )
        .unwrap_err();
    assert!(e_dis.contains("HTTP 403"), "got: {e_dis}");

    // Enable assist, then test wrong code -> 403
    host.dispatch("telegram.server.assistAllow", &json!({ "allowAssist": true }))
        .unwrap();
    let e_wrong = host
        .dispatch(
            "telegram.server.assistStart",
            &json!({ "assistId": "123456789", "accessCode": "Wrong999" }),
        )
        .unwrap_err();
    assert!(e_wrong.contains("HTTP 403"), "got: {e_wrong}");

    // 3. Unknown 9-digit assist ID -> 404
    let e_404 = host
        .dispatch(
            "telegram.server.assistStart",
            &json!({ "assistId": "999888777", "accessCode": "InitCode1" }),
        )
        .unwrap_err();
    assert!(e_404.contains("HTTP 404"), "got: {e_404}");

    // Zero fake sessions created
    let sessions = host.dispatch("telegram.server.listRemoteSessions", &json!({})).unwrap();
    assert!(sessions["sessions"].as_array().unwrap().is_empty());
}

#[test]
fn e2e_t2_d3() {
    let dir = TempDirGuard::new("t2_d3");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();

    // 1. computer.executeOp fails closed on stale generation & local/unauthenticated nodes
    let lease = host
        .dispatch(
            "computer.leaseAcquire",
            &json!({ "nodeId": "local-android-mika", "holder": "e2e-tester" }),
        )
        .expect("leaseAcquire");
    let gen = lease["generation"].as_i64().unwrap();

    let e_stale = host
        .dispatch(
            "computer.executeOp",
            &json!({ "nodeId": "local-android-mika", "generation": gen - 1, "op": "screenshot" }),
        )
        .unwrap_err();
    assert!(e_stale.contains("Stale CUA lease generation"), "got: {e_stale}");

    let e_local = host
        .dispatch(
            "computer.executeOp",
            &json!({ "nodeId": "local-android-mika", "generation": gen, "op": "screenshot" }),
        )
        .unwrap_err();
    assert!(e_local.contains("LOCAL_EXECUTOR_REQUIRED"), "got: {e_local}");

    // 2. execComputer on run_policy="disabled" returns HTTP 403 (zero fake "执行完成 (0)")
    configure_and_login(&host, &server, "alice");
    host.dispatch(
        "telegram.server.upsertCloudComputer",
        &json!({ "nodeId": "cloud:42:locked", "displayName": "Locked", "host": "10.0.0.99" }),
    )
    .unwrap();
    host.dispatch(
        "telegram.server.patchComputerPolicy",
        &json!({ "nodeId": "cloud:42:locked", "runPolicy": "disabled" }),
    )
    .unwrap();

    let e_pol = host
        .dispatch(
            "telegram.server.execComputer",
            &json!({
                "nodeId": "cloud:42:locked",
                "operation": { "op": "shell_exec", "command": "whoami" }
            }),
        )
        .unwrap_err();
    assert!(e_pol.contains("HTTP 403"), "got: {e_pol}");
    assert!(!e_pol.contains("执行完成 (0)"));
}

#[test]
fn e2e_t2_d4() {
    let dir = TempDirGuard::new("t2_d4");
    let server = MockGpuiTelegramServer::start();
    let host = dir.prod_host();
    configure_and_login(&host, &server, "alice");

    // 1. Empty host fails closed
    let e_empty = host
        .dispatch(
            "telegram.server.upsertCloudComputer",
            &json!({ "displayName": "Bad Box", "host": "   " }),
        )
        .unwrap_err();
    assert!(!e_empty.is_empty());

    // 2. testComputer returning ok:false keeps online == false
    host.dispatch(
        "telegram.server.upsertCloudComputer",
        &json!({ "nodeId": "cloud:42:down", "displayName": "Down Box", "host": "10.0.0.250" }),
    )
    .unwrap();
    server.with_state(|s| {
        s.computer_test_override = Some(json!({
            "ok": false,
            "node_id": "cloud:42:down",
            "stderr": "Connection refused"
        }));
    });
    let test_res = host
        .dispatch("telegram.server.testComputer", &json!({ "nodeId": "cloud:42:down" }))
        .expect("testComputer");
    assert_eq!(test_res["ok"], false);
    assert_eq!(test_res["stderr"], "Connection refused");

    let list = host.dispatch("telegram.server.listComputers", &json!({})).unwrap();
    assert_eq!(list["computers"][0]["online"], false);
}

#[test]
fn e2e_t2_e1() {
    let dir = TempDirGuard::new("t2_e1");
    let server = MockGpuiTelegramServer::start();
    server.with_state(|s| s.seed_chat(101, "Slow Sync", false, 1_700_000_000_000));

    let host = Arc::new(dir.prod_host());
    configure_and_login(&host, &server, "alice");

    // Deterministic server-entry signal: flips true right before the 400ms HTTP delay (outside lock!)
    let delay_entered = Arc::new((Mutex::new(false), Condvar::new()));

    // Inject 400ms HTTP delay (slept OUTSIDE MockServerState mutex!)
    server.add_fault(FaultRule {
        method: Some("GET".into()),
        path_prefix: "/api/v1/chats".into(),
        status: 0,
        error_body: Value::Null,
        delay_ms: 400,
        remaining_hits: Some(1),
        drop_connection: false,
        on_delay_entered: Some(Arc::clone(&delay_entered)),
    });

    let host_a = Arc::clone(&host);
    let t0 = Instant::now();
    let thread_a = thread::spawn(move || {
        let res = host_a.dispatch("telegram.server.listChats", &json!({}));
        (res, t0.elapsed())
    });

    // Deterministically await until MockGpuiTelegramServer has received Thread A's GET /api/v1/chats
    // and entered its 400ms delay (guaranteeing Thread A is actively inside blocking ureq HTTP I/O).
    {
        let (lock, cvar) = &*delay_entered;
        let guard = lock.lock().expect("lock delay_entered");
        let (guard, _) = cvar
            .wait_timeout_while(guard, Duration::from_secs(2), |entered| !*entered)
            .expect("wait_timeout_while delay_entered");
        assert!(
            *guard,
            "Thread A never entered MockGpuiTelegramServer's 400ms delay within 2s"
        );
    }

    let b_start = Instant::now();
    let info = host.dispatch("feature.info", &json!({})).expect("concurrent feature.info");
    assert_eq!(info["desktopParityVersion"], "1.6.6");
    let sess = host.dispatch("telegram.server.session", &json!({})).expect("concurrent session");
    assert_eq!(sess["authenticated"], true);
    host.dispatch(
        "feature.persistence.set",
        &json!({ "key": "e2e_lock_free", "value": "ok" }),
    )
    .expect("concurrent persistence.set");
    let b_elapsed = b_start.elapsed();

    assert!(
        b_elapsed < Duration::from_millis(150),
        "Lock-free concurrency violated: Thread B took {b_elapsed:?} during Thread A's 400ms HTTP call"
    );

    let (a_res, a_elapsed) = thread_a.join().expect("join Thread A");
    assert!(a_res.is_ok(), "Thread A listChats failed: {a_res:?}");
    assert!(
        a_elapsed >= Duration::from_millis(350),
        "Expected Thread A to experience >=350ms server delay, got {a_elapsed:?}"
    );
}

#[test]
fn e2e_t2_e3() {
    let dir = TempDirGuard::new("t2_e3");
    let secret = "E2E_PIN_9876_NEVER_PLAINTEXT";
    let unicode_ok = "🔥🔥🔥🔥"; // 4 Unicode chars (16 UTF-8 bytes)
    let unicode_too_short = "🔥🔥🔥"; // 3 Unicode chars (12 UTF-8 bytes < 4 chars)

    {
        let host = dir.prod_host();

        assert!(host
            .dispatch("telegram.auth.passcodeSet", &json!({ "passcode": "" }))
            .is_err());
        assert!(host
            .dispatch("telegram.auth.passcodeSet", &json!({ "passcode": "123" }))
            .is_err());
        // Multi-byte Unicode char-count boundary: 12 bytes but only 3 Unicode chars (< 4) MUST fail
        assert!(
            host.dispatch("telegram.auth.passcodeSet", &json!({ "passcode": unicode_too_short }))
                .is_err(),
            "3-char multi-byte Unicode passcode must be rejected"
        );

        // 4 Unicode chars MUST succeed and verify accurately
        host.dispatch("telegram.auth.passcodeSet", &json!({ "passcode": unicode_ok }))
            .expect("4-char Unicode passcodeSet");
        assert_eq!(
            host.dispatch("telegram.auth.passcodeVerify", &json!({ "passcode": unicode_ok }))
                .unwrap()["valid"],
            true
        );
        assert_eq!(
            host.dispatch("telegram.auth.passcodeVerify", &json!({ "passcode": unicode_too_short }))
                .unwrap()["valid"],
            false
        );

        // Distinctive ASCII secret must verify and never be written in plaintext on disk
        host.dispatch("telegram.auth.passcodeSet", &json!({ "passcode": secret }))
            .expect("passcodeSet distinctive secret");
        assert_eq!(
            host.dispatch("telegram.auth.passcodeVerify", &json!({ "passcode": "wrong" }))
                .unwrap()["valid"],
            false
        );
        assert_eq!(
            host.dispatch("telegram.auth.passcodeVerify", &json!({ "passcode": secret }))
                .unwrap()["valid"],
            true
        );
    }

    // Host dropped -> SQLite flushed/closed: scan all files on disk for zero plaintext leakage
    assert_no_plaintext_bytes_on_disk(dir.path(), secret);
    assert_no_plaintext_bytes_on_disk(dir.path(), unicode_ok);
}

#[test]
fn e2e_t2_e4() {
    let dir = TempDirGuard::new("t2_e4");
    let host = dir.prod_host();

    // Malformed JSON and unknown methods via dispatch_json must return {"ok":false,"error":...} without panicking
    for bad_req in [
        "{not valid json",
        "",
        "[]",
        r#"{"method":"nonexistent.e2e.method","params":{}}"#,
        r#"{"method":"","params":{}}"#,
    ] {
        let raw = host.dispatch_json(bad_req);
        let parsed: Value = serde_json::from_str(&raw).expect("valid JSON envelope");
        assert_eq!(parsed["ok"], false, "req '{bad_req}' -> {raw}");
        assert!(parsed["error"].as_str().map(|s| !s.is_empty()).unwrap_or(false));
    }

    // Same guarantee across C-ABI entrypoint (respecting panic = "abort" in release)
    let c_dir = CString::new(dir.path().to_str().unwrap()).unwrap();
    let handle = unsafe { mahayana_app_host_create(c_dir.as_ptr()) };
    assert!(handle > 0);
    for bad_req in ["{broken", r#"{"method":"unknown.op","params":{}}"#] {
        let c_in = CString::new(bad_req).unwrap();
        let out_ptr = unsafe { mahayana_app_host_dispatch_with_handle(handle, c_in.as_ptr()) };
        let out_str = unsafe { CStr::from_ptr(out_ptr).to_str().unwrap().to_string() };
        unsafe { mahayana_app_host_free_string(out_ptr) };
        let parsed: Value = serde_json::from_str(&out_str).unwrap();
        assert_eq!(parsed["ok"], false);
    }
    mahayana_app_host_destroy(handle);
}

#[test]
fn e2e_t2_e5() {
    let dir = TempDirGuard::new("t2_e5");
    let host = dir.prod_host();

    let w = host
        .dispatch("wallet.createWallet", &json!({ "initialNanotons": 500_000_000i64 }))
        .unwrap();
    let addr = w["address"].as_str().unwrap();

    // 1. amountNanotons == 0 rejected
    assert!(host
        .dispatch("wallet.sendTon", &json!({ "to": addr, "amountNanotons": 0u64 }))
        .is_err());

    // 2. Invalid address rejected
    assert!(host
        .dispatch("wallet.sendTon", &json!({ "to": "invalid-ton-addr", "amountNanotons": 1000u64 }))
        .is_err());

    // 3. Insufficient balance rejected
    assert!(host
        .dispatch(
            "wallet.sendTon",
            &json!({ "to": addr, "amountNanotons": 9_000_000_000u64 })
        )
        .is_err());

    // 4. tonConnectApprove unconditionally fails closed with TONCONNECT_UNAVAILABLE
    let tc_err = host
        .dispatch(
            "wallet.tonConnectApprove",
            &json!({ "recipient": addr, "amountNanotons": 10_000u64, "dappName": "STON.fi" }),
        )
        .unwrap_err();
    assert!(tc_err.contains("TONCONNECT_UNAVAILABLE"), "got: {tc_err}");

    // 5. wallet.sendTon with broadcast: true against unreachable TON RPC fails closed without mutating balance or seqno
    host.dispatch(
        "wallet.configureRpc",
        &json!({ "rpcEndpoint": closed_loopback_url() }),
    )
    .expect("wallet.configureRpc unreachable");
    let bc_err = host
        .dispatch(
            "wallet.sendTon",
            &json!({
                "to": addr,
                "amountNanotons": 100_000_000u64,
                "comment": "e2e-broadcast-fail-closed",
                "broadcast": true
            }),
        )
        .expect_err("broadcast: true against unreachable TON RPC must fail closed");
    assert!(
        bc_err.contains("BROADCAST_FAILED"),
        "expected BROADCAST_FAILED error, got: {bc_err}"
    );

    // Balance AND seqno MUST remain completely untouched
    let status = host.dispatch("wallet.status", &json!({})).unwrap();
    assert_eq!(status["balanceNanotons"], 500_000_000i64);
    assert_eq!(status["seqno"], 0);
}

#[test]
fn e2e_t2_g2() {
    // Reject dangerous / wrapper / malformed custom webapp URLs
    for rejected in [
        "",
        "   ",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "JAVASCRIPT:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "//evil.example.com/steal",
        "gpui-miniapp://Bot/app",
        "tg://resolve?domain=Bot",
        "https://t.me/Bot/app",
        "http://t.me/Bot/app",
        "t.me/Bot/app",
        "https://",
        "https:///path",
        "https://ok.example/path\nHeader: injected",
        "ftp://files.example/a",
    ] {
        assert_eq!(
            sanitize_webapp_custom_url(rejected),
            None,
            "sanitize_webapp_custom_url MUST reject '{rejected}'"
        );
    }

    // Valid canonical URLs pass cleanly
    assert_eq!(
        sanitize_webapp_custom_url("HTTPS://fabushi.ombhrum.com/miniapp"),
        Some("https://fabushi.ombhrum.com/miniapp".to_string())
    );
    assert_eq!(
        sanitize_webapp_custom_url("/local/page?tab=1"),
        Some("/local/page?tab=1".to_string())
    );
}

#[test]
fn e2e_t2_g3() {
    // 0. Assert literal Telegram CloudStorage spec constants (prevent self-referential constant drift)
    assert_eq!(CLOUD_STORAGE_MAX_KEY_BYTES, 128);
    assert_eq!(CLOUD_STORAGE_MAX_VALUE_BYTES, 4096);
    assert_eq!(CLOUD_STORAGE_MAX_KEYS, 1024);
    assert_eq!(CLOUD_STORAGE_MAX_TOTAL_BYTES, 5 * 1024 * 1024);

    // 1. Key byte bounds: empty & >128B rejected, 128B accepted (both ASCII and multi-byte UTF-8)
    assert!(validate_cloud_storage_entry("", "val", &[]).is_err());
    let key_128 = "k".repeat(128);
    let key_129 = "k".repeat(129);
    assert!(validate_cloud_storage_entry(&key_128, "ok", &[]).is_ok());
    assert!(validate_cloud_storage_entry(&key_129, "ok", &[]).is_err());

    // Multi-byte UTF-8 key: '禅' is 3 UTF-8 bytes -> 42 chars = 126B (Ok), 43 chars = 129B (Err)
    let utf8_key_126b = "禅".repeat(42);
    let utf8_key_129b = "禅".repeat(43);
    assert_eq!(utf8_key_126b.len(), 126);
    assert_eq!(utf8_key_129b.len(), 129);
    assert!(validate_cloud_storage_entry(&utf8_key_126b, "ok", &[]).is_ok());
    assert!(validate_cloud_storage_entry(&utf8_key_129b, "ok", &[]).is_err());

    // 2. Value byte bounds: 4096B accepted, 4097B rejected
    let val_4096 = "v".repeat(4096);
    let val_4097 = "v".repeat(4097);
    assert!(validate_cloud_storage_entry("valid_key", &val_4096, &[]).is_ok());
    assert!(validate_cloud_storage_entry("valid_key", &val_4097, &[]).is_err());

    // 3. Max keys bound (1024): inserting 1025th distinct key rejected, updating existing key allowed
    let full_items: Vec<CloudStorageItem> = (0..1024)
        .map(|i| CloudStorageItem {
            key: format!("k_{i}"),
            value: "x".to_string(),
            updated_at: 0,
        })
        .collect();
    assert!(validate_cloud_storage_entry("k_new_1025", "v", &full_items).is_err());
    assert!(validate_cloud_storage_entry("k_0", "updated", &full_items).is_ok());

    // 4. Exact 5MB total quota boundary: 5_242_880 bytes total (Ok) vs 5_242_881 bytes total (Err)
    let exact_5mb = 5_242_880usize;
    let heavy_key = "h".to_string(); // 1 byte
    let heavy_val = "a".repeat(exact_5mb - 1 - 10); // 5_242_869 bytes -> existing total = 5_242_870 bytes
    let existing_near_5mb = vec![CloudStorageItem {
        key: heavy_key,
        value: heavy_val,
        updated_at: 0,
    }];
    let existing_bytes: usize = existing_near_5mb
        .iter()
        .map(|i| i.key.len() + i.value.len())
        .sum();
    assert_eq!(existing_bytes, exact_5mb - 10);

    // Exact 5_242_880 bytes total (1B key + 9B value = 10B) MUST succeed
    assert_eq!(existing_bytes + "k".len() + "123456789".len(), 5_242_880);
    assert!(
        validate_cloud_storage_entry("k", "123456789", &existing_near_5mb).is_ok(),
        "exact 5_242_880 bytes total CloudStorage quota must be accepted"
    );

    // 5_242_881 bytes total (1B key + 10B value = 11B) MUST fail
    assert_eq!(existing_bytes + "k".len() + "1234567890".len(), 5_242_881);
    let quota_err = validate_cloud_storage_entry("k", "1234567890", &existing_near_5mb)
        .expect_err("5_242_881 bytes total CloudStorage quota must fail");
    assert!(quota_err.contains("5MB"), "got: {quota_err}");
}
