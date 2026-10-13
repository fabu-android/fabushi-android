use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, Sender},
    Arc, Mutex, OnceLock, Weak,
};

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEvent {
    pub family: String,
    pub session_id: String,
    pub payload_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEventFailureReport {
    pub kind: String,
    pub topic: Option<String>,
    pub error_class: String,
}

pub type HostEventFailureReporter =
    Arc<dyn Fn(&HostEventFailureReport) + Send + Sync + 'static>;

static HOST_EVENT_FAILURE_REPORTER: OnceLock<Mutex<Option<HostEventFailureReporter>>> =
    OnceLock::new();

pub fn pin_host_event_failure_reporter(reporter: Option<HostEventFailureReporter>) {
    let slot = HOST_EVENT_FAILURE_REPORTER.get_or_init(|| Mutex::new(None));
    if let Ok(mut current) = slot.lock() {
        *current = reporter;
    }
}

fn report_pinned_host_event_failure(report: &HostEventFailureReport) {
    let reporter = HOST_EVENT_FAILURE_REPORTER
        .get()
        .and_then(|slot| slot.lock().ok().and_then(|current| current.clone()));
    if let Some(reporter) = reporter {
        reporter(report);
    }
}

type HostEventListener = Arc<dyn Fn(&HostEvent) + Send + Sync + 'static>;
type HostCapabilityHandler =
    Arc<dyn Fn(&Value) -> Result<(), String> + Send + Sync + 'static>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostEventFailureMode {
    Continue,
    Reject,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEventHandlerError {
    pub topic: String,
    pub message: String,
}

impl fmt::Display for HostEventHandlerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Host event handler failed for {}: {}",
            self.topic, self.message
        )
    }
}

impl std::error::Error for HostEventHandlerError {}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SubscriptionKind {
    Listener,
    Topic(String),
}

struct HostEventBusState {
    stream_subscribers: Mutex<Vec<Sender<HostEvent>>>,
    listeners: Mutex<BTreeMap<u64, HostEventListener>>,
    topic_handlers: Mutex<HashMap<String, BTreeMap<u64, HostCapabilityHandler>>>,
    next_id: AtomicU64,
    report_failure: HostEventFailureReporter,
}

#[derive(Clone)]
pub struct HostEventBus {
    state: Arc<HostEventBusState>,
}

impl Default for HostEventBus {
    fn default() -> Self {
        Self::with_failure_reporter(Arc::new(report_pinned_host_event_failure))
    }
}

impl HostEventBus {
    pub fn with_failure_reporter(report_failure: HostEventFailureReporter) -> Self {
        Self {
            state: Arc::new(HostEventBusState {
                stream_subscribers: Mutex::new(Vec::new()),
                listeners: Mutex::new(BTreeMap::new()),
                topic_handlers: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(1),
                report_failure,
            }),
        }
    }

    pub fn subscribe(&self) -> Receiver<HostEvent> {
        let (sender, receiver) = mpsc::channel();
        self.state
            .stream_subscribers
            .lock()
            .expect("host event subscribers lock poisoned")
            .push(sender);
        receiver
    }

    /// Shipping stream/listener fanout. A disconnected stream subscriber is normal disposal,
    /// while a callback panic is isolated and reported without breaking other listeners.
    pub fn emit(&self, event: HostEvent) {
        self.state
            .stream_subscribers
            .lock()
            .expect("host event subscribers lock poisoned")
            .retain(|subscriber| subscriber.send(event.clone()).is_ok());

        let listeners = self
            .state
            .listeners
            .lock()
            .map(|listeners| listeners.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for listener in listeners {
            if catch_unwind(AssertUnwindSafe(|| listener(&event))).is_err() {
                (self.state.report_failure)(&HostEventFailureReport {
                    kind: "listener_failed".into(),
                    topic: None,
                    error_class: "panic".into(),
                });
            }
        }
    }

    pub fn subscriber_count(&self) -> usize {
        self.state
            .stream_subscribers
            .lock()
            .map(|subscribers| subscribers.len())
            .unwrap_or_default()
    }

    pub fn subscribe_listener<F>(&self, listener: F) -> HostEventSubscription
    where
        F: Fn(&HostEvent) + Send + Sync + 'static,
    {
        let id = self.state.next_id.fetch_add(1, Ordering::Relaxed);
        self.state
            .listeners
            .lock()
            .expect("host event listener registry poisoned")
            .insert(id, Arc::new(listener));
        HostEventSubscription {
            state: Arc::downgrade(&self.state),
            id,
            kind: SubscriptionKind::Listener,
            active: true,
        }
    }

    pub fn on<F>(&self, topic: impl Into<String>, handler: F) -> HostEventSubscription
    where
        F: Fn(&Value) -> Result<(), String> + Send + Sync + 'static,
    {
        let topic = topic.into();
        let id = self.state.next_id.fetch_add(1, Ordering::Relaxed);
        self.state
            .topic_handlers
            .lock()
            .expect("host capability handler registry poisoned")
            .entry(topic.clone())
            .or_default()
            .insert(id, Arc::new(handler));
        HostEventSubscription {
            state: Arc::downgrade(&self.state),
            id,
            kind: SubscriptionKind::Topic(topic),
            active: true,
        }
    }

    /// Capability-topic dispatch matching Desktop Continue/Reject failure semantics.
    /// Handler failures are always reported; Reject additionally returns the first failure.
    pub fn emit_topic(
        &self,
        topic: &str,
        payload: &Value,
        failure_mode: HostEventFailureMode,
    ) -> Result<(), HostEventHandlerError> {
        let handlers = self
            .state
            .topic_handlers
            .lock()
            .ok()
            .and_then(|topics| topics.get(topic).cloned())
            .map(|handlers| handlers.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        for handler in handlers {
            let outcome = catch_unwind(AssertUnwindSafe(|| handler(payload)));
            let failure = match outcome {
                Ok(Ok(())) => None,
                Ok(Err(message)) => Some((message, "Error")),
                Err(_) => Some(("handler panicked".to_string(), "panic")),
            };
            let Some((message, error_class)) = failure else {
                continue;
            };
            (self.state.report_failure)(&HostEventFailureReport {
                kind: "subscriber_failed".into(),
                topic: Some(topic.to_string()),
                error_class: error_class.into(),
            });
            if failure_mode == HostEventFailureMode::Reject {
                return Err(HostEventHandlerError {
                    topic: topic.to_string(),
                    message,
                });
            }
        }
        Ok(())
    }
}

pub struct HostEventSubscription {
    state: Weak<HostEventBusState>,
    id: u64,
    kind: SubscriptionKind,
    active: bool,
}

impl HostEventSubscription {
    pub fn unsubscribe(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        let Some(state) = self.state.upgrade() else {
            return;
        };
        match &self.kind {
            SubscriptionKind::Listener => {
                if let Ok(mut listeners) = state.listeners.lock() {
                    listeners.remove(&self.id);
                }
            }
            SubscriptionKind::Topic(topic) => {
                if let Ok(mut topics) = state.topic_handlers.lock() {
                    if let Some(handlers) = topics.get_mut(topic) {
                        handlers.remove(&self.id);
                        if handlers.is_empty() {
                            topics.remove(topic);
                        }
                    }
                }
            }
        }
    }
}

impl Drop for HostEventSubscription {
    fn drop(&mut self) {
        self.unsubscribe();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    fn event() -> HostEvent {
        HostEvent {
            family: "transcript".into(),
            session_id: "s".into(),
            payload_json: "{}".into(),
        }
    }

    #[test]
    fn fanout_is_not_competing_consumption() {
        let bus = HostEventBus::default();
        let a = bus.subscribe();
        let b = bus.subscribe();
        let event = event();
        bus.emit(event.clone());
        assert_eq!(a.recv().unwrap(), event);
        assert_eq!(b.recv().unwrap(), event);
    }

    #[test]
    fn listener_panic_is_reported_and_does_not_break_other_listener() {
        let failures = Arc::new(Mutex::new(Vec::new()));
        let failure_sink = Arc::clone(&failures);
        let bus = HostEventBus::with_failure_reporter(Arc::new(move |report| {
            failure_sink.lock().unwrap().push(report.clone());
        }));
        let delivered = Arc::new(AtomicUsize::new(0));
        let delivered_sink = Arc::clone(&delivered);
        let _panic = bus.subscribe_listener(|_| panic!("boom"));
        let _healthy = bus.subscribe_listener(move |_| {
            delivered_sink.fetch_add(1, AtomicOrdering::SeqCst);
        });

        bus.emit(event());

        assert_eq!(delivered.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(
            failures.lock().unwrap().as_slice(),
            &[HostEventFailureReport {
                kind: "listener_failed".into(),
                topic: None,
                error_class: "panic".into(),
            }]
        );
    }

    #[test]
    fn topic_failure_continue_reports_but_reject_returns_first_error() {
        let failures = Arc::new(Mutex::new(Vec::new()));
        let failure_sink = Arc::clone(&failures);
        let bus = HostEventBus::with_failure_reporter(Arc::new(move |report| {
            failure_sink.lock().unwrap().push(report.clone());
        }));
        let _failed = bus.on("tool", |_| Err("denied".into()));
        let _ok = bus.on("tool", |_| Ok(()));

        assert!(bus
            .emit_topic("tool", &serde_json::json!({}), HostEventFailureMode::Continue)
            .is_ok());
        let rejected = bus
            .emit_topic("tool", &serde_json::json!({}), HostEventFailureMode::Reject)
            .unwrap_err();
        assert_eq!(rejected.topic, "tool");
        assert_eq!(rejected.message, "denied");
        assert_eq!(failures.lock().unwrap().len(), 2);
    }

    #[test]
    fn subscription_drop_unregisters_listener_and_topic_handler() {
        let bus = HostEventBus::default();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_listener = Arc::clone(&calls);
        {
            let _listener = bus.subscribe_listener(move |_| {
                calls_listener.fetch_add(1, AtomicOrdering::SeqCst);
            });
            bus.emit(event());
        }
        bus.emit(event());
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);

        let topic_calls = Arc::new(AtomicUsize::new(0));
        let topic_sink = Arc::clone(&topic_calls);
        {
            let _topic = bus.on("tool", move |_| {
                topic_sink.fetch_add(1, AtomicOrdering::SeqCst);
                Ok(())
            });
            bus.emit_topic("tool", &serde_json::json!({}), HostEventFailureMode::Continue)
                .unwrap();
        }
        bus.emit_topic("tool", &serde_json::json!({}), HostEventFailureMode::Continue)
            .unwrap();
        assert_eq!(topic_calls.load(AtomicOrdering::SeqCst), 1);
    }
}
