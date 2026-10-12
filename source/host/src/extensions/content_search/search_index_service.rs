use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rusqlite::Connection;

use super::search_index_db::{
    MessageSearchResult, MediaSearchResult, SEARCH_INDEX_SCHEMA_VERSION,
    ensure_search_index_schema, open_search_index_db, read_reconcile_done,
    read_search_index_schema_version, search_media, search_messages,
    stamp_search_index_schema_version,
};
use super::search_index_worker::{
    SearchIndexWorker, SearchIndexWorkerConfig, SearchIndexWorkerRequest,
    is_sqlite_corrupt_error,
};
use super::search_index_writer::{IndexEntry, SearchIndexJob};

pub const MAX_INDEX_REBUILDS: usize = 3;
pub const MAX_WORKER_RESPAWNS: usize = 3;
pub const MAX_FAILED_JOB_RECONCILES: usize = 3;
pub const SEARCH_INDEX_DISPOSE_TIMEOUT_MS: u64 = 2_000;
pub const SQLITE_DB_SIDECAR_SUFFIXES: &[&str] = &["-wal", "-shm", "-journal"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchIndexJobFailure {
    pub message: String,
    pub is_index_corrupt: bool,
    pub is_worker_unavailable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchIndexJobResult {
    Ok,
    Failure(SearchIndexJobFailure),
}

pub trait SearchIndexJobPort: Send {
    fn post(&mut self, job: &SearchIndexJob) -> SearchIndexJobResult;
    fn terminate(&mut self);
}

pub type SearchIndexJobPortFactory = Arc<
    dyn Fn(&SearchIndexWorkerConfig) -> Result<Box<dyn SearchIndexJobPort>, String>
        + Send
        + Sync,
>;

struct WorkerSearchIndexJobPort {
    worker: SearchIndexWorker,
    next_request_id: u64,
}

impl WorkerSearchIndexJobPort {
    fn create(config: &SearchIndexWorkerConfig) -> Result<Box<dyn SearchIndexJobPort>, String> {
        Ok(Box::new(Self {
            worker: SearchIndexWorker::spawn(config.clone())?,
            next_request_id: 1,
        }))
    }
}

impl SearchIndexJobPort for WorkerSearchIndexJobPort {
    fn post(&mut self, job: &SearchIndexJob) -> SearchIndexJobResult {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        match self.worker.post(SearchIndexWorkerRequest {
            request_id,
            job: job.clone(),
        }) {
            Ok(response) if response.ok => SearchIndexJobResult::Ok,
            Ok(response) => SearchIndexJobResult::Failure(SearchIndexJobFailure {
                message: response
                    .message
                    .unwrap_or_else(|| "search index job failed".into()),
                is_index_corrupt: response.is_index_corrupt,
                is_worker_unavailable: false,
            }),
            Err(message) => SearchIndexJobResult::Failure(SearchIndexJobFailure {
                message,
                is_index_corrupt: false,
                is_worker_unavailable: true,
            }),
        }
    }

    fn terminate(&mut self) {
        self.worker.terminate();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TranscriptMutation {
    EntriesUpserted {
        agent_id: String,
        entries: Vec<IndexEntry>,
    },
    EntryDeleted {
        agent_id: String,
        entry_id: String,
    },
    ConversationCleared {
        agent_id: String,
    },
    AgentRemoved {
        agent_id: String,
    },
    AgentNeedsReindex {
        agent_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchIndexHealth {
    pub kind: String,
    pub stage: Option<String>,
    pub count: Option<usize>,
    pub error_class: Option<String>,
}

impl SearchIndexHealth {
    fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            stage: None,
            count: None,
            error_class: None,
        }
    }

    fn stage(mut self, value: impl Into<String>) -> Self {
        self.stage = Some(value.into());
        self
    }

    fn count(mut self, value: usize) -> Self {
        self.count = Some(value);
        self
    }

    fn error(mut self, value: impl Into<String>) -> Self {
        self.error_class = Some(value.into());
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchIndexServiceSnapshot {
    pub is_unavailable: bool,
    pub is_disposed: bool,
    pub is_reconcile_done: bool,
    pub is_rebuild_pending: bool,
    pub pending_reindex_count: usize,
    pub rebuild_count: usize,
    pub worker_respawn_count: usize,
    pub failed_job_reconcile_count: usize,
}

enum DispatcherCommand {
    Job(SearchIndexJob),
    Rebuild {
        stage: String,
        message: String,
    },
    Barrier(mpsc::SyncSender<()>),
    Stop(mpsc::SyncSender<()>),
}

pub struct SandSearchIndexService {
    index_db_path: PathBuf,
    agents_root_dir: PathBuf,
    db: Arc<Mutex<Option<Connection>>>,
    state: Arc<Mutex<SearchIndexServiceSnapshot>>,
    command_tx: mpsc::Sender<DispatcherCommand>,
    join: Mutex<Option<JoinHandle<()>>>,
    report: Arc<dyn Fn(SearchIndexHealth) + Send + Sync>,
}

impl SandSearchIndexService {
    pub fn new(
        index_db_path: impl Into<PathBuf>,
        agents_root_dir: impl Into<PathBuf>,
    ) -> Self {
        Self::new_with_reporter(
            index_db_path,
            agents_root_dir,
            Arc::new(|_| {}),
        )
    }

    pub fn new_with_reporter(
        index_db_path: impl Into<PathBuf>,
        agents_root_dir: impl Into<PathBuf>,
        report: Arc<dyn Fn(SearchIndexHealth) + Send + Sync>,
    ) -> Self {
        Self::new_with_factory(
            index_db_path,
            agents_root_dir,
            Arc::new(WorkerSearchIndexJobPort::create),
            report,
        )
    }

    pub fn new_with_factory(
        index_db_path: impl Into<PathBuf>,
        agents_root_dir: impl Into<PathBuf>,
        factory: SearchIndexJobPortFactory,
        report: Arc<dyn Fn(SearchIndexHealth) + Send + Sync>,
    ) -> Self {
        let index_db_path = index_db_path.into();
        let agents_root_dir = agents_root_dir.into();
        let db = Arc::new(Mutex::new(None));
        let state = Arc::new(Mutex::new(SearchIndexServiceSnapshot::default()));
        let (command_tx, command_rx) = mpsc::channel();

        let dispatcher_db = Arc::clone(&db);
        let dispatcher_state = Arc::clone(&state);
        let dispatcher_report = Arc::clone(&report);
        let dispatcher_index_path = index_db_path.clone();
        let dispatcher_agents_root = agents_root_dir.clone();
        let dispatcher_tx = command_tx.clone();
        let join = thread::Builder::new()
            .name("fabushi-search-index-service".into())
            .spawn(move || {
                dispatcher_main(
                    dispatcher_index_path,
                    dispatcher_agents_root,
                    dispatcher_db,
                    dispatcher_state,
                    command_rx,
                    dispatcher_tx,
                    factory,
                    dispatcher_report,
                )
            })
            .expect("search-index service dispatcher must spawn");

        Self {
            index_db_path,
            agents_root_dir,
            db,
            state,
            command_tx,
            join: Mutex::new(Some(join)),
            report,
        }
    }

    pub fn start(&self) {
        {
            let Ok(state) = self.state.lock() else {
                return;
            };
            if state.is_disposed || state.is_unavailable {
                return;
            }
        }

        let needs_open = self.db.lock().map(|db| db.is_none()).unwrap_or(false);
        if needs_open {
            match open_and_migrate(&self.index_db_path) {
                Ok(connection) => {
                    let reconcile_done = read_reconcile_done(&connection).unwrap_or(false);
                    if let Ok(mut db) = self.db.lock() {
                        *db = Some(connection);
                    }
                    if let Ok(mut state) = self.state.lock() {
                        state.is_reconcile_done = reconcile_done;
                    }
                }
                Err(error) => {
                    mark_unavailable_shared(
                        &self.db,
                        &self.state,
                        &self.report,
                        "open",
                        error,
                    );
                    return;
                }
            }
        }
        self.enqueue(SearchIndexJob::Reconcile);
    }

    pub fn is_search_ready(&self) -> bool {
        let has_db = self.db.lock().map(|db| db.is_some()).unwrap_or(false);
        let Ok(state) = self.state.lock() else {
            return false;
        };
        has_db
            && !state.is_unavailable
            && state.is_reconcile_done
            && state.pending_reindex_count == 0
    }

    pub fn snapshot(&self) -> SearchIndexServiceSnapshot {
        self.state.lock().map(|state| state.clone()).unwrap_or_default()
    }

    pub fn search_messages(&self, query: &str, limit: usize) -> Option<Vec<MessageSearchResult>> {
        if self.snapshot().is_unavailable {
            return None;
        }
        let result = {
            let Ok(db) = self.db.lock() else {
                return None;
            };
            let connection = db.as_ref()?;
            search_messages(connection, query, limit)
        };
        match result {
            Ok(rows) => Some(rows),
            Err(error) => {
                self.handle_index_failure("search-messages", &error);
                None
            }
        }
    }

    pub fn search_media(&self, query: &str, limit: usize) -> Option<Vec<MediaSearchResult>> {
        if self.snapshot().is_unavailable {
            return None;
        }
        let result = {
            let Ok(db) = self.db.lock() else {
                return None;
            };
            let connection = db.as_ref()?;
            search_media(connection, query, limit)
        };
        match result {
            Ok(rows) => Some(rows),
            Err(error) => {
                self.handle_index_failure("search-media", &error);
                None
            }
        }
    }

    pub fn apply_mutation(&self, mutation: TranscriptMutation) {
        match mutation {
            TranscriptMutation::EntriesUpserted { agent_id, entries } => {
                if !entries.is_empty() {
                    self.enqueue(SearchIndexJob::UpsertEntries { agent_id, entries });
                }
            }
            TranscriptMutation::EntryDeleted { agent_id, entry_id } => {
                self.enqueue(SearchIndexJob::DeleteEntry { agent_id, entry_id });
            }
            TranscriptMutation::ConversationCleared { agent_id }
            | TranscriptMutation::AgentRemoved { agent_id } => {
                self.enqueue(SearchIndexJob::ClearAgent { agent_id });
            }
            TranscriptMutation::AgentNeedsReindex { agent_id } => {
                let has_db = self.db.lock().map(|db| db.is_some()).unwrap_or(false);
                let Ok(mut state) = self.state.lock() else {
                    return;
                };
                if state.is_disposed || state.is_unavailable || !has_db {
                    return;
                }
                state.pending_reindex_count = state.pending_reindex_count.saturating_add(1);
                drop(state);
                self.enqueue(SearchIndexJob::ReindexAgents {
                    agent_ids: vec![agent_id],
                });
            }
        }
    }

    pub fn when_idle(&self) -> bool {
        let (tx, rx) = mpsc::sync_channel(1);
        if self.command_tx.send(DispatcherCommand::Barrier(tx)).is_err() {
            return false;
        }
        rx.recv().is_ok()
    }

    pub fn dispose(&self) {
        let already_disposed = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            if state.is_disposed {
                true
            } else {
                state.is_disposed = true;
                false
            }
        };
        if already_disposed {
            return;
        }

        let (barrier_tx, barrier_rx) = mpsc::sync_channel(1);
        if self
            .command_tx
            .send(DispatcherCommand::Barrier(barrier_tx))
            .is_ok()
            && barrier_rx
                .recv_timeout(Duration::from_millis(SEARCH_INDEX_DISPOSE_TIMEOUT_MS))
                .is_err()
        {
            (self.report)(SearchIndexHealth::new("dispose_drain_cut").error("deadline"));
        }

        let (stop_tx, stop_rx) = mpsc::sync_channel(1);
        let _ = self.command_tx.send(DispatcherCommand::Stop(stop_tx));
        let _ = stop_rx.recv_timeout(Duration::from_millis(
            SEARCH_INDEX_DISPOSE_TIMEOUT_MS,
        ));
        if let Ok(mut join) = self.join.lock() {
            if let Some(handle) = join.take() {
                let _ = handle.join();
            }
        }
        if let Ok(mut db) = self.db.lock() {
            db.take();
        }
    }

    pub fn agents_root_dir(&self) -> &Path {
        &self.agents_root_dir
    }

    fn enqueue(&self, job: SearchIndexJob) {
        let Ok(state) = self.state.lock() else {
            return;
        };
        if state.is_disposed || state.is_unavailable {
            return;
        }
        drop(state);
        if self.command_tx.send(DispatcherCommand::Job(job)).is_err() {
            (self.report)(SearchIndexHealth::new("dispatch_failed").error("channel_closed"));
        }
    }

    fn handle_index_failure(&self, stage: &str, error: &rusqlite::Error) {
        let snapshot = self.snapshot();
        if snapshot.is_disposed || snapshot.is_unavailable {
            return;
        }
        if is_sqlite_corrupt_error(error) {
            let _ = self.command_tx.send(DispatcherCommand::Rebuild {
                stage: stage.into(),
                message: error.to_string(),
            });
        } else {
            (self.report)(
                SearchIndexHealth::new("stage_failed")
                    .stage(stage)
                    .error(error.to_string()),
            );
        }
    }
}

impl Drop for SandSearchIndexService {
    fn drop(&mut self) {
        self.dispose();
    }
}

fn dispatcher_main(
    index_db_path: PathBuf,
    agents_root_dir: PathBuf,
    db: Arc<Mutex<Option<Connection>>>,
    state: Arc<Mutex<SearchIndexServiceSnapshot>>,
    command_rx: mpsc::Receiver<DispatcherCommand>,
    command_tx: mpsc::Sender<DispatcherCommand>,
    factory: SearchIndexJobPortFactory,
    report: Arc<dyn Fn(SearchIndexHealth) + Send + Sync>,
) {
    let config = SearchIndexWorkerConfig {
        index_db_path: index_db_path.clone(),
        agents_root_dir,
    };
    let mut port: Option<Box<dyn SearchIndexJobPort>> = None;

    while let Ok(command) = command_rx.recv() {
        match command {
            DispatcherCommand::Job(job) => run_job(
                job,
                &config,
                &index_db_path,
                &db,
                &state,
                &command_tx,
                &factory,
                &report,
                &mut port,
            ),
            DispatcherCommand::Rebuild { stage, message } => schedule_rebuild(
                &stage,
                &message,
                &index_db_path,
                &db,
                &state,
                &command_tx,
                &report,
                &mut port,
            ),
            DispatcherCommand::Barrier(done) => {
                let _ = done.send(());
            }
            DispatcherCommand::Stop(done) => {
                if let Some(mut current) = port.take() {
                    current.terminate();
                }
                let _ = done.send(());
                break;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_job(
    job: SearchIndexJob,
    config: &SearchIndexWorkerConfig,
    index_db_path: &Path,
    db: &Arc<Mutex<Option<Connection>>>,
    state: &Arc<Mutex<SearchIndexServiceSnapshot>>,
    command_tx: &mpsc::Sender<DispatcherCommand>,
    factory: &SearchIndexJobPortFactory,
    report: &Arc<dyn Fn(SearchIndexHealth) + Send + Sync>,
    port: &mut Option<Box<dyn SearchIndexJobPort>>,
) {
    let is_reindex = matches!(job, SearchIndexJob::ReindexAgents { .. });
    let has_db = db.lock().map(|db| db.is_some()).unwrap_or(false);
    let should_run = state
        .lock()
        .map(|state| !state.is_disposed && !state.is_unavailable && has_db)
        .unwrap_or(false);

    if should_run {
        dispatch_job(
            &job,
            config,
            index_db_path,
            db,
            state,
            command_tx,
            factory,
            report,
            port,
        );
    }

    if is_reindex {
        if let Ok(mut state) = state.lock() {
            state.pending_reindex_count = state.pending_reindex_count.saturating_sub(1);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn dispatch_job(
    job: &SearchIndexJob,
    config: &SearchIndexWorkerConfig,
    index_db_path: &Path,
    db: &Arc<Mutex<Option<Connection>>>,
    state: &Arc<Mutex<SearchIndexServiceSnapshot>>,
    command_tx: &mpsc::Sender<DispatcherCommand>,
    factory: &SearchIndexJobPortFactory,
    report: &Arc<dyn Fn(SearchIndexHealth) + Send + Sync>,
    port: &mut Option<Box<dyn SearchIndexJobPort>>,
) {
    if port.is_none() {
        match factory(config) {
            Ok(created) => *port = Some(created),
            Err(error) => {
                handle_worker_unavailable(error, db, state, command_tx, report, port);
                return;
            }
        }
    }

    let result = port
        .as_mut()
        .map(|port| port.post(job))
        .unwrap_or_else(|| {
            SearchIndexJobResult::Failure(SearchIndexJobFailure {
                message: "search-index worker is no longer running".into(),
                is_index_corrupt: false,
                is_worker_unavailable: true,
            })
        });

    match result {
        SearchIndexJobResult::Ok => {
            if matches!(job, SearchIndexJob::Reconcile) {
                if let Ok(mut state) = state.lock() {
                    state.is_reconcile_done = true;
                }
            }
        }
        SearchIndexJobResult::Failure(failure) if failure.is_index_corrupt => {
            schedule_rebuild(
                &format!("job-{}", job_kind(job)),
                &failure.message,
                index_db_path,
                db,
                state,
                command_tx,
                report,
                port,
            );
        }
        SearchIndexJobResult::Failure(failure) if failure.is_worker_unavailable => {
            handle_worker_unavailable(
                failure.message,
                db,
                state,
                command_tx,
                report,
                port,
            );
        }
        SearchIndexJobResult::Failure(failure) => {
            let count = {
                let Ok(mut state) = state.lock() else {
                    return;
                };
                state.failed_job_reconcile_count =
                    state.failed_job_reconcile_count.saturating_add(1);
                state.failed_job_reconcile_count
            };
            if count > MAX_FAILED_JOB_RECONCILES {
                mark_unavailable_with_port(
                    db,
                    state,
                    report,
                    port,
                    &format!("job-{}", job_kind(job)),
                    failure.message,
                );
                return;
            }

            if matches!(job, SearchIndexJob::ReindexAgents { .. }) {
                (report)(
                    SearchIndexHealth::new("job_retry")
                        .stage(job_kind(job))
                        .count(count),
                );
                if let Ok(mut state) = state.lock() {
                    state.pending_reindex_count =
                        state.pending_reindex_count.saturating_add(1);
                }
                let _ = command_tx.send(DispatcherCommand::Job(job.clone()));
            } else {
                (report)(
                    SearchIndexHealth::new("job_reconcile")
                        .stage(job_kind(job))
                        .count(count),
                );
                if let Ok(mut state) = state.lock() {
                    state.is_reconcile_done = false;
                }
                let _ = command_tx.send(DispatcherCommand::Job(SearchIndexJob::Reconcile));
            }
        }
    }
}

fn handle_worker_unavailable(
    message: String,
    db: &Arc<Mutex<Option<Connection>>>,
    state: &Arc<Mutex<SearchIndexServiceSnapshot>>,
    command_tx: &mpsc::Sender<DispatcherCommand>,
    report: &Arc<dyn Fn(SearchIndexHealth) + Send + Sync>,
    port: &mut Option<Box<dyn SearchIndexJobPort>>,
) {
    if let Some(mut current) = port.take() {
        current.terminate();
    }
    let count = {
        let Ok(mut state) = state.lock() else {
            return;
        };
        state.worker_respawn_count = state.worker_respawn_count.saturating_add(1);
        state.worker_respawn_count
    };
    if count > MAX_WORKER_RESPAWNS {
        mark_unavailable_with_port(
            db,
            state,
            report,
            port,
            "worker-respawn",
            message,
        );
        return;
    }
    (report)(SearchIndexHealth::new("worker_respawn").count(count));
    if let Ok(mut state) = state.lock() {
        state.is_reconcile_done = false;
    }
    let _ = command_tx.send(DispatcherCommand::Job(SearchIndexJob::Reconcile));
}

#[allow(clippy::too_many_arguments)]
fn schedule_rebuild(
    stage: &str,
    message: &str,
    index_db_path: &Path,
    db: &Arc<Mutex<Option<Connection>>>,
    state: &Arc<Mutex<SearchIndexServiceSnapshot>>,
    command_tx: &mpsc::Sender<DispatcherCommand>,
    report: &Arc<dyn Fn(SearchIndexHealth) + Send + Sync>,
    port: &mut Option<Box<dyn SearchIndexJobPort>>,
) {
    let (count, exceeded) = {
        let Ok(mut state) = state.lock() else {
            return;
        };
        if state.is_disposed || state.is_unavailable || state.is_rebuild_pending {
            return;
        }
        state.rebuild_count = state.rebuild_count.saturating_add(1);
        let count = state.rebuild_count;
        if count > MAX_INDEX_REBUILDS {
            (count, true)
        } else {
            state.is_reconcile_done = false;
            state.is_rebuild_pending = true;
            (count, false)
        }
    };

    if exceeded {
        mark_unavailable_with_port(
            db,
            state,
            report,
            port,
            stage,
            message.to_string(),
        );
        return;
    }

    (report)(
        SearchIndexHealth::new("corrupt_rebuild")
            .stage(stage)
            .count(count),
    );
    if let Some(mut current) = port.take() {
        current.terminate();
    }
    if let Ok(mut reader) = db.lock() {
        reader.take();
    }

    match recreate_index_file(index_db_path) {
        Ok(connection) => {
            if let Ok(mut reader) = db.lock() {
                *reader = Some(connection);
            }
            if let Ok(mut state) = state.lock() {
                state.is_rebuild_pending = false;
            }
            let _ = command_tx.send(DispatcherCommand::Job(SearchIndexJob::Reconcile));
        }
        Err(error) => {
            if let Ok(mut state) = state.lock() {
                state.is_rebuild_pending = false;
                state.is_unavailable = true;
            }
            (report)(
                SearchIndexHealth::new("unavailable")
                    .stage("rebuild")
                    .error(error),
            );
        }
    }
}

fn mark_unavailable_with_port(
    db: &Arc<Mutex<Option<Connection>>>,
    state: &Arc<Mutex<SearchIndexServiceSnapshot>>,
    report: &Arc<dyn Fn(SearchIndexHealth) + Send + Sync>,
    port: &mut Option<Box<dyn SearchIndexJobPort>>,
    stage: &str,
    error: String,
) {
    if let Some(mut current) = port.take() {
        current.terminate();
    }
    mark_unavailable_shared(db, state, report, stage, error);
}

fn mark_unavailable_shared(
    db: &Arc<Mutex<Option<Connection>>>,
    state: &Arc<Mutex<SearchIndexServiceSnapshot>>,
    report: &Arc<dyn Fn(SearchIndexHealth) + Send + Sync>,
    stage: &str,
    error: String,
) {
    if let Ok(mut reader) = db.lock() {
        reader.take();
    }
    if let Ok(mut state) = state.lock() {
        state.is_unavailable = true;
    }
    (report)(
        SearchIndexHealth::new("unavailable")
            .stage(stage)
            .error(error),
    );
}

fn job_kind(job: &SearchIndexJob) -> &'static str {
    match job {
        SearchIndexJob::UpsertEntries { .. } => "upsert-entries",
        SearchIndexJob::DeleteEntry { .. } => "delete-entry",
        SearchIndexJob::ClearAgent { .. } => "clear-agent",
        SearchIndexJob::ReindexAgents { .. } => "reindex-agents",
        SearchIndexJob::Reconcile => "reconcile",
    }
}

fn open_and_migrate(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let db = match open_search_index_db(path).and_then(|db| {
        ensure_search_index_schema(&db)?;
        Ok(db)
    }) {
        Ok(db) => db,
        Err(error) => {
            if is_fts5_missing_error(&error.to_string()) {
                return Err(error.to_string());
            }
            return recreate_index_file(path);
        }
    };

    let version = read_search_index_schema_version(&db).map_err(|error| error.to_string())?;
    if version != SEARCH_INDEX_SCHEMA_VERSION {
        let fresh = version == 0 && count_indexed_messages(&db) == 0;
        if fresh {
            stamp_search_index_schema_version(&db).map_err(|error| error.to_string())?;
            return Ok(db);
        }
        drop(db);
        return recreate_index_file(path);
    }
    Ok(db)
}

fn recreate_index_file(path: &Path) -> Result<Connection, String> {
    remove_index_files(path);
    let db = open_search_index_db(path).map_err(|error| error.to_string())?;
    ensure_search_index_schema(&db).map_err(|error| error.to_string())?;
    stamp_search_index_schema_version(&db).map_err(|error| error.to_string())?;
    Ok(db)
}

fn remove_index_files(path: &Path) {
    let base = path.to_string_lossy();
    for suffix in std::iter::once("").chain(SQLITE_DB_SIDECAR_SUFFIXES.iter().copied()) {
        let _ = fs::remove_file(format!("{base}{suffix}"));
    }
}

fn count_indexed_messages(db: &Connection) -> i64 {
    db.query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
        .unwrap_or(0)
}

fn is_fts5_missing_error(message: &str) -> bool {
    message.contains("no such module: fts5")
}
