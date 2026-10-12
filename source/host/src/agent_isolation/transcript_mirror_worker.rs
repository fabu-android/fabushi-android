use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use crate::transcript_mirror::conversation_state_binary::decode_transcript_mirror_conversation_state;
use crate::transcript_mirror::legacy_transcript_mirror::{
    LegacyFileTranscriptMirror, LegacyTranscriptBlobStore, LegacyTranscriptState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptMirrorWorkerJob {
    pub conversation_id: String,
    pub state_blob_id: Vec<u8>,
    pub blob_db_paths: Vec<PathBuf>,
    pub transcripts_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TranscriptMirrorWorkerError {
    #[error("transcript mirror checkpoint blob is unavailable")]
    MissingTranscriptState,
    #[error("transcript mirror checkpoint protobuf is invalid: {0}")]
    InvalidTranscriptState(String),
    #[error("transcript mirror write failed: {0}")]
    MirrorWrite(String),
}

pub struct ReadOnlySqliteBlobStore {
    db_paths: Vec<PathBuf>,
    connections: Mutex<HashMap<PathBuf, Connection>>,
}

impl ReadOnlySqliteBlobStore {
    pub fn new(db_paths: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            db_paths: db_paths.into_iter().collect(),
            connections: Mutex::new(HashMap::new()),
        }
    }

    fn open_connection(path: &Path) -> Option<Connection> {
        if !path.is_file() {
            return None;
        }
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(path, flags).ok()?;
        connection
            .busy_timeout(Duration::from_millis(5_000))
            .ok()?;
        Some(connection)
    }
}

impl LegacyTranscriptBlobStore for ReadOnlySqliteBlobStore {
    fn get_blob(&self, blob_id: &[u8]) -> Result<Option<Vec<u8>>, String> {
        let key = to_hex(blob_id);
        for path in &self.db_paths {
            let mut connections = self
                .connections
                .lock()
                .map_err(|_| "transcript mirror read connection cache poisoned".to_string())?;
            if !connections.contains_key(path) {
                if let Some(connection) = Self::open_connection(path) {
                    connections.insert(path.clone(), connection);
                } else {
                    continue;
                }
            }
            let Some(connection) = connections.get(path) else {
                continue;
            };
            let value = connection
                .query_row(
                    "SELECT data FROM blobs WHERE id = ?1",
                    params![key],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional();
            match value {
                Ok(Some(bytes)) => return Ok(Some(bytes)),
                Ok(None) | Err(_) => {}
            }
        }
        Ok(None)
    }
}

pub fn run_transcript_mirror_worker_job(
    job: &TranscriptMirrorWorkerJob,
) -> Result<bool, TranscriptMirrorWorkerError> {
    let blob_store = ReadOnlySqliteBlobStore::new(job.blob_db_paths.clone());
    let state_binary = blob_store
        .get_blob(&job.state_blob_id)
        .map_err(TranscriptMirrorWorkerError::MirrorWrite)?
        .ok_or(TranscriptMirrorWorkerError::MissingTranscriptState)?;
    let state = decode_transcript_mirror_conversation_state(&state_binary)
        .map_err(|error| TranscriptMirrorWorkerError::InvalidTranscriptState(error.to_string()))?;
    LegacyFileTranscriptMirror::new(&job.transcripts_dir)
        .write_full(
            &job.conversation_id,
            &LegacyTranscriptState {
                root_prompt_messages_json: state.root_prompt_messages_json,
                summary_archives: state.summary_archives,
                turns: state.turns,
            },
            &blob_store,
        )
        .map_err(|error| TranscriptMirrorWorkerError::MirrorWrite(error.to_string()))
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
