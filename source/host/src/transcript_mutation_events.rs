use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{
    Arc, Mutex, OnceLock,
    mpsc::{self, Receiver, Sender},
};

use serde_json::{Map, Value};

/// Android typed mutation channel retained for Coordinator/session consumers that need a compact
/// sequence-bearing event. This remains source-compatible with the pre-existing Android owner.
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

/// Full-fidelity mutation envelope used by the Desktop-equivalent Agent DB/session runtime.
/// Keeping this registry next to the typed Android bus avoids a second persistence owner while
/// allowing incremental consumers to subscribe without collapsing entry payloads into a string.
pub type TranscriptMutationEnvelope = Map<String, Value>;
type TranscriptMutationListener =
    Arc<dyn Fn(&TranscriptMutationEnvelope) + Send + Sync + 'static>;

fn listeners() -> &'static Mutex<BTreeMap<u64, TranscriptMutationListener>> {
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
        if let Ok(mut values) = listeners().lock() {
            values.remove(&self.id);
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
    F: Fn(&TranscriptMutationEnvelope) + Send + Sync + 'static,
{
    let id = next_listener_id();
    listeners()
        .lock()
        .expect("transcript mutation listener registry poisoned")
        .insert(id, Arc::new(listener));
    TranscriptMutationSubscription { id, active: true }
}

pub fn publish_transcript_mutation(mutation: &TranscriptMutationEnvelope) {
    let snapshot = listeners()
        .lock()
        .map(|values| values.values().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    for listener in snapshot {
        let _ = catch_unwind(AssertUnwindSafe(|| listener(mutation)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_fidelity_listener_receives_payload_and_unsubscribe_fences_later_events() {
        let received = Arc::new(Mutex::new(Vec::<TranscriptMutationEnvelope>::new()));
        let sink = Arc::clone(&received);
        let subscription = subscribe_transcript_mutations(move |event| {
            sink.lock().unwrap().push(event.clone());
        });

        let mut first = TranscriptMutationEnvelope::new();
        first.insert("kind".into(), Value::String("entries-upserted".into()));
        first.insert("agentId".into(), Value::String("agent-a".into()));
        first.insert(
            "entries".into(),
            Value::Array(vec![serde_json::json!({"id":"m1","kind":"message"})]),
        );
        publish_transcript_mutation(&first);
        assert_eq!(received.lock().unwrap().as_slice(), &[first.clone()]);

        subscription.unsubscribe();
        publish_transcript_mutation(&first);
        assert_eq!(received.lock().unwrap().len(), 1);
    }

    #[test]
    fn typed_android_bus_remains_source_compatible() {
        let bus = TranscriptMutationBus::default();
        let receiver = bus.subscribe();
        let event = TranscriptMutation {
            agent_id: "agent-a".into(),
            sequence: 7,
            kind: "entries-upserted".into(),
        };
        bus.publish(event.clone());
        assert_eq!(receiver.recv().unwrap(), event);
    }
}
