use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{
    Arc, Mutex, OnceLock,
    mpsc::{self, Receiver, Sender},
};

use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptMutation {
    pub agent_id: String,
    pub sequence: u64,
    pub kind: String,
}

#[derive(Clone, Default)]
pub struct TranscriptMutationBus {
    subscribers: Arc<Mutex<Vec<Sender<TranscriptMutation>>>>,
}

impl TranscriptMutationBus {
    pub fn subscribe(&self) -> Receiver<TranscriptMutation> {
        let (tx, rx) = mpsc::channel();
        self.subscribers.lock().unwrap().push(tx);
        rx
    }

    pub fn publish(&self, event: TranscriptMutation) {
        self.subscribers
            .lock()
            .unwrap()
            .retain(|tx| tx.send(event.clone()).is_ok());
    }
}

/// Desktop-compatible Session mutation payload. This is an event projection only; canonical
/// transcript/session state remains owned by the Rust Host stores.
pub type TranscriptMutationPayload = Map<String, Value>;
type TranscriptMutationListener =
    Arc<dyn Fn(&TranscriptMutationPayload) + Send + Sync + 'static>;

fn payload_listeners() -> &'static Mutex<BTreeMap<u64, TranscriptMutationListener>> {
    static LISTENERS: OnceLock<Mutex<BTreeMap<u64, TranscriptMutationListener>>> = OnceLock::new();
    LISTENERS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn next_listener_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug)]
pub struct TranscriptMutationSubscription {
    id: u64,
    active: bool,
}

impl TranscriptMutationSubscription {
    pub fn unsubscribe(mut self) {
        self.remove();
    }

    fn remove(&mut self) {
        if !self.active {
            return;
        }
        if let Ok(mut listeners) = payload_listeners().lock() {
            listeners.remove(&self.id);
        }
        self.active = false;
    }
}

impl Drop for TranscriptMutationSubscription {
    fn drop(&mut self) {
        self.remove();
    }
}

pub fn subscribe_transcript_mutations<F>(listener: F) -> TranscriptMutationSubscription
where
    F: Fn(&TranscriptMutationPayload) + Send + Sync + 'static,
{
    let id = next_listener_id();
    payload_listeners()
        .lock()
        .expect("transcript mutation listener registry poisoned")
        .insert(id, Arc::new(listener));
    TranscriptMutationSubscription { id, active: true }
}

pub fn publish_transcript_mutation(mutation: &TranscriptMutationPayload) {
    let snapshot = payload_listeners()
        .lock()
        .map(|listeners| listeners.values().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    for listener in snapshot {
        let _ = catch_unwind(AssertUnwindSafe(|| listener(mutation)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_bus_drops_closed_subscribers_without_affecting_live_ones() {
        let bus = TranscriptMutationBus::default();
        let closed = bus.subscribe();
        let live = bus.subscribe();
        drop(closed);
        let event = TranscriptMutation {
            agent_id: "agent-a".into(),
            sequence: 7,
            kind: "entries-upserted".into(),
        };
        bus.publish(event.clone());
        assert_eq!(live.recv().unwrap(), event);
    }

    #[test]
    fn payload_subscription_is_explicitly_disposable() {
        let received = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = Arc::clone(&received);
        let subscription = subscribe_transcript_mutations(move |mutation| {
            if let Some(kind) = mutation.get("kind").and_then(Value::as_str) {
                sink.lock().unwrap().push(kind.to_string());
            }
        });
        let mut first = Map::new();
        first.insert("kind".into(), Value::String("agent-needs-reindex".into()));
        publish_transcript_mutation(&first);
        assert_eq!(
            received.lock().unwrap().as_slice(),
            &["agent-needs-reindex".to_string()]
        );

        subscription.unsubscribe();
        let mut second = Map::new();
        second.insert("kind".into(), Value::String("entries-upserted".into()));
        publish_transcript_mutation(&second);
        assert_eq!(received.lock().unwrap().len(), 1);
    }
}
