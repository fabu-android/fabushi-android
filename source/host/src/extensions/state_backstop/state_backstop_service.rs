use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Condvar, Mutex, Weak,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

pub const SAND_STATE_BACKSTOP_REL_PATH: &str = "state/store.db";
pub const DEFAULT_MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;
pub const STATE_BACKSTOP_DEBOUNCE_MS: u64 = 5_000;
const MAX_AGENT_ID_BYTES: usize = 512;

pub trait StateBackstopObjectStore: Send + Sync {
    fn put(&self, path: &str, bytes: &[u8]) -> Result<(), String>;
    fn get(&self, path: &str) -> Result<Option<Vec<u8>>, String>;
}

pub type StateBackstopStoreProvider =
    Arc<dyn Fn(&str) -> Arc<dyn StateBackstopObjectStore> + Send + Sync>;
pub type StateBackstopSourceResolver =
    Arc<dyn Fn(&str) -> Result<String, String> + Send + Sync>;
pub type StateBackstopReadDb =
    Arc<dyn Fn(&Path) -> Result<Option<Vec<u8>>, String> + Send + Sync>;
pub type StateBackstopLog = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateBackstopSnapshotResult {
    Uploaded { bytes: usize },
    Skipped { reason: String },
    Error { error: String },
}

pub struct StateBackstopOptions {
    pub object_store_provider: StateBackstopStoreProvider,
    pub source_id_for_agent: StateBackstopSourceResolver,
    pub agents_root_dir: PathBuf,
    pub read_db_bytes: StateBackstopReadDb,
    pub max_snapshot_bytes: usize,
    pub debounce: Duration,
    pub log: StateBackstopLog,
}

impl StateBackstopOptions {
    pub fn new(
        object_store_provider: StateBackstopStoreProvider,
        source_id_for_agent: StateBackstopSourceResolver,
        agents_root_dir: PathBuf,
        read_db_bytes: StateBackstopReadDb,
    ) -> Self {
        Self {
            object_store_provider,
            source_id_for_agent,
            agents_root_dir,
            read_db_bytes,
            max_snapshot_bytes: DEFAULT_MAX_SNAPSHOT_BYTES,
            debounce: Duration::from_millis(STATE_BACKSTOP_DEBOUNCE_MS),
            log: Arc::new(|message| eprintln!("[sand-state-backstop] {message}")),
        }
    }
}

pub fn read_store_db_bytes(
    db_path: &Path,
    checkpoint: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<Option<Vec<u8>>, String> {
    if !db_path.exists() {
        return Ok(None);
    }
    checkpoint(db_path)?;
    fs::read(db_path)
        .map(Some)
        .map_err(|error| error.to_string())
}

#[derive(Debug, Default)]
struct DebounceState {
    revision: u64,
    stopped: bool,
}

#[derive(Debug, Default)]
struct DebounceSlot {
    state: Mutex<DebounceState>,
    changed: Condvar,
}

impl DebounceSlot {
    fn trigger(&self) {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.stopped {
            return;
        }
        state.revision = state.revision.wrapping_add(1);
        self.changed.notify_all();
    }

    fn stop(&self) {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.stopped = true;
        self.changed.notify_all();
    }
}

pub struct SandStateBackstop {
    object_store_provider: StateBackstopStoreProvider,
    source_id_for_agent: StateBackstopSourceResolver,
    agents_root_dir: PathBuf,
    read_db_bytes: StateBackstopReadDb,
    max_snapshot_bytes: usize,
    debounce: Duration,
    log: StateBackstopLog,
    disposed: Arc<AtomicBool>,
    pending: Arc<Mutex<HashMap<String, Arc<DebounceSlot>>>>,
}

impl SandStateBackstop {
    pub fn new(options: StateBackstopOptions) -> Arc<Self> {
        Arc::new(Self {
            object_store_provider: options.object_store_provider,
            source_id_for_agent: options.source_id_for_agent,
            agents_root_dir: options.agents_root_dir,
            read_db_bytes: options.read_db_bytes,
            max_snapshot_bytes: options.max_snapshot_bytes,
            debounce: options.debounce,
            log: options.log,
            disposed: Arc::new(AtomicBool::new(false)),
            pending: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn db_path_for(&self, agent_id: &str) -> PathBuf {
        self.agents_root_dir.join(agent_id).join("store.db")
    }

    pub fn schedule_snapshot(self: &Arc<Self>, agent_id: impl Into<String>) {
        if self.disposed.load(Ordering::Acquire) {
            return;
        }

        let agent_id = agent_id.into();
        if let Err(error) = validate_agent_id(&agent_id) {
            (self.log)(&format!("snapshot rejected for invalid agent id: {error}"));
            return;
        }
        let (slot, created) = {
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match pending.get(&agent_id) {
                Some(slot) => (Arc::clone(slot), false),
                None => {
                    let slot = Arc::new(DebounceSlot::default());
                    pending.insert(agent_id.clone(), Arc::clone(&slot));
                    (slot, true)
                }
            }
        };

        if created {
            let service = Arc::downgrade(self);
            let worker_slot = Arc::clone(&slot);
            let worker_agent_id = agent_id.clone();
            let debounce = self.debounce;
            if let Err(error) = thread::Builder::new()
                .name("sand-state-backstop-snapshot".into())
                .spawn(move || {
                    run_debounce_worker(service, worker_slot, worker_agent_id, debounce);
                })
            {
                self.pending
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .remove(&agent_id);
                (self.log)(&format!(
                    "could not start snapshot debounce worker for {agent_id}: {error}"
                ));
                return;
            }
        }

        slot.trigger();
    }

    pub fn snapshot_now(&self, agent_id: &str) -> StateBackstopSnapshotResult {
        let result = (|| -> Result<StateBackstopSnapshotResult, String> {
            validate_agent_id(agent_id)?;
            let path = self.db_path_for(agent_id);
            let Some(bytes) = (self.read_db_bytes)(&path)? else {
                return Ok(StateBackstopSnapshotResult::Skipped {
                    reason: "no store.db".into(),
                });
            };
            if bytes.len() > self.max_snapshot_bytes {
                let reason = format!(
                    "store.db {}B over {}B cap",
                    bytes.len(),
                    self.max_snapshot_bytes
                );
                (self.log)(&format!("skip {agent_id}: {reason}"));
                return Ok(StateBackstopSnapshotResult::Skipped { reason });
            }
            let source_id = (self.source_id_for_agent)(agent_id)?;
            let store = (self.object_store_provider)(&source_id);
            store.put(SAND_STATE_BACKSTOP_REL_PATH, &bytes)?;
            (self.log)(&format!("uploaded {agent_id} ({}B)", bytes.len()));
            Ok(StateBackstopSnapshotResult::Uploaded { bytes: bytes.len() })
        })();

        match result {
            Ok(result) => result,
            Err(error) => {
                (self.log)(&format!("snapshot {agent_id} failed: {error}"));
                StateBackstopSnapshotResult::Error { error }
            }
        }
    }

    pub fn read_snapshot(&self, agent_id: &str) -> Result<Option<Vec<u8>>, String> {
        validate_agent_id(agent_id)?;
        let source_id = (self.source_id_for_agent)(agent_id)?;
        (self.object_store_provider)(&source_id).get(SAND_STATE_BACKSTOP_REL_PATH)
    }

    pub fn dispose(&self) {
        self.disposed.store(true, Ordering::Release);
        let slots = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .drain()
            .map(|(_, slot)| slot)
            .collect::<Vec<_>>();
        for slot in slots {
            slot.stop();
        }
    }
}

fn run_debounce_worker(
    service: Weak<SandStateBackstop>,
    slot: Arc<DebounceSlot>,
    agent_id: String,
    debounce: Duration,
) {
    let mut seen_revision = 0u64;

    loop {
        let mut state = slot
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        while !state.stopped && state.revision == seen_revision {
            state = slot
                .changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        if state.stopped {
            return;
        }

        seen_revision = state.revision;
        let mut deadline = Instant::now() + debounce;
        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let wait_for = deadline.saturating_duration_since(now);
            let (next_state, timeout) = slot
                .changed
                .wait_timeout(state, wait_for)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = next_state;

            if state.stopped {
                return;
            }
            if state.revision != seen_revision {
                seen_revision = state.revision;
                deadline = Instant::now() + debounce;
                continue;
            }
            if timeout.timed_out() {
                break;
            }
        }
        drop(state);

        let Some(service) = service.upgrade() else {
            return;
        };
        if service.disposed.load(Ordering::Acquire) {
            return;
        }
        if let StateBackstopSnapshotResult::Error { error } = service.snapshot_now(&agent_id) {
            (service.log)(&format!("snapshot rejected for {agent_id}: {error}"));
        }
    }
}

fn validate_agent_id(agent_id: &str) -> Result<(), String> {
    let agent_id = agent_id.trim();
    if agent_id.is_empty()
        || agent_id.len() > MAX_AGENT_ID_BYTES
        || matches!(agent_id, "." | "..")
        || agent_id
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\'))
    {
        return Err("state backstop agent id must be one bounded path component".into());
    }
    Ok(())
}

pub fn is_state_backstop_enabled_value(raw: Option<&str>) -> bool {
    raw.map(str::trim)
        .map(str::to_ascii_lowercase)
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"))
}

/// Android never derives destructive/durable backstop policy from process environment.
pub fn is_state_backstop_enabled() -> bool {
    false
}

#[cfg(test)]
mod android_tests {
    use super::*;

    #[test]
    fn android_environment_gate_is_never_a_second_authority() {
        assert!(!is_state_backstop_enabled());
        assert!(is_state_backstop_enabled_value(Some("true")));
        assert!(!is_state_backstop_enabled_value(Some("false")));
    }

    #[test]
    fn agent_identity_cannot_escape_the_canonical_agents_root() {
        for invalid in ["", "   ", ".", "..", "../outside", "nested/agent", "nested\\agent"] {
            assert!(validate_agent_id(invalid).is_err(), "{invalid:?} must be rejected");
        }
        assert!(validate_agent_id(&"a".repeat(MAX_AGENT_ID_BYTES + 1)).is_err());
        assert!(validate_agent_id("agent:account-a-123").is_ok());
    }
}
