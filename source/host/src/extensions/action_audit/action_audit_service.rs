use std::collections::{BTreeMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const ACTION_AUDIT_FLUSH_INTERVAL_MS: u64 = 5_000;
pub const MAX_AUDIT_BATCH_SIZE: usize = 50;
pub const MAX_PENDING_AUDIT_EVENTS: usize = 2_000;
pub const AUDIT_FLUSH_FAILURE_BACKOFF_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AuditAction {
    McpToolCall {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "serverIdentifier")]
        server_identifier: String,
        #[serde(rename = "serverName", skip_serializing_if = "Option::is_none")]
        server_name: Option<String>,
        #[serde(rename = "toolName")]
        tool_name: String,
        transport: String,
        status: String,
        #[serde(rename = "durationMs")]
        duration_ms: f64,
    },
    ShellCommand {
        command: String,
        #[serde(rename = "shellKind")]
        shell_kind: String,
        target: String,
        allowed: Option<bool>,
        #[serde(rename = "blockedReason", skip_serializing_if = "Option::is_none")]
        blocked_reason: Option<String>,
        #[serde(rename = "classificationReasons", default)]
        classification_reasons: Vec<String>,
    },
    BrowserNavigation {
        url: String,
        #[serde(rename = "pageTitle")]
        page_title: String,
    },
    ComputerUseSession {
        #[serde(rename = "toolCallId", skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        #[serde(rename = "actionCount")]
        action_count: u64,
        #[serde(rename = "actionCounts")]
        action_counts: BTreeMap<String, u64>,
        #[serde(rename = "durationMs")]
        duration_ms: f64,
        #[serde(rename = "screenshotCount")]
        screenshot_count: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRecord {
    pub occurred_at_ms: u64,
    pub agent_id: String,
    pub turn_id: Option<String>,
    pub box_id: Option<String>,
    pub action: AuditAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    pub event_id: String,
    pub occurred_at_ms: u64,
    pub agent_id: String,
    pub turn_id: String,
    pub box_id: String,
    pub action: AuditAction,
}

pub fn is_backend_forwardable(record: &AuditRecord) -> bool {
    match &record.action {
        AuditAction::McpToolCall { transport, .. } => transport == "stdio",
        _ => true,
    }
}

fn iso_ms(ms: u64) -> String {
    let millis = i64::try_from(ms).unwrap_or(i64::MAX);
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(millis)
        .map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".into())
}

pub fn local_audit_jsonl_line(record: &AuditRecord, event_id: &str) -> String {
    let mut base = serde_json::Map::new();
    base.insert("ts".into(), serde_json::Value::String(iso_ms(record.occurred_at_ms)));
    base.insert("agentId".into(), serde_json::Value::String(record.agent_id.clone()));
    base.insert("eventId".into(), serde_json::Value::String(event_id.into()));
    if let Some(turn) = record.turn_id.as_ref().filter(|value| !value.is_empty()) {
        base.insert("turnId".into(), serde_json::Value::String(turn.clone()));
    }
    match &record.action {
        AuditAction::McpToolCall { tool_call_id, server_identifier, tool_name, transport, status, duration_ms, .. } => {
            base.insert("type".into(), "mcp_tool_call".into());
            base.insert("serverIdentifier".into(), server_identifier.clone().into());
            base.insert("toolName".into(), tool_name.clone().into());
            base.insert("toolCallId".into(), tool_call_id.clone().into());
            base.insert("transport".into(), transport.clone().into());
            base.insert("status".into(), status.clone().into());
            base.insert("durationMs".into(), serde_json::json!(duration_ms));
        }
        AuditAction::BrowserNavigation { url, page_title } => {
            base.insert("type".into(), "browser_navigation".into());
            base.insert("url".into(), url.clone().into());
            base.insert("pageTitle".into(), page_title.clone().into());
        }
        AuditAction::ComputerUseSession { tool_call_id, action_count, action_counts, duration_ms, screenshot_count } => {
            base.insert("type".into(), "computer_use_session".into());
            base.insert("toolCallId".into(), tool_call_id.clone().unwrap_or_default().into());
            base.insert("actionCount".into(), (*action_count).into());
            base.insert("actionCounts".into(), serde_json::to_value(action_counts).unwrap_or_default());
            base.insert("durationMs".into(), serde_json::json!(duration_ms));
            base.insert("screenshotCount".into(), (*screenshot_count).into());
        }
        AuditAction::ShellCommand { command, shell_kind, target, .. } => {
            base.insert("type".into(), "shell_command".into());
            base.insert("command".into(), command.clone().into());
            base.insert("shellKind".into(), shell_kind.clone().into());
            base.insert("target".into(), target.clone().into());
        }
    }
    format!("{}\n", serde_json::Value::Object(base))
}

type Enabled = Arc<dyn Fn() -> bool + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditSendError {
    pub message: String,
    pub retry_after_ms: Option<u64>,
}

impl AuditSendError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), retry_after_ms: None }
    }
}

pub fn flush_failure_backoff_ms(error: &AuditSendError) -> u64 {
    error.retry_after_ms.unwrap_or(0).max(AUDIT_FLUSH_FAILURE_BACKOFF_MS)
}

pub type SendBatch = Arc<dyn Fn(&[AuditEvent]) -> Result<(), AuditSendError> + Send + Sync>;
type Now = Arc<dyn Fn() -> u64 + Send + Sync>;
type Id = Arc<dyn Fn() -> String + Send + Sync>;
type AuditPath = Arc<dyn Fn(&str) -> PathBuf + Send + Sync>;

struct State {
    pending: Vec<AuditEvent>,
    loaded: bool,
    backoff_until_ms: u64,
    accepting: bool,
    stopped: bool,
}

pub struct SandActionAuditor {
    state: Arc<(Mutex<State>, Condvar)>,
    outbox_path: PathBuf,
    audit_path: AuditPath,
    enabled: Enabled,
    send_batch: SendBatch,
    now: Now,
    random_id: Id,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl SandActionAuditor {
    pub fn new(
        outbox_path: PathBuf,
        audit_path: AuditPath,
        enabled: Enabled,
        send_batch: SendBatch,
        flush_interval: Duration,
    ) -> Self {
        let state = Arc::new((Mutex::new(State {
            pending: Vec::new(), loaded: false, backoff_until_ms: 0, accepting: true, stopped: false,
        }), Condvar::new()));
        let now: Now = Arc::new(|| SystemTime::now().duration_since(UNIX_EPOCH)
            .unwrap_or_default().as_millis().try_into().unwrap_or(u64::MAX));
        let random_id: Id = Arc::new(|| Uuid::new_v4().to_string());
        let worker_state = Arc::clone(&state);
        let worker_outbox = outbox_path.clone();
        let worker_enabled = Arc::clone(&enabled);
        let worker_send = Arc::clone(&send_batch);
        let worker_now = Arc::clone(&now);
        let worker = thread::spawn(move || {
            let (lock, cv) = &*worker_state;
            let mut first = true;
            loop {
                let state = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                let (mut state, _) = cv.wait_timeout(state, flush_interval)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if state.stopped { break; }
                if first { first = false; continue; }
                flush_locked(&mut state, &worker_outbox, &*worker_enabled, &*worker_send, &*worker_now);
            }
        });
        Self { state, outbox_path, audit_path, enabled, send_batch, now, random_id, worker: Mutex::new(Some(worker)) }
    }

    #[cfg(test)]
    pub fn with_clock_and_id(mut self, now: Now, random_id: Id) -> Self {
        self.now = now;
        self.random_id = random_id;
        self
    }

    pub fn record(&self, record: AuditRecord) {
        let event_id = (self.random_id)();
        let path = (self.audit_path)(&record.agent_id);
        if let Some(parent) = path.parent() { let _ = fs::create_dir_all(parent); }
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = file.write_all(local_audit_jsonl_line(&record, &event_id).as_bytes());
        }
        if !is_backend_forwardable(&record) { return; }
        let (lock, _) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.accepting { return; }
        state.pending.push(AuditEvent {
            event_id,
            occurred_at_ms: record.occurred_at_ms,
            agent_id: record.agent_id,
            turn_id: record.turn_id.unwrap_or_default(),
            box_id: record.box_id.unwrap_or_default(),
            action: record.action,
        });
        if state.pending.len() > MAX_PENDING_AUDIT_EVENTS {
            let drop_count = state.pending.len() - MAX_PENDING_AUDIT_EVENTS;
            state.pending.drain(0..drop_count);
        }
    }

    pub fn flush(&self) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        flush_locked(&mut state, &self.outbox_path, &*self.enabled, &*self.send_batch, &*self.now);
    }

    pub fn dispose(&self) {
        {
            let (lock, cv) = &*self.state;
            let mut state = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.accepting = false;
            state.stopped = true;
            cv.notify_all();
        }
        if let Some(handle) = self.worker.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take() {
            let _ = handle.join();
        }
        self.flush();
        let (lock, _) = &*self.state;
        let state = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = persist(&self.outbox_path, &state.pending);
    }

    pub fn pending_len(&self) -> usize {
        let (lock, _) = &*self.state;
        lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).pending.len()
    }
}

impl Drop for SandActionAuditor {
    fn drop(&mut self) { self.dispose(); }
}

fn load(path: &Path) -> Vec<AuditEvent> {
    fs::read_to_string(path).ok()
        .and_then(|raw| serde_json::from_str::<Vec<AuditEvent>>(&raw).ok())
        .unwrap_or_default().into_iter().take(MAX_PENDING_AUDIT_EVENTS).collect()
}

fn persist(path: &Path, pending: &[AuditEvent]) -> io::Result<()> {
    if pending.is_empty() {
        return match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        };
    }
    let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "audit outbox has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let encoded = serde_json::to_vec(pending).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(&encoded)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(&temporary); }
    result
}

fn flush_locked(
    state: &mut State,
    outbox: &Path,
    enabled: &dyn Fn() -> bool,
    send: &dyn Fn(&[AuditEvent]) -> Result<(), AuditSendError>,
    now: &dyn Fn() -> u64,
) {
    if !state.loaded {
        let prior = load(outbox);
        let room = MAX_PENDING_AUDIT_EVENTS.saturating_sub(prior.len());
        let tail = state.pending.iter().rev().take(room).cloned().collect::<Vec<_>>();
        state.pending = prior.into_iter().chain(tail.into_iter().rev()).collect();
        state.loaded = true;
    }
    let current_ms = now();
    if current_ms < state.backoff_until_ms { return; }
    if state.pending.is_empty() || !enabled() {
        let _ = persist(outbox, &state.pending);
        return;
    }
    while !state.pending.is_empty() {
        let batch = state.pending.iter().take(MAX_AUDIT_BATCH_SIZE).cloned().collect::<Vec<_>>();
        match send(&batch) {
            Ok(()) => {
                let ids = batch.iter().map(|event| event.event_id.clone()).collect::<HashSet<_>>();
                state.pending.retain(|event| !ids.contains(&event.event_id));
            }
            Err(error) => {
                state.backoff_until_ms = current_ms.saturating_add(flush_failure_backoff_ms(&error));
                let _ = persist(outbox, &state.pending);
                return;
            }
        }
    }
    let _ = persist(outbox, &state.pending);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn record(transport: &str, index: u64) -> AuditRecord {
        AuditRecord {
            occurred_at_ms: 1_000 + index,
            agent_id: "agent-a".into(),
            turn_id: Some("turn-a".into()),
            box_id: Some("box-a".into()),
            action: AuditAction::McpToolCall {
                tool_call_id: format!("tool-{index}"),
                server_identifier: "server-a".into(),
                server_name: None,
                tool_name: "read".into(),
                transport: transport.into(),
                status: "ok".into(),
                duration_ms: 12.0,
            },
        }
    }

    #[test]
    fn only_stdio_mcp_is_backend_forwardable() {
        assert!(is_backend_forwardable(&record("stdio", 1)));
        assert!(!is_backend_forwardable(&record("streamable_http", 2)));
    }

    #[test]
    fn failed_send_uses_at_least_desktop_backoff() {
        assert_eq!(flush_failure_backoff_ms(&AuditSendError::new("offline")), 30_000);
        assert_eq!(flush_failure_backoff_ms(&AuditSendError {
            message: "rate limited".into(), retry_after_ms: Some(45_000)
        }), 45_000);
    }

    #[test]
    fn pending_queue_keeps_newest_two_thousand() {
        let root = tempfile::tempdir().unwrap();
        let local_root = root.path().to_path_buf();
        let auditor = SandActionAuditor::new(
            root.path().join("outbox.json"),
            Arc::new(move |_| local_root.join("audit.jsonl")),
            Arc::new(|| false),
            Arc::new(|_| Ok(())),
            Duration::from_secs(60),
        );
        for index in 0..(MAX_PENDING_AUDIT_EVENTS as u64 + 25) {
            auditor.record(record("stdio", index));
        }
        assert_eq!(auditor.pending_len(), MAX_PENDING_AUDIT_EVENTS);
    }

    #[test]
    fn outbox_reopens_and_flushes_in_bounded_batches() {
        let root = tempfile::tempdir().unwrap();
        let outbox = root.path().join("outbox.json");
        let local_path = root.path().join("audit.jsonl");
        let sent = Arc::new(Mutex::new(Vec::<usize>::new()));
        let sent_clone = Arc::clone(&sent);
        let ids = Arc::new(AtomicU64::new(0));
        let ids_clone = Arc::clone(&ids);
        let auditor = SandActionAuditor::new(
            outbox.clone(),
            Arc::new(move |_| local_path.clone()),
            Arc::new(|| true),
            Arc::new(move |events| {
                sent_clone.lock().unwrap().push(events.len());
                Err(AuditSendError::new("offline"))
            }),
            Duration::from_secs(60),
        ).with_clock_and_id(
            Arc::new(|| 5_000),
            Arc::new(move || format!("event-{}", ids_clone.fetch_add(1, Ordering::Relaxed))),
        );
        for index in 0..75 { auditor.record(record("stdio", index)); }
        auditor.flush();
        assert_eq!(*sent.lock().unwrap(), vec![50]);
        assert_eq!(auditor.pending_len(), 75);
        drop(auditor);

        let sent_after = Arc::new(Mutex::new(Vec::<usize>::new()));
        let sent_after_clone = Arc::clone(&sent_after);
        let local_path = root.path().join("audit-2.jsonl");
        let reopened = SandActionAuditor::new(
            outbox,
            Arc::new(move |_| local_path.clone()),
            Arc::new(|| true),
            Arc::new(move |events| {
                sent_after_clone.lock().unwrap().push(events.len());
                Ok(())
            }),
            Duration::from_secs(60),
        ).with_clock_and_id(Arc::new(|| 40_000), Arc::new(|| "unused".into()));
        reopened.flush();
        assert_eq!(*sent_after.lock().unwrap(), vec![50, 25]);
        assert_eq!(reopened.pending_len(), 0);
    }
}
