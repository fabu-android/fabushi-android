use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub type AgentWorkerFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub const DEFAULT_IDLE_TIMEOUT_MS: u64 = 5 * 60_000;
pub const DEFAULT_MAX_WORKERS: usize = 64;
pub const DEFAULT_SWEEP_INTERVAL_MS: u64 = 30_000;
pub const DEFAULT_BUSY_TIMEOUT_MS: u64 = 5_000;

pub trait AgentBlobWorkerBackend: Send + Sync {
    type Error: Send + Sync + 'static;

    fn get_blob<'a>(
        &'a self,
        agent_id: &'a str,
        blob_db_path: &'a Path,
        blob_id: &'a [u8],
        legacy_blob_db_path: Option<&'a Path>,
    ) -> AgentWorkerFuture<'a, Result<Option<Vec<u8>>, Self::Error>>;

    fn set_blob<'a>(
        &'a self,
        agent_id: &'a str,
        blob_db_path: &'a Path,
        blob_id: &'a [u8],
        blob_data: &'a [u8],
        legacy_blob_db_path: Option<&'a Path>,
    ) -> AgentWorkerFuture<'a, Result<(), Self::Error>>;

    fn find_latest_root_blob_id<'a>(
        &'a self,
        agent_id: &'a str,
        blob_db_path: &'a Path,
        legacy_blob_db_path: Option<&'a Path>,
    ) -> AgentWorkerFuture<'a, Result<Option<Vec<u8>>, Self::Error>>;

    fn clear_blobs<'a>(
        &'a self,
        agent_id: &'a str,
        blob_db_path: &'a Path,
        legacy_blob_db_path: Option<&'a Path>,
    ) -> AgentWorkerFuture<'a, Result<(), Self::Error>>;

    fn clear_stale_checkpoint_roots<'a>(
        &'a self,
        agent_id: &'a str,
        blob_db_path: &'a Path,
        retained_root_id_hex: &'a str,
        legacy_blob_db_path: Option<&'a Path>,
    ) -> AgentWorkerFuture<'a, Result<usize, Self::Error>>;

    fn collect_conversation_garbage<'a>(
        &'a self,
        agent_id: &'a str,
        blob_db_path: &'a Path,
        retained_root_id_hex: &'a str,
        pending_write_retention_ms: u64,
        legacy_blob_db_path: Option<&'a Path>,
    ) -> AgentWorkerFuture<'a, Result<ConversationGarbageCollectionOutcome, Self::Error>>;

    fn verify_legacy_blob_retirement<'a>(
        &'a self,
        agent_id: &'a str,
        blob_db_path: &'a Path,
        retained_root_id_hex: &'a str,
        legacy_blob_db_path: &'a Path,
    ) -> AgentWorkerFuture<'a, Result<LegacyBlobRetirementVerdict, Self::Error>>;

    fn close_store<'a>(
        &'a self,
        agent_id: &'a str,
        blob_db_path: &'a Path,
    ) -> AgentWorkerFuture<'a, Result<(), Self::Error>>;
}

#[derive(Debug)]
pub enum AgentWorkerPoolError<BackendError> {
    Backend(BackendError),
    Closed,
    WorkerUnavailable(String),
}

impl<BackendError: fmt::Display> fmt::Display for AgentWorkerPoolError<BackendError> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(error) => error.fmt(formatter),
            Self::Closed => formatter.write_str("agent worker pool is closed"),
            Self::WorkerUnavailable(message) => formatter.write_str(message),
        }
    }
}

impl<BackendError> std::error::Error for AgentWorkerPoolError<BackendError>
where
    BackendError: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Backend(error) => Some(error),
            Self::Closed | Self::WorkerUnavailable(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentWorkerPoolOptions {
    pub busy_timeout_ms: u64,
    pub idle_timeout_ms: u64,
    pub max_workers: usize,
    pub sweep_interval_ms: u64,
}

impl Default for AgentWorkerPoolOptions {
    fn default() -> Self {
        Self {
            busy_timeout_ms: DEFAULT_BUSY_TIMEOUT_MS,
            idle_timeout_ms: DEFAULT_IDLE_TIMEOUT_MS,
            max_workers: DEFAULT_MAX_WORKERS,
            sweep_interval_ms: DEFAULT_SWEEP_INTERVAL_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentWorkerDescription {
    pub blob_db_path: PathBuf,
    pub worker_id: u64,
    pub pid: u32,
    pub next_request_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationGarbageCollectionOutcome {
    Skipped {
        reason: String,
        unresolved_proto_refs: usize,
    },
    Collected {
        deleted_rows: usize,
        deleted_bytes: u64,
        live_rows: usize,
        live_bytes: u64,
        retained_pending_rows: usize,
        vacuumed: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyBlobRetirementVerdict {
    pub is_retirable: bool,
    pub reason: Option<String>,
    pub legacy_rows: u64,
    pub legacy_bytes: u64,
}

impl LegacyBlobRetirementVerdict {
    pub fn defer(reason: impl Into<String>) -> Self {
        Self {
            is_retirable: false,
            reason: Some(reason.into()),
            legacy_rows: 0,
            legacy_bytes: 0,
        }
    }

    pub fn retirable(legacy_rows: u64, legacy_bytes: u64) -> Self {
        Self {
            is_retirable: true,
            reason: None,
            legacy_rows,
            legacy_bytes,
        }
    }
}

#[derive(Debug, Clone)]
struct WorkerBoot {
    agent_id: String,
    blob_db_path: PathBuf,
    legacy_blob_db_path: Option<PathBuf>,
    busy_timeout_ms: u64,
}

enum WorkerRequest<BackendError> {
    Init {
        request_id: u64,
        reply: mpsc::SyncSender<(u64, u32)>,
    },
    GetBlob {
        request_id: u64,
        blob_id: Vec<u8>,
        reply: mpsc::SyncSender<Result<Option<Vec<u8>>, AgentWorkerPoolError<BackendError>>>,
    },
    SetBlob {
        request_id: u64,
        blob_id: Vec<u8>,
        blob_data: Vec<u8>,
        reply: mpsc::SyncSender<Result<(), AgentWorkerPoolError<BackendError>>>,
    },
    FindLatestRoot {
        request_id: u64,
        reply: mpsc::SyncSender<Result<Option<Vec<u8>>, AgentWorkerPoolError<BackendError>>>,
    },
    ClearBlobs {
        request_id: u64,
        reply: mpsc::SyncSender<Result<(), AgentWorkerPoolError<BackendError>>>,
    },
    ClearStaleRoots {
        request_id: u64,
        retained_root_id_hex: String,
        reply: mpsc::SyncSender<Result<usize, AgentWorkerPoolError<BackendError>>>,
    },
    CollectGarbage {
        request_id: u64,
        retained_root_id_hex: String,
        pending_write_retention_ms: u64,
        reply: mpsc::SyncSender<Result<ConversationGarbageCollectionOutcome, AgentWorkerPoolError<BackendError>>>,
    },
    VerifyLegacyBlobRetirement {
        request_id: u64,
        retained_root_id_hex: String,
        legacy_blob_db_path: PathBuf,
        reply: mpsc::SyncSender<Result<LegacyBlobRetirementVerdict, AgentWorkerPoolError<BackendError>>>,
    },
    Close {
        request_id: u64,
        reply: mpsc::SyncSender<()>,
    },
}

struct AgentWorkerConnection<BackendError>
where
    BackendError: Send + Sync + 'static,
{
    sender: mpsc::Sender<WorkerRequest<BackendError>>,
    next_request_id: AtomicU64,
    last_activity_at: Arc<AtomicU64>,
    is_dead: Arc<AtomicBool>,
    worker_id: u64,
    pid: u32,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl<BackendError> AgentWorkerConnection<BackendError>
where
    BackendError: Send + Sync + 'static,
{
    fn request_id(&self) -> u64 {
        self.next_request_id.fetch_add(1, Ordering::Relaxed)
    }

    fn touch(&self) {
        self.last_activity_at.store(now_ms(), Ordering::Release);
    }

    fn send_get(
        &self,
        blob_id: &[u8],
    ) -> Result<Option<Vec<u8>>, AgentWorkerPoolError<BackendError>> {
        if self.is_dead.load(Ordering::Acquire) {
            return Err(AgentWorkerPoolError::WorkerUnavailable(
                "agent worker is no longer running".into(),
            ));
        }
        self.touch();
        let request_id = self.request_id();
        let (reply, result) = mpsc::sync_channel(1);
        self.sender
            .send(WorkerRequest::GetBlob {
                request_id,
                blob_id: blob_id.to_vec(),
                reply,
            })
            .map_err(|_| AgentWorkerPoolError::WorkerUnavailable(
                "agent worker request channel is closed".into(),
            ))?;
        let result = result.recv().map_err(|_| {
            AgentWorkerPoolError::WorkerUnavailable(
                "agent worker exited before replying".into(),
            )
        })?;
        self.touch();
        result
    }

    fn send_set(
        &self,
        blob_id: &[u8],
        blob_data: &[u8],
    ) -> Result<(), AgentWorkerPoolError<BackendError>> {
        if self.is_dead.load(Ordering::Acquire) {
            return Err(AgentWorkerPoolError::WorkerUnavailable(
                "agent worker is no longer running".into(),
            ));
        }
        self.touch();
        let request_id = self.request_id();
        let (reply, result) = mpsc::sync_channel(1);
        self.sender
            .send(WorkerRequest::SetBlob {
                request_id,
                blob_id: blob_id.to_vec(),
                blob_data: blob_data.to_vec(),
                reply,
            })
            .map_err(|_| AgentWorkerPoolError::WorkerUnavailable(
                "agent worker request channel is closed".into(),
            ))?;
        let result = result.recv().map_err(|_| {
            AgentWorkerPoolError::WorkerUnavailable(
                "agent worker exited before replying".into(),
            )
        })?;
        self.touch();
        result
    }

    fn send_find_latest_root(&self) -> Result<Option<Vec<u8>>, AgentWorkerPoolError<BackendError>> {
        self.touch();
        let request_id = self.request_id();
        let (reply, result) = mpsc::sync_channel(1);
        self.sender.send(WorkerRequest::FindLatestRoot { request_id, reply })
            .map_err(|_| AgentWorkerPoolError::WorkerUnavailable("agent worker request channel is closed".into()))?;
        let result = result.recv().map_err(|_| AgentWorkerPoolError::WorkerUnavailable("agent worker exited before replying".into()))?;
        self.touch();
        result
    }

    fn send_clear_blobs(&self) -> Result<(), AgentWorkerPoolError<BackendError>> {
        self.touch();
        let request_id = self.request_id();
        let (reply, result) = mpsc::sync_channel(1);
        self.sender.send(WorkerRequest::ClearBlobs { request_id, reply })
            .map_err(|_| AgentWorkerPoolError::WorkerUnavailable("agent worker request channel is closed".into()))?;
        let result = result.recv().map_err(|_| AgentWorkerPoolError::WorkerUnavailable("agent worker exited before replying".into()))?;
        self.touch();
        result
    }

    fn send_clear_stale_roots(&self, retained_root_id_hex: &str) -> Result<usize, AgentWorkerPoolError<BackendError>> {
        self.touch();
        let request_id = self.request_id();
        let (reply, result) = mpsc::sync_channel(1);
        self.sender.send(WorkerRequest::ClearStaleRoots {
            request_id,
            retained_root_id_hex: retained_root_id_hex.to_string(),
            reply,
        }).map_err(|_| AgentWorkerPoolError::WorkerUnavailable("agent worker request channel is closed".into()))?;
        let result = result.recv().map_err(|_| AgentWorkerPoolError::WorkerUnavailable("agent worker exited before replying".into()))?;
        self.touch();
        result
    }

    fn send_collect_garbage(
        &self,
        retained_root_id_hex: &str,
        pending_write_retention_ms: u64,
    ) -> Result<ConversationGarbageCollectionOutcome, AgentWorkerPoolError<BackendError>> {
        self.touch();
        let request_id = self.request_id();
        let (reply, result) = mpsc::sync_channel(1);
        self.sender.send(WorkerRequest::CollectGarbage {
            request_id,
            retained_root_id_hex: retained_root_id_hex.to_string(),
            pending_write_retention_ms,
            reply,
        }).map_err(|_| AgentWorkerPoolError::WorkerUnavailable("agent worker request channel is closed".into()))?;
        let result = result.recv().map_err(|_| AgentWorkerPoolError::WorkerUnavailable("agent worker exited before replying".into()))?;
        self.touch();
        result
    }

    fn send_verify_legacy_blob_retirement(
        &self,
        retained_root_id_hex: &str,
        legacy_blob_db_path: &Path,
    ) -> Result<LegacyBlobRetirementVerdict, AgentWorkerPoolError<BackendError>> {
        self.touch();
        let request_id = self.request_id();
        let (reply, result) = mpsc::sync_channel(1);
        self.sender.send(WorkerRequest::VerifyLegacyBlobRetirement {
            request_id,
            retained_root_id_hex: retained_root_id_hex.to_string(),
            legacy_blob_db_path: legacy_blob_db_path.to_path_buf(),
            reply,
        }).map_err(|_| AgentWorkerPoolError::WorkerUnavailable("agent worker request channel is closed".into()))?;
        let result = result.recv().map_err(|_| AgentWorkerPoolError::WorkerUnavailable("agent worker exited before replying".into()))?;
        self.touch();
        result
    }

    fn init(&self) -> Result<(), AgentWorkerPoolError<BackendError>> {
        let request_id = self.request_id();
        let (reply, result) = mpsc::sync_channel(1);
        self.sender
            .send(WorkerRequest::Init { request_id, reply })
            .map_err(|_| AgentWorkerPoolError::WorkerUnavailable(
                "agent worker failed during init".into(),
            ))?;
        let (worker_id, pid) = result.recv().map_err(|_| {
            AgentWorkerPoolError::WorkerUnavailable(
                "agent worker exited during init".into(),
            )
        })?;
        if worker_id != self.worker_id || pid != self.pid {
            return Err(AgentWorkerPoolError::WorkerUnavailable(
                "agent worker returned inconsistent boot identity".into(),
            ));
        }
        self.touch();
        Ok(())
    }

    fn close(&self) {
        if !self.is_dead.swap(true, Ordering::AcqRel) {
            let request_id = self.request_id();
            let (reply, result) = mpsc::sync_channel(1);
            let _ = self.sender.send(WorkerRequest::Close { request_id, reply });
            let _ = result.recv_timeout(Duration::from_secs(2));
        }
        if let Some(join) = self
            .join
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            let _ = join.join();
        }
    }

    fn activity_at(&self) -> u64 {
        self.last_activity_at.load(Ordering::Acquire)
    }

    fn description(&self, blob_db_path: PathBuf) -> AgentWorkerDescription {
        AgentWorkerDescription {
            blob_db_path,
            worker_id: self.worker_id,
            pid: self.pid,
            next_request_id: self.next_request_id.load(Ordering::Acquire),
        }
    }
}

struct PoolState<BackendError>
where
    BackendError: Send + Sync + 'static,
{
    connections: BTreeMap<PathBuf, Arc<AgentWorkerConnection<BackendError>>>,
    active_ops: BTreeMap<PathBuf, usize>,
}

impl<BackendError> Default for PoolState<BackendError>
where
    BackendError: Send + Sync + 'static,
{
    fn default() -> Self {
        Self {
            connections: BTreeMap::new(),
            active_ops: BTreeMap::new(),
        }
    }
}

struct PoolInner<Backend>
where
    Backend: AgentBlobWorkerBackend + 'static,
{
    backend: Arc<Backend>,
    options: AgentWorkerPoolOptions,
    state: Mutex<PoolState<Backend::Error>>,
    next_worker_id: AtomicU64,
    sweep_started: AtomicBool,
    closed: AtomicBool,
}

pub struct AgentWorkerPool<Backend>
where
    Backend: AgentBlobWorkerBackend + 'static,
{
    inner: Arc<PoolInner<Backend>>,
}

impl<Backend> AgentWorkerPool<Backend>
where
    Backend: AgentBlobWorkerBackend + 'static,
{
    pub fn new(backend: Backend) -> Self {
        Self::with_options(backend, AgentWorkerPoolOptions::default())
    }

    pub fn with_options(backend: Backend, mut options: AgentWorkerPoolOptions) -> Self {
        options.max_workers = options.max_workers.max(1);
        options.sweep_interval_ms = options.sweep_interval_ms.max(1);
        Self {
            inner: Arc::new(PoolInner {
                backend: Arc::new(backend),
                options,
                state: Mutex::new(PoolState::default()),
                next_worker_id: AtomicU64::new(1),
                sweep_started: AtomicBool::new(false),
                closed: AtomicBool::new(false),
            }),
        }
    }

    pub fn backend(&self) -> &Backend {
        self.inner.backend.as_ref()
    }

    pub fn get_blob_blocking(
        &self,
        agent_id: &str,
        blob_db_path: &Path,
        blob_id: &[u8],
        legacy_blob_db_path: Option<&Path>,
    ) -> Result<Option<Vec<u8>>, AgentWorkerPoolError<Backend::Error>> {
        self.retain(blob_db_path);
        let result = self
            .ensure(agent_id, blob_db_path, legacy_blob_db_path)
            .and_then(|connection| connection.send_get(blob_id));
        self.release(blob_db_path);
        result
    }

    pub async fn get_blob(
        &self,
        agent_id: &str,
        blob_db_path: &Path,
        blob_id: &[u8],
        legacy_blob_db_path: Option<&Path>,
    ) -> Result<Option<Vec<u8>>, AgentWorkerPoolError<Backend::Error>> {
        self.get_blob_blocking(
            agent_id,
            blob_db_path,
            blob_id,
            legacy_blob_db_path,
        )
    }

    pub fn set_blob_blocking(
        &self,
        agent_id: &str,
        blob_db_path: &Path,
        blob_id: &[u8],
        blob_data: &[u8],
        legacy_blob_db_path: Option<&Path>,
    ) -> Result<(), AgentWorkerPoolError<Backend::Error>> {
        self.retain(blob_db_path);
        let result = self
            .ensure(agent_id, blob_db_path, legacy_blob_db_path)
            .and_then(|connection| connection.send_set(blob_id, blob_data));
        self.release(blob_db_path);
        result
    }

    pub async fn set_blob(
        &self,
        agent_id: &str,
        blob_db_path: &Path,
        blob_id: &[u8],
        blob_data: &[u8],
        legacy_blob_db_path: Option<&Path>,
    ) -> Result<(), AgentWorkerPoolError<Backend::Error>> {
        self.set_blob_blocking(
            agent_id,
            blob_db_path,
            blob_id,
            blob_data,
            legacy_blob_db_path,
        )
    }

    pub async fn find_latest_root_blob_id(
        &self,
        agent_id: &str,
        blob_db_path: &Path,
        legacy_blob_db_path: Option<&Path>,
    ) -> Result<Option<Vec<u8>>, AgentWorkerPoolError<Backend::Error>> {
        self.retain(blob_db_path);
        let result = self.ensure(agent_id, blob_db_path, legacy_blob_db_path)
            .and_then(|connection| connection.send_find_latest_root());
        self.release(blob_db_path);
        result
    }

    pub async fn clear_blobs(
        &self,
        agent_id: &str,
        blob_db_path: &Path,
        legacy_blob_db_path: Option<&Path>,
    ) -> Result<(), AgentWorkerPoolError<Backend::Error>> {
        self.retain(blob_db_path);
        let result = self.ensure(agent_id, blob_db_path, legacy_blob_db_path)
            .and_then(|connection| connection.send_clear_blobs());
        self.release(blob_db_path);
        result
    }

    pub async fn clear_stale_checkpoint_roots(
        &self,
        agent_id: &str,
        blob_db_path: &Path,
        retained_root_id_hex: &str,
        legacy_blob_db_path: Option<&Path>,
    ) -> Result<usize, AgentWorkerPoolError<Backend::Error>> {
        self.retain(blob_db_path);
        let result = self.ensure(agent_id, blob_db_path, legacy_blob_db_path)
            .and_then(|connection| connection.send_clear_stale_roots(retained_root_id_hex));
        self.release(blob_db_path);
        result
    }

    pub async fn collect_conversation_garbage(
        &self,
        agent_id: &str,
        blob_db_path: &Path,
        retained_root_id_hex: &str,
        pending_write_retention_ms: u64,
        legacy_blob_db_path: Option<&Path>,
    ) -> Result<ConversationGarbageCollectionOutcome, AgentWorkerPoolError<Backend::Error>> {
        self.retain(blob_db_path);
        let result = self.ensure(agent_id, blob_db_path, legacy_blob_db_path)
            .and_then(|connection| connection.send_collect_garbage(retained_root_id_hex, pending_write_retention_ms));
        self.release(blob_db_path);
        result
    }

    pub async fn verify_legacy_blob_retirement(
        &self,
        agent_id: &str,
        blob_db_path: &Path,
        retained_root_id_hex: &str,
        legacy_blob_db_path: &Path,
    ) -> Result<LegacyBlobRetirementVerdict, AgentWorkerPoolError<Backend::Error>> {
        self.retain(blob_db_path);
        let result = self.ensure(agent_id, blob_db_path, Some(legacy_blob_db_path))
            .and_then(|connection| connection.send_verify_legacy_blob_retirement(retained_root_id_hex, legacy_blob_db_path));
        self.release(blob_db_path);
        result
    }

    pub fn close_store_blocking(&self, blob_db_path: &Path) {
        let connection = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.active_ops.remove(blob_db_path);
            state.connections.remove(blob_db_path)
        };
        if let Some(connection) = connection {
            connection.close();
        }
    }

    pub async fn close_store(&self, blob_db_path: &Path) {
        self.close_store_blocking(blob_db_path);
    }

    pub async fn close_all(&self) {
        self.close_all_blocking();
    }

    pub fn active_worker_count(&self) -> usize {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .connections
            .len()
    }

    pub fn describe_workers(&self) -> Vec<AgentWorkerDescription> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .connections
            .iter()
            .map(|(path, connection)| connection.description(path.clone()))
            .collect()
    }

    pub fn sweep_idle_at(&self, observed_now_ms: u64) -> usize {
        let victims = take_idle_connections(
            &self.inner,
            observed_now_ms,
            self.inner.options.idle_timeout_ms,
        );
        let count = victims.len();
        for connection in victims {
            connection.close();
        }
        count
    }

    fn retain(&self, blob_db_path: &Path) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *state.active_ops.entry(blob_db_path.to_path_buf()).or_insert(0) += 1;
    }

    fn release(&self, blob_db_path: &Path) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(active) = state.active_ops.get_mut(blob_db_path) else {
            return;
        };
        *active = active.saturating_sub(1);
        if *active == 0 {
            state.active_ops.remove(blob_db_path);
        }
    }

    fn ensure(
        &self,
        agent_id: &str,
        blob_db_path: &Path,
        legacy_blob_db_path: Option<&Path>,
    ) -> Result<Arc<AgentWorkerConnection<Backend::Error>>, AgentWorkerPoolError<Backend::Error>> {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(AgentWorkerPoolError::Closed);
        }
        if let Some(existing) = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .connections
            .get(blob_db_path)
            .cloned()
        {
            return Ok(existing);
        }

        if let Some(victim) = take_capacity_victim(&self.inner) {
            victim.close();
        }

        let boot = WorkerBoot {
            agent_id: agent_id.to_string(),
            blob_db_path: blob_db_path.to_path_buf(),
            legacy_blob_db_path: legacy_blob_db_path.map(Path::to_path_buf),
            busy_timeout_ms: self.inner.options.busy_timeout_ms,
        };
        let worker_id = self.inner.next_worker_id.fetch_add(1, Ordering::Relaxed);
        let candidate = spawn_connection(Arc::clone(&self.inner.backend), worker_id, boot)?;
        let selected = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(existing) = state.connections.get(blob_db_path).cloned() {
                existing
            } else {
                state
                    .connections
                    .insert(blob_db_path.to_path_buf(), Arc::clone(&candidate));
                Arc::clone(&candidate)
            }
        };
        if !Arc::ptr_eq(&selected, &candidate) {
            candidate.close();
        }
        self.start_sweep();
        Ok(selected)
    }

    fn start_sweep(&self) {
        if self.inner.options.idle_timeout_ms == u64::MAX {
            return;
        }
        if self.inner.sweep_started.swap(true, Ordering::AcqRel) {
            return;
        }
        let weak = Arc::downgrade(&self.inner);
        let interval = self.inner.options.sweep_interval_ms;
        let _ = thread::Builder::new()
            .name("mahayana-agent-worker-sweep".into())
            .spawn(move || loop {
                thread::sleep(Duration::from_millis(interval));
                let Some(inner) = weak.upgrade() else {
                    break;
                };
                if inner.closed.load(Ordering::Acquire) {
                    break;
                }
                for connection in take_idle_connections(
                    &inner,
                    now_ms(),
                    inner.options.idle_timeout_ms,
                ) {
                    connection.close();
                }
            });
    }

    fn close_all_blocking(&self) {
        self.inner.closed.store(true, Ordering::Release);
        let all = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.active_ops.clear();
            std::mem::take(&mut state.connections)
                .into_values()
                .collect::<Vec<_>>()
        };
        for connection in all {
            connection.close();
        }
    }
}

impl<Backend> Drop for AgentWorkerPool<Backend>
where
    Backend: AgentBlobWorkerBackend + 'static,
{
    fn drop(&mut self) {
        self.close_all_blocking();
    }
}

fn spawn_connection<Backend>(
    backend: Arc<Backend>,
    worker_id: u64,
    boot: WorkerBoot,
) -> Result<Arc<AgentWorkerConnection<Backend::Error>>, AgentWorkerPoolError<Backend::Error>>
where
    Backend: AgentBlobWorkerBackend + 'static,
{
    let (sender, receiver) = mpsc::channel::<WorkerRequest<Backend::Error>>();
    let last_activity_at = Arc::new(AtomicU64::new(now_ms()));
    let is_dead = Arc::new(AtomicBool::new(false));
    let dead_for_worker = Arc::clone(&is_dead);
    let activity_for_worker = Arc::clone(&last_activity_at);
    let thread_name = format!("mahayana-agent-worker-{worker_id}");
    let pid = std::process::id();
    let worker_boot = boot.clone();

    let join = thread::Builder::new()
        .name(thread_name)
        .spawn(move || {
            while let Ok(request) = receiver.recv() {
                activity_for_worker.store(now_ms(), Ordering::Release);
                match request {
                    WorkerRequest::Init { request_id, reply } => {
                        let _ = request_id;
                        let _ = worker_boot.busy_timeout_ms;
                        let _ = reply.send((worker_id, pid));
                    }
                    WorkerRequest::GetBlob {
                        request_id,
                        blob_id,
                        reply,
                    } => {
                        let _ = request_id;
                        let result = futures::executor::block_on(backend.get_blob(
                            &worker_boot.agent_id,
                            &worker_boot.blob_db_path,
                            &blob_id,
                            worker_boot.legacy_blob_db_path.as_deref(),
                        ))
                        .map_err(AgentWorkerPoolError::Backend);
                        let _ = reply.send(result);
                    }
                    WorkerRequest::SetBlob {
                        request_id,
                        blob_id,
                        blob_data,
                        reply,
                    } => {
                        let _ = request_id;
                        let result = futures::executor::block_on(backend.set_blob(
                            &worker_boot.agent_id,
                            &worker_boot.blob_db_path,
                            &blob_id,
                            &blob_data,
                            worker_boot.legacy_blob_db_path.as_deref(),
                        ))
                        .map_err(AgentWorkerPoolError::Backend);
                        let _ = reply.send(result);
                    }
                    WorkerRequest::FindLatestRoot { request_id, reply } => {
                        let _ = request_id;
                        let result = futures::executor::block_on(backend.find_latest_root_blob_id(
                            &worker_boot.agent_id,
                            &worker_boot.blob_db_path,
                            worker_boot.legacy_blob_db_path.as_deref(),
                        )).map_err(AgentWorkerPoolError::Backend);
                        let _ = reply.send(result);
                    }
                    WorkerRequest::ClearBlobs { request_id, reply } => {
                        let _ = request_id;
                        let result = futures::executor::block_on(backend.clear_blobs(
                            &worker_boot.agent_id,
                            &worker_boot.blob_db_path,
                            worker_boot.legacy_blob_db_path.as_deref(),
                        )).map_err(AgentWorkerPoolError::Backend);
                        let _ = reply.send(result);
                    }
                    WorkerRequest::ClearStaleRoots { request_id, retained_root_id_hex, reply } => {
                        let _ = request_id;
                        let result = futures::executor::block_on(backend.clear_stale_checkpoint_roots(
                            &worker_boot.agent_id,
                            &worker_boot.blob_db_path,
                            &retained_root_id_hex,
                            worker_boot.legacy_blob_db_path.as_deref(),
                        )).map_err(AgentWorkerPoolError::Backend);
                        let _ = reply.send(result);
                    }
                    WorkerRequest::CollectGarbage { request_id, retained_root_id_hex, pending_write_retention_ms, reply } => {
                        let _ = request_id;
                        let result = futures::executor::block_on(backend.collect_conversation_garbage(
                            &worker_boot.agent_id,
                            &worker_boot.blob_db_path,
                            &retained_root_id_hex,
                            pending_write_retention_ms,
                            worker_boot.legacy_blob_db_path.as_deref(),
                        )).map_err(AgentWorkerPoolError::Backend);
                        let _ = reply.send(result);
                    }
                    WorkerRequest::VerifyLegacyBlobRetirement { request_id, retained_root_id_hex, legacy_blob_db_path, reply } => {
                        let _ = request_id;
                        let result = futures::executor::block_on(backend.verify_legacy_blob_retirement(
                            &worker_boot.agent_id,
                            &worker_boot.blob_db_path,
                            &retained_root_id_hex,
                            &legacy_blob_db_path,
                        )).map_err(AgentWorkerPoolError::Backend);
                        let _ = reply.send(result);
                    }
                    WorkerRequest::Close { request_id, reply } => {
                        let _ = request_id;
                        let _ = futures::executor::block_on(backend.close_store(
                            &worker_boot.agent_id,
                            &worker_boot.blob_db_path,
                        ));
                        let _ = reply.send(());
                        break;
                    }
                }
                activity_for_worker.store(now_ms(), Ordering::Release);
            }
            dead_for_worker.store(true, Ordering::Release);
        })
        .map_err(|error| AgentWorkerPoolError::WorkerUnavailable(error.to_string()))?;

    let connection = Arc::new(AgentWorkerConnection {
        sender,
        next_request_id: AtomicU64::new(1),
        last_activity_at,
        is_dead,
        worker_id,
        pid,
        join: Mutex::new(Some(join)),
    });
    connection.init()?;
    Ok(connection)
}

fn take_capacity_victim<Backend>(
    inner: &Arc<PoolInner<Backend>>,
) -> Option<Arc<AgentWorkerConnection<Backend::Error>>>
where
    Backend: AgentBlobWorkerBackend + 'static,
{
    let mut state = inner
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.connections.len() < inner.options.max_workers {
        return None;
    }
    let victim_path = state
        .connections
        .iter()
        .filter(|(path, _)| state.active_ops.get(*path).copied().unwrap_or(0) == 0)
        .min_by_key(|(_, connection)| connection.activity_at())
        .map(|(path, _)| path.clone())?;
    state.connections.remove(&victim_path)
}

fn take_idle_connections<Backend>(
    inner: &Arc<PoolInner<Backend>>,
    observed_now_ms: u64,
    idle_timeout_ms: u64,
) -> Vec<Arc<AgentWorkerConnection<Backend::Error>>>
where
    Backend: AgentBlobWorkerBackend + 'static,
{
    if idle_timeout_ms == u64::MAX {
        return Vec::new();
    }
    let mut state = inner
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let paths = state
        .connections
        .iter()
        .filter(|(path, connection)| {
            state.active_ops.get(*path).copied().unwrap_or(0) == 0
                && observed_now_ms.saturating_sub(connection.activity_at()) >= idle_timeout_ms
        })
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    paths
        .into_iter()
        .filter_map(|path| state.connections.remove(&path))
        .collect()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
