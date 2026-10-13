use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::extensions::experiments::{HostExperimentsExtension, StopGateSubscription};

pub const SAND_PRODUCT_ANALYTICS_GATE: &str = "sand_product_analytics";
pub const MAX_DEFERRED_ANALYTICS_EVENTS: usize = 256;
pub const ANALYTICS_BUFFER_LIMIT: usize = 200;
pub const ANALYTICS_FLUSH_INTERVAL_MS: u64 = 3_000;

pub type ProductAnalyticsSender =
    Arc<dyn Fn(&Value) -> Result<(), String> + Send + Sync + 'static>;

#[derive(Debug, Clone, PartialEq)]
pub struct ProductAnalyticsEvent {
    pub name: String,
    pub properties: BTreeMap<String, Value>,
}

pub fn product_analytics_event(name: &str, properties: &BTreeMap<String, String>) -> ProductAnalyticsEvent {
    ProductAnalyticsEvent {
        name: name.to_string(),
        properties: properties
            .iter()
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect(),
    }
}

#[derive(Debug, Clone)]
struct ScopedQueuedEvent {
    account_scope: String,
    event: ProductAnalyticsEvent,
    timestamp_ms: u64,
}

enum WorkerCommand {
    Event(ScopedQueuedEvent),
    Gate(bool),
    Stop,
}

pub struct AndroidProductAnalyticsRuntime {
    tx: mpsc::Sender<WorkerCommand>,
    gate_stop: Mutex<Option<StopGateSubscription>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    active_day_keys: Mutex<BTreeMap<String, u64>>,
}

impl AndroidProductAnalyticsRuntime {
    pub fn start(
        experiments: Arc<HostExperimentsExtension>,
        sender: ProductAnalyticsSender,
    ) -> Arc<Self> {
        let enabled = experiments.check_feature_gate(SAND_PRODUCT_ANALYTICS_GATE);
        let (tx, rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("fabushi-product-analytics".into())
            .spawn(move || run_worker(rx, sender, enabled))
            .expect("failed to start Product Analytics worker");
        let runtime = Arc::new(Self {
            tx,
            gate_stop: Mutex::new(None),
            worker: Mutex::new(Some(worker)),
            active_day_keys: Mutex::new(BTreeMap::new()),
        });
        let weak = Arc::downgrade(&runtime);
        let stop = experiments.subscribe_gate(
            SAND_PRODUCT_ANALYTICS_GATE,
            Arc::new(move |enabled| {
                if let Some(runtime) = weak.upgrade() {
                    let _ = runtime.tx.send(WorkerCommand::Gate(enabled));
                }
            }),
        );
        *runtime.gate_stop.lock().unwrap_or_else(|p| p.into_inner()) = Some(stop);
        runtime
    }

    pub fn track(
        &self,
        account_scope: &str,
        event: ProductAnalyticsEvent,
    ) -> Result<(), String> {
        let account_scope = account_scope.trim();
        if account_scope.is_empty() {
            return Err("Product Analytics account scope is required".into());
        }
        if event.name.trim().is_empty() {
            return Err("Product Analytics event name is required".into());
        }
        self.tx
            .send(WorkerCommand::Event(ScopedQueuedEvent {
                account_scope: account_scope.to_string(),
                event,
                timestamp_ms: wall_clock_now_ms(),
            }))
            .map_err(|_| "Product Analytics worker is unavailable".to_string())
    }

    pub fn mark_active(&self, account_scope: &str, reason: &str) -> Result<(), String> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err("Product Analytics active reason is required".into());
        }
        let day = wall_clock_now_ms() / 86_400_000;
        let key = format!("{account_scope}\n{reason}");
        {
            let mut days = self.active_day_keys.lock().map_err(|_| {
                "Product Analytics active-day state lock poisoned".to_string()
            })?;
            if days.get(&key).copied() == Some(day) {
                return Ok(());
            }
            days.insert(key, day);
        }
        self.track(
            account_scope,
            ProductAnalyticsEvent {
                name: "sand.app.active".into(),
                properties: BTreeMap::from([(
                    "reason".into(),
                    Value::String(reason.to_string()),
                )]),
            },
        )
    }
}

impl Drop for AndroidProductAnalyticsRuntime {
    fn drop(&mut self) {
        if let Ok(stop) = self.gate_stop.get_mut() {
            if let Some(stop) = stop.take() {
                stop();
            }
        }
        let _ = self.tx.send(WorkerCommand::Stop);
        if let Ok(worker) = self.worker.get_mut() {
            if let Some(worker) = worker.take() {
                let _ = worker.join();
            }
        }
    }
}

fn run_worker(
    rx: mpsc::Receiver<WorkerCommand>,
    sender: ProductAnalyticsSender,
    mut enabled: bool,
) {
    let mut buffer = VecDeque::<ScopedQueuedEvent>::new();
    let mut current_scope: Option<String> = None;
    let mut flush_deadline: Option<Instant> = None;

    loop {
        let command = if enabled && !buffer.is_empty() {
            let deadline = flush_deadline
                .get_or_insert_with(|| Instant::now() + Duration::from_millis(ANALYTICS_FLUSH_INTERVAL_MS));
            match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(command) => Some(command),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    flush_batch(&mut buffer, &sender);
                    flush_deadline = None;
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => None,
            }
        } else {
            rx.recv().ok()
        };

        match command {
            Some(WorkerCommand::Event(queued)) => {
                if current_scope.as_deref() != Some(queued.account_scope.as_str()) {
                    buffer.clear();
                    current_scope = Some(queued.account_scope.clone());
                    flush_deadline = None;
                }
                buffer.push_back(queued);
                if enabled {
                    if buffer.len() >= ANALYTICS_BUFFER_LIMIT {
                        flush_batch(&mut buffer, &sender);
                        flush_deadline = None;
                    }
                } else {
                    while buffer.len() > MAX_DEFERRED_ANALYTICS_EVENTS {
                        buffer.pop_front();
                    }
                }
            }
            Some(WorkerCommand::Gate(next)) => {
                enabled = next;
                if enabled {
                    flush_batch(&mut buffer, &sender);
                    flush_deadline = None;
                }
            }
            Some(WorkerCommand::Stop) | None => {
                if enabled {
                    flush_batch(&mut buffer, &sender);
                }
                break;
            }
        }
    }
}

fn flush_batch(buffer: &mut VecDeque<ScopedQueuedEvent>, sender: &ProductAnalyticsSender) {
    if buffer.is_empty() {
        return;
    }
    let batch = buffer
        .iter()
        .take(ANALYTICS_BUFFER_LIMIT)
        .cloned()
        .collect::<Vec<_>>();
    let payload = json!({
        "events": batch.iter().map(|queued| {
            let event_data = queued.event.properties.iter().map(|(key, value)| {
                let encoded = match value {
                    Value::String(value) => json!({"stringValue":value}),
                    Value::Bool(value) => json!({"boolValue":value}),
                    Value::Number(value) => json!({"doubleValue":value.as_f64().unwrap_or(0.0)}),
                    _ => Value::Null,
                };
                (key.clone(), encoded)
            }).collect::<serde_json::Map<_, _>>();
            json!({
                "eventName": queued.event.name,
                "eventData": event_data,
                "timestamp": queued.timestamp_ms.to_string(),
            })
        }).collect::<Vec<_>>()
    });
    match sender(&payload) {
        Ok(()) => {
            for _ in 0..batch.len() {
                buffer.pop_front();
            }
        }
        Err(_) => {
            while buffer.len() > ANALYTICS_BUFFER_LIMIT {
                buffer.pop_front();
            }
        }
    }
}

fn wall_clock_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::experiments::HostExperimentsOptions;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn test_experiments(enabled: bool) -> Arc<HostExperimentsExtension> {
        let experiments = Arc::new(HostExperimentsExtension::with_options(
            HostExperimentsOptions {
                is_dev_build: true,
                env_gate_overrides: None,
            },
        ));
        experiments.set_settings_override(SAND_PRODUCT_ANALYTICS_GATE, Some(enabled));
        experiments
    }

    fn wait_until(predicate: impl Fn() -> bool) {
        for _ in 0..100 {
            if predicate() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("Product Analytics test condition timed out");
    }

    #[test]
    fn disabled_gate_defers_and_enable_flushes() {
        let experiments = test_experiments(false);
        let batches = Arc::new(Mutex::new(Vec::<Value>::new()));
        let sink = Arc::clone(&batches);
        let runtime = AndroidProductAnalyticsRuntime::start(
            Arc::clone(&experiments),
            Arc::new(move |payload| {
                sink.lock().unwrap().push(payload.clone());
                Ok(())
            }),
        );
        runtime
            .track(
                "account-a",
                ProductAnalyticsEvent {
                    name: "sand.test".into(),
                    properties: BTreeMap::from([("ok".into(), Value::Bool(true))]),
                },
            )
            .unwrap();
        std::thread::sleep(Duration::from_millis(30));
        assert!(batches.lock().unwrap().is_empty());
        experiments.set_settings_override(SAND_PRODUCT_ANALYTICS_GATE, Some(true));
        wait_until(|| !batches.lock().unwrap().is_empty());
        let first = batches.lock().unwrap()[0].clone();
        assert_eq!(first["events"][0]["eventName"], "sand.test");
        assert_eq!(first["events"][0]["eventData"]["ok"]["boolValue"], true);
    }

    #[test]
    fn account_switch_drops_old_deferred_events_before_new_scope_delivery() {
        let experiments = test_experiments(false);
        let batches = Arc::new(Mutex::new(Vec::<Value>::new()));
        let sink = Arc::clone(&batches);
        let runtime = AndroidProductAnalyticsRuntime::start(
            Arc::clone(&experiments),
            Arc::new(move |payload| {
                sink.lock().unwrap().push(payload.clone());
                Ok(())
            }),
        );
        for (scope, name) in [("account-a", "old"), ("account-b", "new")] {
            runtime
                .track(
                    scope,
                    ProductAnalyticsEvent {
                        name: name.into(),
                        properties: BTreeMap::new(),
                    },
                )
                .unwrap();
        }
        experiments.set_settings_override(SAND_PRODUCT_ANALYTICS_GATE, Some(true));
        wait_until(|| !batches.lock().unwrap().is_empty());
        let batch = batches.lock().unwrap()[0].clone();
        assert_eq!(batch["events"].as_array().unwrap().len(), 1);
        assert_eq!(batch["events"][0]["eventName"], "new");
    }

    #[test]
    fn failed_delivery_is_bounded_and_never_panics_business_caller() {
        let experiments = test_experiments(true);
        let attempts = Arc::new(AtomicUsize::new(0));
        let sink = Arc::clone(&attempts);
        let runtime = AndroidProductAnalyticsRuntime::start(
            experiments,
            Arc::new(move |_| {
                sink.fetch_add(1, Ordering::SeqCst);
                Err("offline".into())
            }),
        );
        for index in 0..ANALYTICS_BUFFER_LIMIT {
            runtime
                .track(
                    "account-a",
                    ProductAnalyticsEvent {
                        name: format!("event-{index}"),
                        properties: BTreeMap::new(),
                    },
                )
                .unwrap();
        }
        wait_until(|| attempts.load(Ordering::SeqCst) > 0);
        assert!(runtime.mark_active("account-a", "user_action").is_ok());
    }
}
