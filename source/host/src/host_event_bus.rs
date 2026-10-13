use std::{
    collections::{BTreeMap, HashMap},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex, Weak,
    },
};

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEvent {
    pub family: String,
    pub session_id: String,
    pub payload_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEventFailure {
    pub kind: &'static str,
    pub topic: Option<String>,
    pub error_class: &'static str,
}

pub type HostEventFailureReporter = Arc<dyn Fn(&HostEventFailure) + Send + Sync + 'static>;
type HostEventListener = Arc<dyn Fn(&HostEvent) + Send + Sync + 'static>;
type HostCapabilityHandler =
    Arc<dyn Fn(&Value) -> Result<(), String> + Send + Sync + 'static>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostEventFailureMode {
    Continue,
    Reject,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("Host event handler failed for {topic}: {message}")]
pub struct HostEventHandlerError {
    pub topic: String,
    pub message: String,
}

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
        Self::with_failure_reporter(Arc::new(|_| {}))
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

    /// Existing Android stream fan-out plus Desktop-equivalent callback listeners.
    /// One faulty listener is fenced and cannot suppress delivery to later listeners.
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
                (self.state.report_failure)(&HostEventFailure {
                    kind: "listener_failed",
                    topic: None,
                    error_class: "panic",
                });
            }
        }
    }

    pub fn subscriber_count(&self) -> usize {
        self.state
            .stream_subscribers
            .lock()
            .expect("host event subscribers lock poisoned")
            .len()
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
            (self.state.report_failure)(&HostEventFailure {
                kind: "subscriber_failed",
                topic: Some(topic.to_string()),
                error_class,
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
    pub fn unsubscribe(mut self) {
        self.remove();
    }

    fn remove(&mut self) {
        if !self.active {
            return;
        }
        let Some(state) = self.state.upgrade() else {
            self.active = false;
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
                    let remove_topic = if let Some(handlers) = topics.get_mut(topic) {
                        handlers.remove(&self.id);
                        handlers.is_empty()
                    } else {
                        false
                    };
                    if remove_topic {
                        topics.remove(topic);
                    }
                }
            }
        }
        self.active = false;
    }
}

impl Drop for HostEventSubscription {
    fn drop(&mut self) {
        self.remove();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    #[test]
    fn fanout_is_not_competing_consumption() {
        let bus = HostEventBus::default();
        let a = bus.subscribe();
        let b = bus.subscribe();
        let event = HostEvent {
            family: "transcript".into(),
            session_id: "s".into(),
            payload_json: "{}".into(),
        };
        bus.emit(event.clone());
        assert_eq!(a.recv().unwrap(), event);
        assert_eq!(b.recv().unwrap(), event);
    }

    #[test]
    fn listener_panic_is_fenced_and_later_listener_still_runs() {
        let failures = Arc::new(Mutex::new(Vec::<HostEventFailure>::new()));
        let sink = Arc::clone(&failures);
        let bus = HostEventBus::with_failure_reporter(Arc::new(move |failure| {
            sink.lock().unwrap().push(failure.clone());
        }));
        let delivered = Arc::new(AtomicUsize::new(0));
        let _panic = bus.subscribe_listener(|_| panic!("boom"));
        let delivered_ref = Arc::clone(&delivered);
        let _healthy = bus.subscribe_listener(move |_| {
            delivered_ref.fetch_add(1, AtomicOrdering::SeqCst);
        });

        bus.emit(HostEvent {
            family: "agent".into(),
            session_id: "session-1".into(),
            payload_json: "{}".into(),
        });

        assert_eq!(delivered.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(
            failures.lock().unwrap().as_slice(),
            &[HostEventFailure {
                kind: "listener_failed",
                topic: None,
                error_class: "panic",
            }]
        );
    }

    #[test]
    fn topic_failure_mode_controls_rejection_without_hiding_failures() {
        let failures = Arc::new(Mutex::new(Vec::<HostEventFailure>::new()));
        let sink = Arc::clone(&failures);
        let bus = HostEventBus::with_failure_reporter(Arc::new(move |failure| {
            sink.lock().unwrap().push(failure.clone());
        }));
        let calls = Arc::new(AtomicUsize::new(0));
        let first_calls = Arc::clone(&calls);
        let _first = bus.on("tool.result", move |_| {
            first_calls.fetch_add(1, AtomicOrdering::SeqCst);
            Err("rejected".into())
        });
        let second_calls = Arc::clone(&calls);
        let _second = bus.on("tool.result", move |_| {
            second_calls.fetch_add(1, AtomicOrdering::SeqCst);
            Ok(())
        });

        assert!(bus
            .emit_topic(
                "tool.result",
                &serde_json::json!({"ok": true}),
                HostEventFailureMode::Continue,
            )
            .is_ok());
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 2);

        calls.store(0, AtomicOrdering::SeqCst);
        let error = bus
            .emit_topic(
                "tool.result",
                &serde_json::json!({"ok": true}),
                HostEventFailureMode::Reject,
            )
            .unwrap_err();
        assert_eq!(error.topic, "tool.result");
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(failures.lock().unwrap().len(), 2);
    }

    #[test]
    fn dropping_subscription_unregisters_callback_and_topic_handler() {
        let bus = HostEventBus::default();
        let listener_calls = Arc::new(AtomicUsize::new(0));
        {
            let listener_calls_ref = Arc::clone(&listener_calls);
            let _subscription = bus.subscribe_listener(move |_| {
                listener_calls_ref.fetch_add(1, AtomicOrdering::SeqCst);
            });
        }
        bus.emit(HostEvent {
            family: "agent".into(),
            session_id: "session-1".into(),
            payload_json: "{}".into(),
        });
        assert_eq!(listener_calls.load(AtomicOrdering::SeqCst), 0);

        let topic_calls = Arc::new(AtomicUsize::new(0));
        {
            let topic_calls_ref = Arc::clone(&topic_calls);
            let _subscription = bus.on("agent.stop", move |_| {
                topic_calls_ref.fetch_add(1, AtomicOrdering::SeqCst);
                Ok(())
            });
        }
        bus.emit_topic(
            "agent.stop",
            &serde_json::json!({}),
            HostEventFailureMode::Continue,
        )
        .unwrap();
        assert_eq!(topic_calls.load(AtomicOrdering::SeqCst), 0);
    }
}
