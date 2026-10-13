use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

pub const XUSER_RELAY_ERROR_BACKOFF_MS: u64 = 30_000;

pub trait XuserRelayPoller: Send + Sync {
    fn poll(&self, ack_ids: &[String]) -> Result<Value, String>;
}

pub type XuserRelayEventHandler = Arc<dyn Fn(&Value) -> Result<bool, String> + Send + Sync>;

#[derive(Debug, Default)]
struct RelayRuntimeState {
    started: bool,
    is_ticking: bool,
    backoff_until_ms: u64,
    pending_ack_ids: Vec<String>,
}

/// Android-port of the platform-neutral shared-room relay drain state machine.
///
/// Authentication and HTTP transport deliberately stay outside this type so the Android account
/// runtime remains the single credential owner. An event is acknowledged only after the canonical
/// sharing handler accepts it; duplicate event ids are carried forward without redispatch.
pub struct XuserRelayRuntime {
    poller: Arc<dyn XuserRelayPoller>,
    on_event: XuserRelayEventHandler,
    state: Mutex<RelayRuntimeState>,
}

impl XuserRelayRuntime {
    pub fn new(poller: Arc<dyn XuserRelayPoller>, on_event: XuserRelayEventHandler) -> Self {
        Self {
            poller,
            on_event,
            state: Mutex::new(RelayRuntimeState::default()),
        }
    }

    pub fn start(&self) {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .started = true;
    }

    pub fn stop(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.started = false;
        state.is_ticking = false;
        state.backoff_until_ms = 0;
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

    pub fn tick(&self, now_ms: u64) -> Result<bool, String> {
        let pending = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "xuser relay runtime lock poisoned".to_string())?;
            if !state.started || state.is_ticking || now_ms < state.backoff_until_ms {
                return Ok(false);
            }
            state.is_ticking = true;
            state.pending_ack_ids.clone()
        };

        let payload = match self.poller.poll(&pending) {
            Ok(payload) => payload,
            Err(error) => {
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| "xuser relay runtime lock poisoned".to_string())?;
                state.is_ticking = false;
                state.backoff_until_ms = now_ms.saturating_add(XUSER_RELAY_ERROR_BACKOFF_MS);
                return Err(error);
            }
        };

        let previously_acked = pending.iter().cloned().collect::<BTreeSet<_>>();
        let mut next_ack_ids = Vec::new();
        let mut next_ack_set = BTreeSet::new();
        if let Some(events) = payload.get("events").and_then(Value::as_array) {
            for event in events {
                let Some(record) = event.as_object() else {
                    continue;
                };
                let Some(id) = record
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    continue;
                };
                if record
                    .get("kind")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .is_none_or(str::is_empty)
                {
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

        let mut state = self
            .state
            .lock()
            .map_err(|_| "xuser relay runtime lock poisoned".to_string())?;
        state.pending_ack_ids = next_ack_ids;
        state.is_ticking = false;
        state.backoff_until_ms = 0;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakePoller {
        calls: AtomicUsize,
        fail_first: bool,
    }

    impl XuserRelayPoller for FakePoller {
        fn poll(&self, ack_ids: &[String]) -> Result<Value, String> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_first && call == 0 {
                return Err("offline".into());
            }
            if call == 0 || (self.fail_first && call == 1) {
                Ok(json!({
                    "events":[
                        {"id":"e1","kind":"room-entry"},
                        {"id":"e1","kind":"room-entry"},
                        {"id":"e2","kind":"turn-request"},
                        {"id":"bad-no-kind"}
                    ]
                }))
            } else {
                Ok(json!({
                    "events": ack_ids
                        .iter()
                        .map(|id| json!({"id":id,"kind":"already-acked"}))
                        .collect::<Vec<_>>()
                }))
            }
        }
    }

    #[test]
    fn successful_handler_controls_ack_and_duplicate_ids_are_not_redispatched() {
        let poller = Arc::new(FakePoller {
            calls: AtomicUsize::new(0),
            fail_first: false,
        });
        let handled = Arc::new(Mutex::new(Vec::<String>::new()));
        let handled_for_callback = Arc::clone(&handled);
        let runtime = XuserRelayRuntime::new(
            poller,
            Arc::new(move |event| {
                let id = event["id"].as_str().unwrap().to_string();
                handled_for_callback.lock().unwrap().push(id.clone());
                Ok(id == "e1")
            }),
        );
        runtime.start();
        assert!(runtime.tick(1_000).unwrap());
        assert_eq!(runtime.pending_ack_ids(), vec!["e1"]);
        assert_eq!(*handled.lock().unwrap(), vec!["e1", "e2"]);
        assert!(runtime.tick(1_001).unwrap());
        assert_eq!(*handled.lock().unwrap(), vec!["e1", "e2"]);
    }

    #[test]
    fn poll_error_enters_backoff_without_losing_pending_ack_state() {
        let poller = Arc::new(FakePoller {
            calls: AtomicUsize::new(0),
            fail_first: true,
        });
        let runtime = XuserRelayRuntime::new(poller, Arc::new(|_| Ok(true)));
        runtime.start();
        assert_eq!(runtime.tick(10).unwrap_err(), "offline");
        assert_eq!(runtime.backoff_until_ms(), 10 + XUSER_RELAY_ERROR_BACKOFF_MS);
        assert!(!runtime.tick(11).unwrap());
        assert!(runtime
            .tick(10 + XUSER_RELAY_ERROR_BACKOFF_MS)
            .unwrap());
        assert_eq!(runtime.pending_ack_ids(), vec!["e1", "e2"]);
    }

    #[test]
    fn stopped_runtime_never_polls() {
        let poller = Arc::new(FakePoller {
            calls: AtomicUsize::new(0),
            fail_first: false,
        });
        let runtime = XuserRelayRuntime::new(poller.clone(), Arc::new(|_| Ok(true)));
        assert!(!runtime.tick(1).unwrap());
        runtime.start();
        runtime.stop();
        assert!(!runtime.tick(2).unwrap());
        assert_eq!(poller.calls.load(Ordering::SeqCst), 0);
    }
}
