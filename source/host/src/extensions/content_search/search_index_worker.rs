use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use rusqlite::{Error, ErrorCode};

use super::search_index_db::{ensure_search_index_schema, open_search_index_db};
use super::search_index_writer::{SandSearchIndexWriter, SearchIndexJob};

#[derive(Debug, Clone)]
pub struct SearchIndexWorkerConfig {
    pub index_db_path: PathBuf,
    pub agents_root_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchIndexWorkerRequest {
    pub request_id: u64,
    pub job: SearchIndexJob,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchIndexWorkerResponse {
    pub request_id: u64,
    pub ok: bool,
    pub message: Option<String>,
    pub is_index_corrupt: bool,
}

pub fn is_sqlite_corrupt_error(error: &Error) -> bool {
    matches!(
        error,
        Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase,
                ..
            },
            _
        )
    )
}

pub fn run_worker_request(
    writer: &mut SandSearchIndexWriter,
    request: SearchIndexWorkerRequest,
) -> SearchIndexWorkerResponse {
    match writer.run_job(&request.job) {
        Ok(()) => SearchIndexWorkerResponse {
            request_id: request.request_id,
            ok: true,
            message: None,
            is_index_corrupt: false,
        },
        Err(error) => SearchIndexWorkerResponse {
            request_id: request.request_id,
            ok: false,
            message: Some(error.to_string()),
            is_index_corrupt: is_sqlite_corrupt_error(&error),
        },
    }
}

enum WorkerCommand {
    Request {
        request: SearchIndexWorkerRequest,
        respond_to: Sender<SearchIndexWorkerResponse>,
    },
    Stop,
}

pub struct SearchIndexWorker {
    command_tx: Sender<WorkerCommand>,
    join: Option<JoinHandle<()>>,
}

impl SearchIndexWorker {
    pub fn spawn(config: SearchIndexWorkerConfig) -> Result<Self, String> {
        let (command_tx, command_rx) = mpsc::channel::<WorkerCommand>();
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), String>>(1);

        let join = thread::Builder::new()
            .name("fabushi-search-index-worker".into())
            .spawn(move || worker_main(config, command_rx, ready_tx))
            .map_err(|error| error.to_string())?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                command_tx,
                join: Some(join),
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(error) => {
                let _ = join.join();
                Err(format!("search-index worker startup channel closed: {error}"))
            }
        }
    }

    pub fn post(
        &self,
        request: SearchIndexWorkerRequest,
    ) -> Result<SearchIndexWorkerResponse, String> {
        let (respond_to, response_rx) = mpsc::channel();
        self.command_tx
            .send(WorkerCommand::Request {
                request,
                respond_to,
            })
            .map_err(|_| "search-index worker is no longer running".to_string())?;
        response_rx
            .recv()
            .map_err(|_| "search-index worker exited before replying".to_string())
    }

    pub fn terminate(&mut self) {
        let _ = self.command_tx.send(WorkerCommand::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for SearchIndexWorker {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn worker_main(
    config: SearchIndexWorkerConfig,
    command_rx: Receiver<WorkerCommand>,
    ready_tx: mpsc::SyncSender<Result<(), String>>,
) {
    let db = match open_search_index_db(&config.index_db_path) {
        Ok(db) => db,
        Err(error) => {
            let _ = ready_tx.send(Err(error.to_string()));
            return;
        }
    };
    if let Err(error) = ensure_search_index_schema(&db) {
        let _ = ready_tx.send(Err(error.to_string()));
        return;
    }

    let mut writer = SandSearchIndexWriter::new(db, config.agents_root_dir);
    if ready_tx.send(Ok(())).is_err() {
        return;
    }

    while let Ok(command) = command_rx.recv() {
        match command {
            WorkerCommand::Request {
                request,
                respond_to,
            } => {
                let response = run_worker_request(&mut writer, request);
                let _ = respond_to.send(response);
            }
            WorkerCommand::Stop => break,
        }
    }
    writer.close();
}
