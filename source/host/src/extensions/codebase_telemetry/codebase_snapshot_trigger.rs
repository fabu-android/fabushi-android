// Direct Rust-to-Rust Android port from bhrumom/fabushi-desktop@371b72f1cd20de03dc4dff9a15ed761054968093.
// This module is platform-neutral protocol/state logic; Desktop process spawning is intentionally not copied.
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::thread;

pub const MAX_HANDLED_REASONS: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SnapshotReasonType { AgentRequestStart, AgentRequestEnd }

impl SnapshotReasonType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::AgentRequestStart => "AGENT_REQUEST_START",
            Self::AgentRequestEnd => "AGENT_REQUEST_END",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SnapshotReason {
    pub reason_type: SnapshotReasonType,
    pub request_id: String,
}

pub fn snapshot_reason_key(reason: &SnapshotReason) -> String {
    serde_json::to_string(&(reason.reason_type.as_str(), &reason.request_id))
        .expect("snapshot reason key serializes")
}

pub type SnapshotSession = Arc<dyn Fn(SnapshotReason) -> Result<(), String> + Send + Sync>;
pub type SnapshotSessionProvider = Arc<dyn Fn() -> Option<SnapshotSession> + Send + Sync>;
pub type SnapshotDebug = Arc<dyn Fn(&str) + Send + Sync>;
pub type SnapshotWarn = Arc<dyn Fn(&str, &str) + Send + Sync>;

struct TriggerState {
    handled: HashSet<String>,
    order: VecDeque<String>,
    failure_count: u64,
    next_failure_log_at: u64,
}
impl Default for TriggerState {
    fn default() -> Self {
        Self { handled: HashSet::new(), order: VecDeque::new(), failure_count: 0, next_failure_log_at: 1 }
    }
}

pub struct CodebaseSnapshotTrigger {
    get_session: SnapshotSessionProvider,
    debug: SnapshotDebug,
    warn: SnapshotWarn,
    state: Arc<Mutex<TriggerState>>,
}

impl CodebaseSnapshotTrigger {
    pub fn new(get_session: SnapshotSessionProvider, debug: SnapshotDebug, warn: SnapshotWarn) -> Self {
        Self { get_session, debug, warn, state: Arc::new(Mutex::new(TriggerState::default())) }
    }

    pub fn handle(&self, reason: SnapshotReason) {
        let Some(session) = (self.get_session)() else {
            (self.debug)(&format!("snapshot trigger dropped (telemetry inactive): {}", reason.reason_type.as_str()));
            return;
        };
        let key = snapshot_reason_key(&reason);
        {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            if state.handled.contains(&key) { return; }
            state.handled.insert(key.clone());
            state.order.push_back(key.clone());
            if state.handled.len() > MAX_HANDLED_REASONS {
                if let Some(oldest) = state.order.pop_front() { state.handled.remove(&oldest); }
            }
        }
        let state = Arc::clone(&self.state);
        let warn = Arc::clone(&self.warn);
        let _ = thread::Builder::new().name("codebase-snapshot-trigger".into()).spawn(move || {
            if let Err(error) = session(reason) {
                let mut state = state.lock().unwrap_or_else(|p| p.into_inner());
                state.handled.remove(&key);
                state.order.retain(|entry| entry != &key);
                state.failure_count = state.failure_count.saturating_add(1);
                if state.failure_count >= state.next_failure_log_at {
                    let n = state.failure_count;
                    state.next_failure_log_at = state.next_failure_log_at.saturating_mul(2).max(2);
                    drop(state);
                    warn(&format!("snapshot trigger failed (failure #{n})"), &error);
                }
            }
        });
    }
}
