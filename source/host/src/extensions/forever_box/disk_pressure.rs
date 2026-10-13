use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex, mpsc,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::durable_file_policy::SAND_DISK_PRESSURE_REMINDERS_FILE_NAME;

use super::disk_pressure_guard::{
    DiskPressureChangeListener, DiskPressureGuard, DiskPressureGuardOptions,
    DiskPressureLevel, DiskPressureLogger, DiskPressureReport, DiskPressureReporter,
    DiskPressureSampleListener, DiskVolumeRoot, read_disk_volume_snapshots,
};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SandDiskPressureLedgerError(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LedgerPending {
    agent_id: String,
    episode_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReminderLedger {
    version: u32,
    active_episode_id: Option<String>,
    handled_agent_ids: Vec<String>,
    pending: Vec<LedgerPending>,
}

#[derive(Debug, Clone)]
struct ReminderClaim {
    claim_id: String,
    episode_id: String,
}

#[derive(Debug, Default)]
struct ReminderState {
    active_episode_id: Option<String>,
    restored_episode_awaiting_pressure: bool,
    active_episode_persistence_pending: bool,
    handled_agent_ids: BTreeSet<String>,
    pending_episode_ids: BTreeMap<String, Vec<String>>,
    claims: BTreeMap<String, ReminderClaim>,
}

pub type DiskPressureLedgerErrorReporter =
    Arc<dyn Fn(&str) + Send + Sync + 'static>;
pub type EpisodeIdFactory = Arc<dyn Fn() -> String + Send + Sync + 'static>;

pub struct DiskPressureReminderEpisodes {
    file_path: Option<PathBuf>,
    create_episode_id: EpisodeIdFactory,
    on_ledger_error: Option<DiskPressureLedgerErrorReporter>,
    state: Mutex<ReminderState>,
}

impl DiskPressureReminderEpisodes {
    pub fn new(
        root_dir: Option<&Path>,
        create_episode_id: Option<EpisodeIdFactory>,
        on_ledger_error: Option<DiskPressureLedgerErrorReporter>,
    ) -> Self {
        let file_path = root_dir.map(|root| root.join(SAND_DISK_PRESSURE_REMINDERS_FILE_NAME));
        let mut state = ReminderState::default();

        if let Some(path) = file_path.as_ref() {
            match fs::read(path) {
                Ok(bytes) => match serde_json::from_slice::<ReminderLedger>(&bytes) {
                    Ok(ledger) if ledger.version == 1 => {
                        state.active_episode_id = ledger.active_episode_id;
                        state.restored_episode_awaiting_pressure =
                            state.active_episode_id.is_some();
                        if state.active_episode_id.is_some() {
                            state
                                .handled_agent_ids
                                .extend(ledger.handled_agent_ids);
                        }
                        for item in ledger.pending {
                            Self::enqueue_locked(
                                &mut state,
                                &item.agent_id,
                                &item.episode_id,
                            );
                        }
                    }
                    Ok(_) | Err(_) => {
                        if let Some(report) = &on_ledger_error {
                            report("invalid disk-pressure reminder ledger");
                        }
                    }
                },
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    if let Some(report) = &on_ledger_error {
                        report(&error.to_string());
                    }
                }
            }
        }

        Self {
            file_path,
            create_episode_id: create_episode_id
                .unwrap_or_else(|| Arc::new(|| Uuid::new_v4().to_string())),
            on_ledger_error,
            state: Mutex::new(state),
        }
    }

    fn enqueue_locked(
        state: &mut ReminderState,
        agent_id: &str,
        episode_id: &str,
    ) -> bool {
        let pending = state
            .pending_episode_ids
            .entry(agent_id.to_string())
            .or_default();
        if pending.iter().any(|value| value == episode_id) {
            return false;
        }
        pending.push(episode_id.to_string());
        true
    }

    fn report_ledger_error(&self, error: impl ToString) {
        if let Some(report) = &self.on_ledger_error {
            report(&error.to_string());
        }
    }

    fn persist_locked(&self, state: &ReminderState) -> bool {
        let Some(file_path) = self.file_path.as_ref() else {
            return true;
        };

        if state.active_episode_id.is_none() && state.pending_episode_ids.is_empty() {
            match fs::remove_file(file_path) {
                Ok(()) => return true,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return true,
                Err(error) => {
                    self.report_ledger_error(error);
                    return false;
                }
            }
        }

        let ledger = ReminderLedger {
            version: 1,
            active_episode_id: state.active_episode_id.clone(),
            handled_agent_ids: if state.active_episode_id.is_some() {
                state.handled_agent_ids.iter().cloned().collect()
            } else {
                Vec::new()
            },
            pending: state
                .pending_episode_ids
                .iter()
                .flat_map(|(agent_id, ids)| {
                    ids.iter().map(move |episode_id| LedgerPending {
                        agent_id: agent_id.clone(),
                        episode_id: episode_id.clone(),
                    })
                })
                .collect(),
        };
        let bytes = match serde_json::to_vec(&ledger) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.report_ledger_error(error);
                return false;
            }
        };
        let Some(parent) = file_path.parent() else {
            self.report_ledger_error("disk-pressure reminder ledger has no parent directory");
            return false;
        };
        if let Err(error) = fs::create_dir_all(parent) {
            self.report_ledger_error(error);
            return false;
        }

        let file_name = file_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("disk-pressure-reminders.json");
        let part = file_path.with_file_name(format!("{file_name}.part"));
        if let Err(error) = fs::write(&part, bytes) {
            self.report_ledger_error(error);
            return false;
        }
        #[cfg(windows)]
        if file_path.exists() {
            if let Err(error) = fs::remove_file(file_path) {
                let _ = fs::remove_file(&part);
                self.report_ledger_error(error);
                return false;
            }
        }
        if let Err(error) = fs::rename(&part, file_path) {
            let _ = fs::remove_file(&part);
            self.report_ledger_error(error);
            return false;
        }
        true
    }

    pub fn claim(&self, agent_id: &str, claim_id: &str) -> Option<String> {
        let mut state = self.state.lock().ok()?;
        if let Some(existing) = state.claims.get(agent_id) {
            return (existing.claim_id == claim_id)
                .then(|| existing.episode_id.clone());
        }
        if let Some(active_episode_id) = state.active_episode_id.clone() {
            if !state.handled_agent_ids.contains(agent_id)
                && Self::enqueue_locked(&mut state, agent_id, &active_episode_id)
            {
                self.persist_locked(&state);
            }
        }
        let episode_id = state
            .pending_episode_ids
            .get(agent_id)
            .and_then(|pending| pending.first())
            .cloned()?;
        state.claims.insert(
            agent_id.to_string(),
            ReminderClaim {
                claim_id: claim_id.to_string(),
                episode_id: episode_id.clone(),
            },
        );
        Some(episode_id)
    }

    pub fn release(&self, agent_id: &str, claim_id: &str) {
        if let Ok(mut state) = self.state.lock() {
            if state
                .claims
                .get(agent_id)
                .is_some_and(|claim| claim.claim_id == claim_id)
            {
                state.claims.remove(agent_id);
            }
        }
    }

    pub fn enroll<'a>(&self, agent_ids: impl IntoIterator<Item = &'a str>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let Some(active_episode_id) = state.active_episode_id.clone() else {
            return;
        };
        let mut changed = false;
        for agent_id in agent_ids {
            if !state.handled_agent_ids.contains(agent_id) {
                changed =
                    Self::enqueue_locked(&mut state, agent_id, &active_episode_id)
                        || changed;
            }
        }
        if changed {
            self.persist_locked(&state);
        }
    }

    pub fn commit(&self, agent_id: &str, claim_id: &str) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        let Some(claim) = state.claims.get(agent_id).cloned() else {
            return false;
        };
        let Some(pending) = state.pending_episode_ids.get(agent_id).cloned() else {
            return false;
        };
        if claim.claim_id != claim_id
            || pending.first().is_none_or(|episode| episode != &claim.episode_id)
        {
            return false;
        }

        let remaining = pending.iter().skip(1).cloned().collect::<Vec<_>>();
        if remaining.is_empty() {
            state.pending_episode_ids.remove(agent_id);
        } else {
            state
                .pending_episode_ids
                .insert(agent_id.to_string(), remaining);
        }
        let already_handled = state.handled_agent_ids.contains(agent_id);
        if state.active_episode_id.as_deref() == Some(claim.episode_id.as_str()) {
            state.handled_agent_ids.insert(agent_id.to_string());
        }

        if !self.persist_locked(&state) {
            state
                .pending_episode_ids
                .insert(agent_id.to_string(), pending);
            if !already_handled {
                state.handled_agent_ids.remove(agent_id);
            }
            return false;
        }

        state.claims.remove(agent_id);
        true
    }

    pub fn forget_agent(&self, agent_id: &str) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.claims.remove(agent_id);
        let changed = state.pending_episode_ids.remove(agent_id).is_some()
            || state.handled_agent_ids.remove(agent_id);
        if changed {
            self.persist_locked(&state);
        }
    }

    pub fn observe_pressure(&self, level: DiskPressureLevel, complete: bool) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        if level == DiskPressureLevel::Healthy {
            if state.active_episode_id.is_none()
                || (!complete && state.restored_episode_awaiting_pressure)
            {
                return;
            }
        } else if state.active_episode_id.is_some() {
            state.restored_episode_awaiting_pressure = false;
            if state.active_episode_persistence_pending && self.persist_locked(&state) {
                state.active_episode_persistence_pending = false;
            }
            return;
        }

        let old_active = state.active_episode_id.clone();
        let old_restored = state.restored_episode_awaiting_pressure;
        let old_handled = state.handled_agent_ids.clone();

        state.active_episode_id = if level == DiskPressureLevel::Healthy {
            None
        } else {
            Some((self.create_episode_id)())
        };
        state.restored_episode_awaiting_pressure = false;
        state.handled_agent_ids.clear();

        if self.persist_locked(&state) {
            state.active_episode_persistence_pending = false;
            return;
        }

        if level != DiskPressureLevel::Healthy {
            state.active_episode_persistence_pending = true;
            return;
        }

        state.active_episode_id = old_active;
        state.restored_episode_awaiting_pressure = old_restored;
        state.handled_agent_ids = old_handled;
    }
}

pub type DiskPressureWatchListener =
    Arc<dyn Fn(Option<DiskPressureLevel>) + Send + Sync + 'static>;

pub struct DiskPressureWatchDeps {
    pub is_in_box: bool,
    pub root_dir: PathBuf,
    pub polling_interval: Duration,
    pub report: DiskPressureReporter,
    pub log: DiskPressureLogger,
}

pub struct DiskPressureWatch {
    level: Arc<Mutex<Option<DiskPressureLevel>>>,
    reminder_episodes: Arc<DiskPressureReminderEpisodes>,
    listeners: Arc<Mutex<BTreeMap<u64, DiskPressureWatchListener>>>,
    next_listener_id: Mutex<u64>,
    guard: Option<Arc<DiskPressureGuard>>,
    stop_tx: Option<mpsc::Sender<()>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl DiskPressureWatch {
    pub fn level(&self) -> Option<DiskPressureLevel> {
        self.level.lock().ok().and_then(|level| *level)
    }

    pub fn reminder_episodes(&self) -> Arc<DiskPressureReminderEpisodes> {
        Arc::clone(&self.reminder_episodes)
    }

    pub fn subscribe(&self, listener: DiskPressureWatchListener) -> u64 {
        let mut next = self.next_listener_id.lock().expect("disk-pressure listener id");
        *next = next.saturating_add(1);
        let id = *next;
        self.listeners
            .lock()
            .expect("disk-pressure listeners")
            .insert(id, listener);
        id
    }

    pub fn unsubscribe(&self, subscription: u64) {
        if let Ok(mut listeners) = self.listeners.lock() {
            listeners.remove(&subscription);
        }
    }

    pub fn dispose(&self) {
        if let Some(guard) = &self.guard {
            guard.dispose();
        }
        if let Some(stop) = &self.stop_tx {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.lock().ok().and_then(|mut worker| worker.take()) {
            let _ = worker.join();
        }
        if let Ok(mut listeners) = self.listeners.lock() {
            listeners.clear();
        }
    }
}

pub fn start_disk_pressure_watch(deps: DiskPressureWatchDeps) -> DiskPressureWatch {
    let reminder_episodes = Arc::new(DiskPressureReminderEpisodes::new(
        deps.is_in_box.then_some(deps.root_dir.as_path()),
        None,
        Some({
            let log = Arc::clone(&deps.log);
            Arc::new(move |error| log(&format!(
                "disk-pressure reminder ledger failed: {error}"
            )))
        }),
    ));
    let level = Arc::new(Mutex::new(None));
    let listeners = Arc::new(Mutex::new(BTreeMap::<u64, DiskPressureWatchListener>::new()));

    if !deps.is_in_box {
        return DiskPressureWatch {
            level,
            reminder_episodes,
            listeners,
            next_listener_id: Mutex::new(0),
            guard: None,
            stop_tx: None,
            worker: Mutex::new(None),
        };
    }

    let roots = vec![
        DiskVolumeRoot::new("workspace", PathBuf::from("/workspace")),
        DiskVolumeRoot::new("sand_data", deps.root_dir.clone()),
        DiskVolumeRoot::new("temp", std::env::temp_dir()),
    ];
    let level_for_change = Arc::clone(&level);
    let listeners_for_change = Arc::clone(&listeners);
    let on_pressure_change: DiskPressureChangeListener = Arc::new(move |next| {
        if let Ok(mut slot) = level_for_change.lock() {
            *slot = next;
        }
        let listeners = listeners_for_change
            .lock()
            .map(|listeners| listeners.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for listener in listeners {
            listener(next);
        }
    });
    let reminders_for_sample = Arc::clone(&reminder_episodes);
    let on_successful_sample: DiskPressureSampleListener = Arc::new(move |next, complete| {
        reminders_for_sample.observe_pressure(next, complete);
    });
    let guard = Arc::new(DiskPressureGuard::new(DiskPressureGuardOptions {
        read_volumes: Arc::new(move || Ok(read_disk_volume_snapshots(&roots))),
        report: deps.report,
        on_pressure_change,
        on_successful_sample: Some(on_successful_sample),
        log: deps.log,
        clock: None,
    }));
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let worker_guard = Arc::clone(&guard);
    let interval = deps.polling_interval;
    let worker = thread::Builder::new()
        .name("mahayana-forever-box-disk-pressure".into())
        .spawn(move || {
            worker_guard.on_tick();
            loop {
                match stop_rx.recv_timeout(interval) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => worker_guard.on_tick(),
                }
            }
        })
        .ok();

    DiskPressureWatch {
        level,
        reminder_episodes,
        listeners,
        next_listener_id: Mutex::new(0),
        guard: Some(guard),
        stop_tx: Some(stop_tx),
        worker: Mutex::new(worker),
    }
}
