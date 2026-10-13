use crate::notify_drain_gate::NotifyDrainGate;
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub const XUSER_RELAY_POLL_INTERVAL_MS: u64 = 4_000;
pub const XUSER_RELAY_ERROR_BACKOFF_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XuserRelayHttpError {
    pub status: u16,
    pub path: String,
}

impl std::fmt::Display for XuserRelayHttpError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "sharing {} returned {}", self.path, self.status)
    }
}
impl std::error::Error for XuserRelayHttpError {}

pub trait XuserRelayTransport: Send + Sync {
    fn request(
        &self,
        method: &str,
        url: &str,
        token: &str,
        body: Option<&Value>,
    ) -> Result<Value, XuserRelayHttpError>;
}

/// Android Host adapter. Authentication is injected by the canonical account owner; this transport
/// never stores or refreshes credentials and therefore cannot become a second account authority.
pub struct UreqXuserRelayTransport {
    client_type: String,
    client_version: String,
    box_namespace: String,
}

impl UreqXuserRelayTransport {
    pub fn new(
        client_type: impl Into<String>,
        client_version: impl Into<String>,
        box_namespace: impl Into<String>,
    ) -> Result<Self, String> {
        let client_type = client_type.into();
        let client_version = client_version.into();
        let box_namespace = box_namespace.into();
        for (label, value) in [
            ("client type", client_type.as_str()),
            ("client version", client_version.as_str()),
            ("box namespace", box_namespace.as_str()),
        ] {
            if value.trim().is_empty()
                || value.len() > 256
                || value.chars().any(char::is_control)
            {
                return Err(format!("xuser relay {label} is invalid"));
            }
        }
        Ok(Self {
            client_type,
            client_version,
            box_namespace,
        })
    }
}

impl XuserRelayTransport for UreqXuserRelayTransport {
    fn request(
        &self,
        method: &str,
        url: &str,
        token: &str,
        body: Option<&Value>,
    ) -> Result<Value, XuserRelayHttpError> {
        let request = match method {
            "GET" => ureq::get(url),
            "DELETE" => ureq::delete(url),
            _ => ureq::post(url),
        }
        .timeout_connect(15_000)
        .timeout(Duration::from_secs(30))
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", "application/json")
        .set("x-cursor-client-type", &self.client_type)
        .set("x-cursor-client-version", &self.client_version)
        .set("x-sand-box-namespace", &self.box_namespace);

        let response = match body {
            Some(body) => request.send_json(body.clone()),
            None => request.call(),
        };
        match response {
            Ok(response) => {
                let status = response.status();
                response
                    .into_json::<Value>()
                    .map_err(|_| XuserRelayHttpError {
                        status,
                        path: url.into(),
                    })
            }
            Err(ureq::Error::Status(status, _)) => Err(XuserRelayHttpError {
                status,
                path: url.into(),
            }),
            Err(ureq::Error::Transport(_)) => Err(XuserRelayHttpError {
                status: 0,
                path: url.into(),
            }),
        }
    }
}

pub struct XuserRelayClient {
    backend_url: String,
    access_token: Arc<dyn Fn() -> Result<String, String> + Send + Sync>,
    transport: Arc<dyn XuserRelayTransport>,
}

impl XuserRelayClient {
    pub fn new(
        backend_url: String,
        access_token: Arc<dyn Fn() -> Result<String, String> + Send + Sync>,
        transport: Arc<dyn XuserRelayTransport>,
    ) -> Self {
        Self {
            backend_url: backend_url.trim_end_matches('/').into(),
            access_token,
            transport,
        }
    }

    fn call_raw(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, XuserRelayHttpError> {
        let token = (self.access_token)().map_err(|_| XuserRelayHttpError {
            status: 0,
            path: path.into(),
        })?;
        self.transport
            .request(
                method,
                &format!("{}{}", self.backend_url, path),
                &token,
                body,
            )
            .map_err(|mut error| {
                error.path = path.into();
                error
            })
    }

    fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
        self.call_raw(method, path, body)
            .map_err(|error| describe_relay_error(&error))
    }

    pub fn poll(&self, ack_ids: &[String]) -> Result<Value, String> {
        self.call(
            "POST",
            "/sand/xuser/poll",
            Some(&serde_json::json!({"ackIds":ack_ids})),
        )
    }

    pub fn fetch_share_state(&self) -> Result<Value, String> {
        self.call("POST", "/sand/share-state", Some(&serde_json::json!({})))
    }

    pub fn send(&self, payload: &Value) -> Result<Value, String> {
        let result = self.call("POST", "/sand/xuser/send", Some(payload))?;
        Ok(result
            .get("timestampMs")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .map(|value| serde_json::json!({"timestampMs":value}))
            .unwrap_or_else(|| serde_json::json!({})))
    }

    pub fn create_room(&self, payload: &Value) -> Result<Value, String> {
        self.call("POST", "/sand/share-rooms", Some(payload))
    }

    pub fn create_room_from_agent(&self, payload: &Value) -> Result<Value, String> {
        Ok(match self.call_raw("POST", "/sand/share-rooms/from-agent", Some(payload)) {
            Ok(result) => project_room_invite_result(&result),
            Err(error) => serde_json::json!({
                "status":"error",
                "message":describe_relay_error(&error),
            }),
        })
    }

    pub fn create_room_invite(&self, room_id: &str) -> Result<Value, String> {
        let body = serde_json::json!({"roomId":room_id});
        Ok(match self.call_raw("POST", "/sand/share-rooms/invite-links", Some(&body)) {
            Ok(result) => project_room_invite_result(&result),
            Err(error) => serde_json::json!({
                "status":"error",
                "message":describe_relay_error(&error),
            }),
        })
    }

    pub fn join_room(&self, link: &str) -> Result<Value, String> {
        let body = serde_json::json!({"link":link});
        Ok(match self.call_raw("POST", "/sand/share-rooms/join", Some(&body)) {
            Ok(result) => project_join_room_result(&result),
            Err(error) => serde_json::json!({
                "status":"error",
                "message":describe_relay_error(&error),
            }),
        })
    }

    pub fn respond_to_join_request(&self, payload: &Value) -> Result<Value, String> {
        self.call("POST", "/sand/share-rooms/join/respond", Some(payload))
    }

    pub fn add_own_agent(&self, payload: &Value) -> Result<Value, String> {
        self.call("POST", "/sand/share-rooms/agents/add", Some(payload))
    }

    pub fn set_room_picture(&self, payload: &Value) -> Result<Value, String> {
        self.call("POST", "/sand/share-rooms/picture", Some(payload))
    }

    pub fn remove_own_agent(&self, payload: &Value) -> Result<Value, String> {
        self.call("POST", "/sand/share-rooms/agents/remove", Some(payload))
    }

    pub fn leave_room_with_target_raw(
        &self,
        room_id: &str,
        target_auth_id: Option<&str>,
    ) -> Result<Value, XuserRelayHttpError> {
        let mut body = serde_json::json!({"roomId":room_id});
        if let Some(target_auth_id) = target_auth_id {
            body["targetAuthId"] = Value::String(target_auth_id.into());
        }
        self.call_raw("POST", "/sand/share-rooms/leave", Some(&body))
    }

    pub fn leave_room_with_target(
        &self,
        room_id: &str,
        target_auth_id: Option<&str>,
    ) -> Result<Value, String> {
        self.leave_room_with_target_raw(room_id, target_auth_id)
            .map_err(|error| describe_relay_error(&error))
    }

    pub fn leave_room(&self, room_id: &str) -> Result<Value, String> {
        self.leave_room_with_target(room_id, None)
    }

    pub fn remove_deleted_agent_raw(
        &self,
        agent_id: &str,
    ) -> Result<Value, XuserRelayHttpError> {
        self.call_raw(
            "POST",
            "/sand/share-rooms/agents/remove-deleted",
            Some(&serde_json::json!({"agentId":agent_id})),
        )
    }

    pub fn remove_deleted_agent(&self, agent_id: &str) -> Result<Value, String> {
        self.remove_deleted_agent_raw(agent_id)
            .map_err(|error| describe_relay_error(&error))
    }
}

fn project_room_invite_result(result: &Value) -> Value {
    let room_id = result
        .get("room")
        .and_then(Value::as_object)
        .and_then(|room| room.get("roomId"))
        .and_then(Value::as_str);
    let share_url = result.get("shareUrl").and_then(Value::as_str);
    let expires_at_ms = result.get("expiresAtMs").and_then(Value::as_f64);
    match (room_id, share_url, expires_at_ms) {
        (Some(room_id), Some(share_url), Some(expires_at_ms)) if expires_at_ms.is_finite() => {
            serde_json::json!({
                "status":"ok",
                "shareUrl":share_url,
                "expiresAtMs":expires_at_ms,
                "roomId":room_id,
            })
        }
        _ => serde_json::json!({"status":"error","message":"Unexpected response."}),
    }
}

fn project_join_room_result(result: &Value) -> Value {
    match result.get("status").and_then(Value::as_str) {
        Some("pending") => serde_json::json!({
            "status":"pending",
            "roomName":result.get("roomName").and_then(Value::as_str).unwrap_or("the shared room"),
        }),
        Some("already-member") => {
            let room = result.get("room").and_then(Value::as_object);
            let mut projected = serde_json::json!({
                "status":"already-member",
                "roomName":room.and_then(|room|room.get("name")).and_then(Value::as_str).unwrap_or("the shared room"),
            });
            if let Some(room_id) = room
                .and_then(|room| room.get("roomId"))
                .and_then(Value::as_str)
            {
                projected["roomId"] = Value::String(room_id.into());
            }
            projected
        }
        Some(status @ ("invalid" | "denied" | "rate-limited")) => {
            serde_json::json!({"status":status})
        }
        _ => serde_json::json!({"status":"error","message":"Unexpected response."}),
    }
}

pub type XuserRelayEventHandler = Arc<dyn Fn(&Value) -> Result<bool, String> + Send + Sync>;

#[derive(Debug, Default)]
struct RelayRuntimeState {
    started: bool,
    is_ticking: bool,
    backoff_until_ms: u64,
    pending_ack_ids: Vec<String>,
}

pub struct XuserRelayRuntime {
    client: Arc<XuserRelayClient>,
    on_event: XuserRelayEventHandler,
    now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
    is_notify_connected: Arc<dyn Fn() -> bool + Send + Sync>,
    is_notify_safety_poll_enabled: Arc<dyn Fn() -> bool + Send + Sync>,
    gate: Mutex<NotifyDrainGate>,
    state: Mutex<RelayRuntimeState>,
}

impl XuserRelayRuntime {
    pub fn new(
        client: Arc<XuserRelayClient>,
        on_event: XuserRelayEventHandler,
        now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
        is_notify_connected: Arc<dyn Fn() -> bool + Send + Sync>,
        is_notify_safety_poll_enabled: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            client,
            on_event,
            now_ms,
            is_notify_connected,
            is_notify_safety_poll_enabled,
            gate: Mutex::new(NotifyDrainGate::default()),
            state: Mutex::new(RelayRuntimeState::default()),
        }
    }

    pub fn start(&self) {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.started {
            return;
        }
        state.started = true;
        drop(state);
        self.gate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .reset();
    }

    pub fn stop(&self) {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.started = false;
        state.is_ticking = false;
        state.backoff_until_ms = 0;
    }

    pub fn record_notify(&self) {
        self.gate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .record_notify();
    }

    pub fn request_drain(&self) -> Result<bool, String> {
        self.record_notify();
        self.tick()
    }

    pub fn pending_ack_ids(&self) -> Vec<String> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pending_ack_ids
            .clone()
    }

    pub fn backoff_until_ms(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .backoff_until_ms
    }

    pub fn tick(&self) -> Result<bool, String> {
        let now = (self.now_ms)();
        let pending = {
            let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if !state.started || state.is_ticking || now < state.backoff_until_ms {
                return Ok(false);
            }
            let should_drain = self
                .gate
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .should_drain(
                    now,
                    !state.pending_ack_ids.is_empty(),
                    (self.is_notify_connected)(),
                    (self.is_notify_safety_poll_enabled)(),
                );
            if !should_drain {
                return Ok(false);
            }
            state.is_ticking = true;
            state.pending_ack_ids.clone()
        };

        let payload = match self.client.poll(&pending) {
            Ok(payload) => payload,
            Err(error) => {
                let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.is_ticking = false;
                state.backoff_until_ms = now.saturating_add(XUSER_RELAY_ERROR_BACKOFF_MS);
                return Err(error);
            }
        };

        let previously_acked = pending.iter().cloned().collect::<BTreeSet<_>>();
        let mut next_ack_ids = Vec::new();
        let mut next_ack_set = BTreeSet::new();
        if let Some(events) = payload.get("events").and_then(Value::as_array) {
            for event in events {
                let Some(record) = event.as_object() else { continue };
                let Some(id) = record.get("id").and_then(Value::as_str) else {
                    continue;
                };
                if record.get("kind").and_then(Value::as_str).is_none() {
                    continue;
                }
                if previously_acked.contains(id) || next_ack_set.contains(id) {
                    if next_ack_set.insert(id.to_string()) {
                        next_ack_ids.push(id.to_string());
                    }
                    continue;
                }
                if (self.on_event)(event).unwrap_or(false) && next_ack_set.insert(id.to_string()) {
                    next_ack_ids.push(id.to_string());
                }
            }
        }

        {
            let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.pending_ack_ids = next_ack_ids;
            state.is_ticking = false;
            state.backoff_until_ms = 0;
        }
        self.gate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .record_poll(now);
        Ok(true)
    }
}

pub struct XuserRelayDriver {
    runtime: Arc<XuserRelayRuntime>,
    stopped: Arc<AtomicBool>,
    wake_tx: Mutex<Option<mpsc::Sender<()>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl XuserRelayDriver {
    pub fn new(runtime: Arc<XuserRelayRuntime>) -> Self {
        Self {
            runtime,
            stopped: Arc::new(AtomicBool::new(true)),
            wake_tx: Mutex::new(None),
            worker: Mutex::new(None),
        }
    }

    pub fn runtime(&self) -> Arc<XuserRelayRuntime> {
        Arc::clone(&self.runtime)
    }

    pub fn start(&self) {
        let mut worker = self.worker.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if worker.is_some() {
            return;
        }
        self.stopped.store(false, Ordering::Release);
        self.runtime.start();
        let (wake_tx, wake_rx) = mpsc::channel();
        *self.wake_tx.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(wake_tx);
        let runtime = Arc::clone(&self.runtime);
        let stopped = Arc::clone(&self.stopped);
        *worker = thread::Builder::new()
            .name("android-xuser-relay".into())
            .spawn(move || {
                while !stopped.load(Ordering::Acquire) {
                    if let Err(error) = runtime.tick() {
                        eprintln!("[fabushi:xuser] relay poll failed: {error}");
                    }
                    if stopped.load(Ordering::Acquire) {
                        break;
                    }
                    match wake_rx.recv_timeout(Duration::from_millis(XUSER_RELAY_POLL_INTERVAL_MS)) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .ok();
    }

    pub fn request_drain(&self) {
        self.runtime.record_notify();
        if let Some(wake) = self
            .wake_tx
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
        {
            let _ = wake.send(());
        }
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(wake) = self
            .wake_tx
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            let _ = wake.send(());
        }
        if let Some(worker) = self
            .worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            let _ = worker.join();
        }
        self.runtime.stop();
    }

    pub fn is_started(&self) -> bool {
        self.worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }
}

impl Drop for XuserRelayDriver {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Ok(wake) = self.wake_tx.get_mut() {
            if let Some(wake) = wake.take() {
                let _ = wake.send(());
            }
        }
        if let Ok(worker) = self.worker.get_mut() {
            if let Some(worker) = worker.take() {
                let _ = worker.join();
            }
        }
        self.runtime.stop();
    }
}

pub fn describe_relay_error(error: &XuserRelayHttpError) -> String {
    match error.status {
        403 => "Sharing isn't enabled for your account.".into(),
        429 => "You're doing that too often. Try again in a minute.".into(),
        _ => "Couldn't reach the sharing service. Check your connection and try again.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::AtomicU64;

    #[derive(Default)]
    struct FakeTransport {
        responses: Mutex<VecDeque<Result<Value, XuserRelayHttpError>>>,
        bodies: Mutex<Vec<Value>>,
    }

    impl XuserRelayTransport for FakeTransport {
        fn request(
            &self,
            _method: &str,
            url: &str,
            _token: &str,
            body: Option<&Value>,
        ) -> Result<Value, XuserRelayHttpError> {
            if let Some(body) = body {
                self.bodies.lock().unwrap().push(body.clone());
            }
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err(XuserRelayHttpError {
                    status: 0,
                    path: url.into(),
                }))
        }
    }

    fn runtime(
        transport: Arc<FakeTransport>,
        clock: Arc<AtomicU64>,
        events: Arc<Mutex<Vec<String>>>,
    ) -> XuserRelayRuntime {
        let client = Arc::new(XuserRelayClient::new(
            "https://relay.invalid".into(),
            Arc::new(|| Ok("token".into())),
            transport,
        ));
        let event_sink = Arc::clone(&events);
        XuserRelayRuntime::new(
            client,
            Arc::new(move |event| {
                if let Some(id) = event.get("id").and_then(Value::as_str) {
                    event_sink.lock().unwrap().push(id.into());
                }
                Ok(true)
            }),
            {
                let clock = Arc::clone(&clock);
                Arc::new(move || clock.load(Ordering::SeqCst))
            },
            Arc::new(|| true),
            Arc::new(|| true),
        )
    }

    #[test]
    fn processed_events_are_acked_on_next_poll_without_duplicate_delivery() {
        let transport = Arc::new(FakeTransport::default());
        transport.responses.lock().unwrap().push_back(Ok(serde_json::json!({
            "events":[{"id":"event-a","kind":"room-entry"}]
        })));
        transport
            .responses
            .lock()
            .unwrap()
            .push_back(Ok(serde_json::json!({"events":[]})));
        let clock = Arc::new(AtomicU64::new(1));
        let events = Arc::new(Mutex::new(Vec::new()));
        let runtime = runtime(Arc::clone(&transport), Arc::clone(&clock), Arc::clone(&events));
        runtime.start();
        assert!(runtime.tick().unwrap());
        assert_eq!(runtime.pending_ack_ids(), vec!["event-a"]);
        clock.store(5_000, Ordering::SeqCst);
        assert!(runtime.tick().unwrap());
        let bodies = transport.bodies.lock().unwrap();
        assert_eq!(bodies[1]["ackIds"][0], "event-a");
        assert_eq!(events.lock().unwrap().as_slice(), &["event-a"]);
    }

    #[test]
    fn transport_failure_enters_backoff_without_losing_pending_ack() {
        let transport = Arc::new(FakeTransport::default());
        transport.responses.lock().unwrap().push_back(Ok(serde_json::json!({
            "events":[{"id":"event-a","kind":"room-entry"}]
        })));
        transport.responses.lock().unwrap().push_back(Err(XuserRelayHttpError {
            status: 0,
            path: "/sand/xuser/poll".into(),
        }));
        let clock = Arc::new(AtomicU64::new(1));
        let events = Arc::new(Mutex::new(Vec::new()));
        let runtime = runtime(transport, Arc::clone(&clock), events);
        runtime.start();
        assert!(runtime.tick().unwrap());
        clock.store(5_000, Ordering::SeqCst);
        assert!(runtime.tick().is_err());
        assert_eq!(runtime.pending_ack_ids(), vec!["event-a"]);
        assert_eq!(runtime.backoff_until_ms(), 5_000 + XUSER_RELAY_ERROR_BACKOFF_MS);
        clock.store(5_001, Ordering::SeqCst);
        assert!(!runtime.tick().unwrap());
    }
}

// Desktop-compatible type names retained at the Rust responsibility boundary. Android swaps
// only the HTTP transport implementation; relay/service semantics and callers stay source-stable.
pub type SandXuserRelayClient = XuserRelayClient;
pub type SandXuserRelayRuntime = XuserRelayRuntime;
pub type SandXuserRelayDriver = XuserRelayDriver;
pub type ReqwestXuserRelayTransport = UreqXuserRelayTransport;
