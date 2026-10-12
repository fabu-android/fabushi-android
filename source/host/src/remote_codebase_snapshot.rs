use fabushi_android_shared::ExecutionRequest;
use serde::Serialize;
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::thread;

pub const REMOTE_CODEBASE_SNAPSHOT_CAPABILITY: &str = "codebase.snapshot";
pub const REMOTE_CODEBASE_SNAPSHOT_TIMEOUT_MS: u64 = 120_000;
pub const MAX_HANDLED_REASONS: usize = 1_000;
const MAX_IDENTITY_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SnapshotReasonType {
    AgentRequestStart,
    AgentRequestEnd,
}

impl SnapshotReasonType {
    pub const fn as_str(self) -> &'static str {
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

impl SnapshotReason {
    pub fn validate(&self) -> Result<(), String> {
        validate_identity("snapshot request", &self.request_id)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotWireReason<'a> {
    #[serde(rename = "type")]
    reason_type: &'a str,
    request_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotWireRequest<'a> {
    version: u32,
    reason: SnapshotWireReason<'a>,
}

pub fn snapshot_reason_key(reason: &SnapshotReason) -> Result<String, String> {
    reason.validate()?;
    serde_json::to_string(&(reason.reason_type.as_str(), &reason.request_id))
        .map_err(|error| format!("encode snapshot reason identity: {error}"))
}

pub fn snapshot_execution_request(
    operation_id: impl Into<String>,
    reason: &SnapshotReason,
) -> Result<ExecutionRequest, String> {
    reason.validate()?;
    let operation_id = operation_id.into();
    validate_identity("snapshot operation", &operation_id)?;
    let input_json = serde_json::to_string(&SnapshotWireRequest {
        version: 1,
        reason: SnapshotWireReason {
            reason_type: reason.reason_type.as_str(),
            request_id: &reason.request_id,
        },
    })
    .map_err(|error| format!("encode snapshot request: {error}"))?;
    let request = ExecutionRequest {
        operation_id,
        capability_id: REMOTE_CODEBASE_SNAPSHOT_CAPABILITY.into(),
        input_json,
        timeout_ms: REMOTE_CODEBASE_SNAPSHOT_TIMEOUT_MS,
    };
    request
        .validate()
        .map_err(|error| format!("invalid codebase snapshot execution request: {error:?}"))?;
    Ok(request)
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
        Self {
            handled: HashSet::new(),
            order: VecDeque::new(),
            failure_count: 0,
            next_failure_log_at: 1,
        }
    }
}

pub struct CodebaseSnapshotTrigger {
    get_session: SnapshotSessionProvider,
    debug: SnapshotDebug,
    warn: SnapshotWarn,
    state: Arc<Mutex<TriggerState>>,
}

impl CodebaseSnapshotTrigger {
    pub fn new(
        get_session: SnapshotSessionProvider,
        debug: SnapshotDebug,
        warn: SnapshotWarn,
    ) -> Self {
        Self {
            get_session,
            debug,
            warn,
            state: Arc::new(Mutex::new(TriggerState::default())),
        }
    }

    pub fn handle(&self, reason: SnapshotReason) -> Result<(), String> {
        reason.validate()?;
        let Some(session) = (self.get_session)() else {
            (self.debug)(&format!(
                "snapshot trigger dropped (telemetry inactive): {}",
                reason.reason_type.as_str()
            ));
            return Ok(());
        };
        let key = snapshot_reason_key(&reason)?;
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.handled.contains(&key) {
                return Ok(());
            }
            state.handled.insert(key.clone());
            state.order.push_back(key.clone());
            if state.handled.len() > MAX_HANDLED_REASONS {
                if let Some(oldest) = state.order.pop_front() {
                    state.handled.remove(&oldest);
                }
            }
        }

        let state = Arc::clone(&self.state);
        let warn = Arc::clone(&self.warn);
        thread::Builder::new()
            .name("android-codebase-snapshot-trigger".into())
            .spawn(move || {
                if let Err(error) = session(reason) {
                    let mut state = state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.handled.remove(&key);
                    state.order.retain(|entry| entry != &key);
                    state.failure_count = state.failure_count.saturating_add(1);
                    if state.failure_count >= state.next_failure_log_at {
                        let count = state.failure_count;
                        state.next_failure_log_at =
                            state.next_failure_log_at.saturating_mul(2).max(2);
                        drop(state);
                        warn(
                            &format!("snapshot trigger failed (failure #{count})"),
                            &error,
                        );
                    }
                }
            })
            .map_err(|error| format!("spawn codebase snapshot trigger: {error}"))?;
        Ok(())
    }
}

fn validate_identity(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.len() > MAX_IDENTITY_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(format!("{label} identity is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    fn reason(request_id: &str) -> SnapshotReason {
        SnapshotReason {
            reason_type: SnapshotReasonType::AgentRequestStart,
            request_id: request_id.into(),
        }
    }

    #[test]
    fn remote_request_uses_dedicated_bounded_capability() {
        let request =
            snapshot_execution_request("snapshot-op-1", &reason("request-1")).expect("request");
        assert_eq!(request.capability_id, REMOTE_CODEBASE_SNAPSHOT_CAPABILITY);
        assert_eq!(request.timeout_ms, REMOTE_CODEBASE_SNAPSHOT_TIMEOUT_MS);
        assert!(request.input_json.contains("AGENT_REQUEST_START"));
        assert!(request.input_json.contains("request-1"));
        assert!(!request.input_json.contains("Shell"));
    }

    #[test]
    fn duplicate_reason_is_dispatched_once_after_success() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = mpsc::channel();
        let session: SnapshotSession = {
            let calls = Arc::clone(&calls);
            Arc::new(move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                tx.send(()).expect("signal");
                Ok(())
            })
        };
        let trigger = CodebaseSnapshotTrigger::new(
            Arc::new(move || Some(Arc::clone(&session))),
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
        );
        trigger.handle(reason("same-request")).unwrap();
        rx.recv_timeout(Duration::from_secs(1)).expect("first dispatch");
        trigger.handle(reason("same-request")).unwrap();
        std::thread::sleep(Duration::from_millis(25));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn failed_reason_is_released_for_retry_and_failure_logs_are_sparse() {
        let calls = Arc::new(AtomicUsize::new(0));
        let warnings = Arc::new(Mutex::new(Vec::<String>::new()));
        let (tx, rx) = mpsc::channel();
        let session: SnapshotSession = {
            let calls = Arc::clone(&calls);
            Arc::new(move |_| {
                let count = calls.fetch_add(1, Ordering::SeqCst) + 1;
                tx.send(count).expect("signal");
                Err(format!("failure-{count}"))
            })
        };
        let trigger = CodebaseSnapshotTrigger::new(
            Arc::new(move || Some(Arc::clone(&session))),
            Arc::new(|_| {}),
            {
                let warnings = Arc::clone(&warnings);
                Arc::new(move |message, _| warnings.lock().unwrap().push(message.to_string()))
            },
        );
        for expected in 1..=3 {
            trigger.handle(reason("retryable-request")).unwrap();
            assert_eq!(rx.recv_timeout(Duration::from_secs(1)).unwrap(), expected);
            for _ in 0..100 {
                if snapshot_reason_key(&reason("retryable-request"))
                    .ok()
                    .is_some_and(|key| {
                        !trigger
                            .state
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .handled
                            .contains(&key)
                    })
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        let warnings = warnings.lock().unwrap();
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("#1"));
        assert!(warnings[1].contains("#2"));
    }

    #[test]
    fn inactive_telemetry_does_not_consume_dedupe_identity() {
        let debug_count = Arc::new(AtomicUsize::new(0));
        let trigger = CodebaseSnapshotTrigger::new(
            Arc::new(|| None),
            {
                let debug_count = Arc::clone(&debug_count);
                Arc::new(move |_| {
                    debug_count.fetch_add(1, Ordering::SeqCst);
                })
            },
            Arc::new(|_, _| {}),
        );
        trigger.handle(reason("request-1")).unwrap();
        trigger.handle(reason("request-1")).unwrap();
        assert_eq!(debug_count.load(Ordering::SeqCst), 2);
        assert!(trigger.state.lock().unwrap().handled.is_empty());
    }
}
