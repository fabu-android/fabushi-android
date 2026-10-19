use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};

use serde_json::{Map, Value};

use crate::host_diagnostics::{HostDiagnostic, report_host_diagnostic};
use crate::transcript_mirror::legacy_transcript_mirror::{
    LegacyFileTranscriptMirror, LegacyTranscriptBlobStore, LegacyTranscriptState,
};

use super::agent_worker_pool::AgentBlobWorkerBackend;
use super::worker_blob_store::WorkerBlobStore;
use super::transcript_mirror_worker::{
    TranscriptMirrorWorkerJob, run_transcript_mirror_worker_job,
};

pub const DEFAULT_MIRROR_WORKERS: usize = 2;

pub type TranscriptMirrorWorkerFn =
    Arc<dyn Fn(&TranscriptMirrorWorkerJob) -> Result<bool, String> + Send + Sync + 'static>;

enum WorkerCommand {
    Write {
        job: TranscriptMirrorWorkerJob,
        reply: mpsc::SyncSender<Result<bool, String>>,
    },
    Close {
        reply: mpsc::SyncSender<()>,
    },
}

struct MirrorWorkerConnection {
    sender: mpsc::Sender<WorkerCommand>,
    alive: Arc<AtomicBool>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl MirrorWorkerConnection {
    fn new(
        shard_index: usize,
        worker: TranscriptMirrorWorkerFn,
    ) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel::<WorkerCommand>();
        let alive = Arc::new(AtomicBool::new(true));
        let thread_alive = Arc::clone(&alive);
        let handle = thread::Builder::new()
            .name(format!("transcript-mirror-{shard_index}"))
            .spawn(move || {
                while let Ok(command) = receiver.recv() {
                    match command {
                        WorkerCommand::Write { job, reply } => {
                            let result = catch_unwind(AssertUnwindSafe(|| worker(&job)))
                                .map_err(|_| "transcript mirror worker panicked".to_string())
                                .and_then(|result| result);
                            let _ = reply.send(result);
                        }
                        WorkerCommand::Close { reply } => {
                            let _ = reply.send(());
                            break;
                        }
                    }
                }
                thread_alive.store(false, Ordering::Release);
            })
            .map_err(|error| format!("could not start transcript mirror worker: {error}"))?;
        Ok(Self {
            sender,
            alive,
            handle: Mutex::new(Some(handle)),
        })
    }

    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    fn write(&self, job: TranscriptMirrorWorkerJob) -> Result<bool, String> {
        if !self.is_alive() {
            return Err("mirror worker is no longer running".into());
        }
        let (reply, result) = mpsc::sync_channel(1);
        self.sender
            .send(WorkerCommand::Write { job, reply })
            .map_err(|_| "mirror worker is no longer running".to_string())?;
        result
            .recv()
            .map_err(|_| "mirror worker terminated before replying".to_string())?
    }

    fn close(&self) {
        if self.is_alive() {
            let (reply, result) = mpsc::sync_channel(1);
            let _ = self.sender.send(WorkerCommand::Close { reply });
            let _ = result.recv();
        }
        if let Ok(mut handle) = self.handle.lock() {
            if let Some(handle) = handle.take() {
                let _ = handle.join();
            }
        }
        self.alive.store(false, Ordering::Release);
    }
}

struct QueuedMirrorJob {
    job: TranscriptMirrorWorkerJob,
    resolvers: Vec<mpsc::SyncSender<Result<bool, String>>>,
}

#[derive(Default)]
struct ConversationLane {
    is_running: bool,
    queued: Option<QueuedMirrorJob>,
}

struct PoolState {
    is_closed: bool,
    workers: Vec<Option<Arc<MirrorWorkerConnection>>>,
    lanes: HashMap<String, ConversationLane>,
}

struct PoolInner {
    max_workers: usize,
    worker: TranscriptMirrorWorkerFn,
    state: Mutex<PoolState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranscriptMirrorOffloadPoolOptions {
    pub max_workers: usize,
}

impl Default for TranscriptMirrorOffloadPoolOptions {
    fn default() -> Self {
        Self {
            max_workers: DEFAULT_MIRROR_WORKERS,
        }
    }
}

#[derive(Clone)]
pub struct TranscriptMirrorOffloadPool {
    inner: Arc<PoolInner>,
}

impl TranscriptMirrorOffloadPool {
    pub fn production() -> Self {
        Self::with_worker(
            TranscriptMirrorOffloadPoolOptions::default(),
            Arc::new(|job| {
                run_transcript_mirror_worker_job(job).map_err(|error| error.to_string())
            }),
        )
    }

    pub fn with_worker(
        options: TranscriptMirrorOffloadPoolOptions,
        worker: TranscriptMirrorWorkerFn,
    ) -> Self {
        assert!(
            options.max_workers > 0,
            "transcript mirror offload pool requires at least one worker"
        );
        Self {
            inner: Arc::new(PoolInner {
                max_workers: options.max_workers,
                worker,
                state: Mutex::new(PoolState {
                    is_closed: false,
                    workers: vec![None; options.max_workers],
                    lanes: HashMap::new(),
                }),
            }),
        }
    }

    pub fn worker_index_for(&self, conversation_id: &str) -> usize {
        worker_index_for(conversation_id, self.inner.max_workers)
    }

    pub fn write(&self, job: TranscriptMirrorWorkerJob) -> Result<bool, String> {
        let conversation_id = job.conversation_id.clone();
        let (resolver, result) = mpsc::sync_channel(1);
        let mut launch = None;

        {
            let mut state = self
                .inner
                .state
                .lock()
                .map_err(|_| "transcript mirror offload pool is poisoned".to_string())?;
            if state.is_closed {
                return Err("mirror offload pool is closed".into());
            }
            let lane = state
                .lanes
                .entry(conversation_id.clone())
                .or_default();
            if lane.is_running {
                match lane.queued.as_mut() {
                    Some(queued) => {
                        queued.job = job;
                        queued.resolvers.push(resolver);
                    }
                    None => {
                        lane.queued = Some(QueuedMirrorJob {
                            job,
                            resolvers: vec![resolver],
                        });
                    }
                }
            } else {
                lane.is_running = true;
                launch = Some(QueuedMirrorJob {
                    job,
                    resolvers: vec![resolver],
                });
            }
        }

        if let Some(entry) = launch {
            let inner = Arc::clone(&self.inner);
            thread::spawn(move || run_lane(inner, conversation_id, entry));
        }

        result
            .recv()
            .map_err(|_| "mirror offload lane terminated before replying".to_string())?
    }

    pub fn close_all(&self) {
        let (workers, queued_resolvers) = {
            let Ok(mut state) = self.inner.state.lock() else {
                return;
            };
            if state.is_closed {
                return;
            }
            state.is_closed = true;

            let mut queued_resolvers = Vec::new();
            for lane in state.lanes.values_mut() {
                if let Some(queued) = lane.queued.take() {
                    queued_resolvers.extend(queued.resolvers);
                }
            }
            let workers = state
                .workers
                .iter_mut()
                .filter_map(Option::take)
                .collect::<Vec<_>>();
            (workers, queued_resolvers)
        };

        for resolver in queued_resolvers {
            let _ = resolver.send(Err("mirror offload pool is closed".into()));
        }
        for worker in workers {
            worker.close();
        }
    }

    pub fn active_worker_count(&self) -> usize {
        self.inner
            .state
            .lock()
            .map(|state| {
                state
                    .workers
                    .iter()
                    .filter(|worker| worker.as_ref().is_some_and(|worker| worker.is_alive()))
                    .count()
            })
            .unwrap_or_default()
    }

    pub fn queued_waiter_count(&self, conversation_id: &str) -> usize {
        self.inner
            .state
            .lock()
            .ok()
            .and_then(|state| {
                state
                    .lanes
                    .get(conversation_id)
                    .and_then(|lane| lane.queued.as_ref())
                    .map(|queued| queued.resolvers.len())
            })
            .unwrap_or_default()
    }

    pub fn active_lane_count(&self) -> usize {
        self.inner
            .state
            .lock()
            .map(|state| state.lanes.len())
            .unwrap_or_default()
    }
}

impl Drop for TranscriptMirrorOffloadPool {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) == 1 {
            self.close_all();
        }
    }
}

fn run_lane(
    inner: Arc<PoolInner>,
    conversation_id: String,
    mut entry: QueuedMirrorJob,
) {
    loop {
        let result = connection_for(&inner, &entry.job.conversation_id)
            .and_then(|connection| connection.write(entry.job.clone()));
        for resolver in entry.resolvers.drain(..) {
            let _ = resolver.send(result.clone());
        }

        let next = {
            let Ok(mut state) = inner.state.lock() else {
                return;
            };
            let is_closed = state.is_closed;
            let next = state
                .lanes
                .get_mut(&conversation_id)
                .and_then(|lane| lane.queued.take());

            if is_closed {
                if let Some(queued) = next {
                    for resolver in queued.resolvers {
                        let _ = resolver.send(Err("mirror offload pool is closed".into()));
                    }
                }
                state.lanes.remove(&conversation_id);
                return;
            }

            if next.is_none() {
                state.lanes.remove(&conversation_id);
            }
            next
        };

        let Some(next) = next else {
            return;
        };
        entry = next;
    }
}

fn connection_for(
    inner: &Arc<PoolInner>,
    conversation_id: &str,
) -> Result<Arc<MirrorWorkerConnection>, String> {
    let index = worker_index_for(conversation_id, inner.max_workers);
    let mut state = inner
        .state
        .lock()
        .map_err(|_| "transcript mirror offload pool is poisoned".to_string())?;
    if state.is_closed {
        return Err("mirror offload pool is closed".into());
    }
    if let Some(existing) = state.workers[index].as_ref() {
        if existing.is_alive() {
            return Ok(Arc::clone(existing));
        }
    }
    let connection = Arc::new(MirrorWorkerConnection::new(
        index,
        Arc::clone(&inner.worker),
    )?);
    state.workers[index] = Some(Arc::clone(&connection));
    Ok(connection)
}

fn worker_index_for(conversation_id: &str, max_workers: usize) -> usize {
    let mut hash = 0i32;
    for code_unit in conversation_id.encode_utf16() {
        hash = hash
            .wrapping_mul(31)
            .wrapping_add(i32::from(code_unit));
    }
    (i64::from(hash).abs() as usize) % max_workers
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffloadingTranscriptMirrorOptions {
    pub blob_db_paths: Vec<PathBuf>,
    pub transcripts_dir: PathBuf,
}

pub struct OffloadingTranscriptMirror {
    inline: LegacyFileTranscriptMirror,
    pool: Arc<TranscriptMirrorOffloadPool>,
    options: OffloadingTranscriptMirrorOptions,
    previous_root_prompt_count: Mutex<usize>,
    write_lane: Mutex<()>,
}

impl OffloadingTranscriptMirror {
    pub fn for_transcripts_dir(
        pool: Arc<TranscriptMirrorOffloadPool>,
        options: OffloadingTranscriptMirrorOptions,
        previous_root_prompt_count: usize,
    ) -> Self {
        Self {
            inline: LegacyFileTranscriptMirror::new(&options.transcripts_dir),
            pool,
            options,
            previous_root_prompt_count: Mutex::new(previous_root_prompt_count),
            write_lane: Mutex::new(()),
        }
    }

    pub fn previous_root_prompt_count(&self) -> usize {
        self.previous_root_prompt_count
            .lock()
            .map(|value| *value)
            .unwrap_or_default()
    }

    pub fn write_inline<Store: LegacyTranscriptBlobStore>(
        &self,
        conversation_id: &str,
        state: &LegacyTranscriptState,
        blob_store: &Store,
    ) -> Result<(), String> {
        let _lane = self
            .write_lane
            .lock()
            .map_err(|_| "transcript mirror write lane is poisoned".to_string())?;
        let previous = self.previous_root_prompt_count();
        match self
            .inline
            .write_incremental(conversation_id, state, blob_store, previous)
            .map_err(|error| error.to_string())?
        {
            Some(count) => {
                self.set_previous_root_prompt_count(count);
                return Ok(());
            }
            None => {}
        }

        let written = self
            .inline
            .write_full(conversation_id, state, blob_store)
            .map_err(|error| error.to_string())?;
        if written {
            self.set_previous_root_prompt_count(state.root_prompt_messages_json.len());
        } else {
            warn_stale_mirror(conversation_id, "full-write-failed", None);
        }
        Ok(())
    }

    pub fn write_worker<Backend>(
        &self,
        conversation_id: &str,
        state: &LegacyTranscriptState,
        blob_store: &WorkerBlobStore<Backend>,
        state_blob_id: Option<&[u8]>,
    ) -> Result<(), String>
    where
        Backend: AgentBlobWorkerBackend + 'static,
    {
        let _lane = self
            .write_lane
            .lock()
            .map_err(|_| "transcript mirror write lane is poisoned".to_string())?;
        let previous = self.previous_root_prompt_count();
        match self
            .inline
            .write_incremental(conversation_id, state, blob_store, previous)
            .map_err(|error| error.to_string())?
        {
            Some(count) => {
                self.set_previous_root_prompt_count(count);
                return Ok(());
            }
            None => {}
        }

        let Some(state_blob_id) = state_blob_id.filter(|value| !value.is_empty()) else {
            warn_stale_mirror(conversation_id, "worker-unavailable", None);
            return Ok(());
        };

        let result = self.pool.write(TranscriptMirrorWorkerJob {
            conversation_id: conversation_id.to_string(),
            state_blob_id: state_blob_id.to_vec(),
            blob_db_paths: self.options.blob_db_paths.clone(),
            transcripts_dir: self.options.transcripts_dir.clone(),
        });
        match result {
            Ok(true) => {
                self.set_previous_root_prompt_count(state.root_prompt_messages_json.len());
            }
            Ok(false) => {
                warn_stale_mirror(conversation_id, "worker-write-failed", None);
            }
            Err(error) => {
                warn_stale_mirror(conversation_id, "worker-write-failed", Some("Error"));
                let _ = error;
            }
        }
        Ok(())
    }

    fn set_previous_root_prompt_count(&self, count: usize) {
        if let Ok(mut previous) = self.previous_root_prompt_count.lock() {
            *previous = count;
        }
    }
}

impl<Backend> LegacyTranscriptBlobStore for WorkerBlobStore<Backend>
where
    Backend: AgentBlobWorkerBackend + 'static,
{
    fn get_blob(&self, blob_id: &[u8]) -> Result<Option<Vec<u8>>, String> {
        self.get_blob_blocking(blob_id)
            .map_err(|_| "worker blob read failed".to_string())
    }
}

fn warn_stale_mirror(
    conversation_id: &str,
    reason: &str,
    error_class: Option<&str>,
) {
    let mut fields = Map::new();
    fields.insert(
        "agentId".into(),
        Value::String(conversation_id.to_string()),
    );
    fields.insert("reason".into(), Value::String(reason.to_string()));
    if let Some(error_class) = error_class {
        fields.insert(
            "errorClass".into(),
            Value::String(error_class.to_string()),
        );
    }
    report_host_diagnostic(&HostDiagnostic {
        kind: "transcript_mirror_stale".into(),
        fields,
    });
}
