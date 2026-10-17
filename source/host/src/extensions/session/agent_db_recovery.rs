use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{Connection, params};
use serde_json::Value;

use crate::storage::sqlite_busy::{is_sqlite_corrupt_error, is_sqlite_io_error};
use crate::storage::sqlite_recovery::{
    copy_salvageable_sqlite_rows, open_sqlite_for_salvage, quarantine_corrupt_sqlite_db,
};
use crate::storage::store_db::DB_BUSY_TIMEOUT_MS;

use super::agent_db_schema::AGENT_DB_SCHEMA;
use super::session_diagnostics::{SessionDiagnostic, report_session_diagnostic};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SalvageCounts {
    pub kv: usize,
    pub blobs: usize,
    pub transcript: usize,
}

impl SalvageCounts {
    pub fn total(self) -> usize {
        self.kv + self.blobs + self.transcript
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorruptionRecoveredEvent {
    pub outcome: &'static str,
    pub quarantine_path: Option<PathBuf>,
    pub salvaged: SalvageCounts,
}

pub type CorruptionRecoveredCallback =
    Arc<dyn Fn(&CorruptionRecoveredEvent) + Send + Sync + 'static>;
pub type AgentNeedsReindexCallback = Arc<dyn Fn(&str) + Send + Sync + 'static>;

#[derive(Clone)]
pub struct DbRecoveryOptions {
    pub recover_on_corruption: bool,
    pub reject_if_corrupt: bool,
    pub verify_integrity_on_open: bool,
    pub busy_timeout_ms: u64,
    pub on_corruption_recovered: Option<CorruptionRecoveredCallback>,
    /// Android replacement for Desktop's process-global transcript mutation listener.
    /// The shipping Session owner wires this callback into its typed projection/event route.
    pub on_agent_needs_reindex: Option<AgentNeedsReindexCallback>,
}

impl Default for DbRecoveryOptions {
    fn default() -> Self {
        Self {
            recover_on_corruption: true,
            reject_if_corrupt: false,
            verify_integrity_on_open: true,
            busy_timeout_ms: DB_BUSY_TIMEOUT_MS,
            on_corruption_recovered: None,
            on_agent_needs_reindex: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AgentDbRecoveryError {
    #[error("agent store sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("agent store filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("store.db failed integrity check for {0}")]
    Integrity(String),
}

fn diagnostic(
    kind: &str,
    agent_id: &str,
    metadata: impl IntoIterator<Item = (String, Value)>,
) {
    let mut values = metadata
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    values.insert("agentId".into(), Value::String(agent_id.to_string()));
    report_session_diagnostic(&SessionDiagnostic {
        family: "store_db".into(),
        kind: kind.into(),
        metadata: values,
    });
}

pub fn init_configured_db(
    db_path: &Path,
    agent_dir_name: &str,
    options: &DbRecoveryOptions,
) -> Result<Connection, AgentDbRecoveryError> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let db = Connection::open(db_path)?;
    db.busy_timeout(Duration::from_millis(options.busy_timeout_ms))?;
    if let Err(error) = db.pragma_update(None, "journal_mode", "WAL") {
        diagnostic(
            "wal_unavailable",
            agent_dir_name,
            [("errorClass".into(), Value::String(error.to_string()))],
        );
    }
    if let Err(error) = db.pragma_update(None, "synchronous", "NORMAL") {
        diagnostic(
            "wal_unavailable",
            agent_dir_name,
            [("errorClass".into(), Value::String(error.to_string()))],
        );
    }
    db.execute_batch(AGENT_DB_SCHEMA)?;
    Ok(db)
}

pub fn is_store_db_healthy(db: &Connection) -> bool {
    db.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
        .map(|value| value == "ok")
        .unwrap_or(false)
}

fn sqlite_error(error: &AgentDbRecoveryError) -> Option<&rusqlite::Error> {
    match error {
        AgentDbRecoveryError::Sqlite(error) => Some(error),
        AgentDbRecoveryError::Io(_) | AgentDbRecoveryError::Integrity(_) => None,
    }
}

pub fn open_configured_db(
    db_path: &Path,
    agent_dir_name: &str,
    options: &DbRecoveryOptions,
    has_other_live_handles: bool,
) -> Result<Connection, AgentDbRecoveryError> {
    let recover = options.recover_on_corruption && !has_other_live_handles;
    let mut opened = None;
    let mut last_error = None;

    for attempt in 0..2 {
        match init_configured_db(db_path, agent_dir_name, options) {
            Ok(db) => {
                opened = Some(db);
                break;
            }
            Err(error) => {
                let retry_io = sqlite_error(&error).is_some_and(is_sqlite_io_error) && attempt == 0;
                if retry_io {
                    diagnostic(
                        "open_io_retry",
                        agent_dir_name,
                        [("errorClass".into(), Value::String(error.to_string()))],
                    );
                    last_error = Some(error);
                    continue;
                }
                last_error = Some(error);
                break;
            }
        }
    }

    let Some(db) = opened else {
        let error = last_error.expect("configured db open records an error");
        if recover && sqlite_error(&error).is_some_and(is_sqlite_corrupt_error) {
            return recover_corrupt_store_db(db_path, agent_dir_name, options, &error.to_string());
        }
        return Err(error);
    };

    let verify = (recover || options.reject_if_corrupt) && options.verify_integrity_on_open;
    if verify && !is_store_db_healthy(&db) {
        drop(db);
        if recover {
            diagnostic(
                "quick_check_failed",
                agent_dir_name,
                Vec::<(String, Value)>::new(),
            );
            return recover_corrupt_store_db(
                db_path,
                agent_dir_name,
                options,
                "PRAGMA quick_check failed",
            );
        }
        return Err(AgentDbRecoveryError::Integrity(agent_dir_name.to_string()));
    }
    Ok(db)
}

pub fn quarantine_corrupt_db(db_path: &Path, agent_dir_name: &str) -> Option<PathBuf> {
    let result = quarantine_corrupt_sqlite_db(db_path, None, false, 1, 50);
    if result.rename_error_code.is_none() {
        return result.quarantine_path;
    }
    let error_class = result
        .rename_error_code
        .clone()
        .unwrap_or_else(|| "unknown".into());
    if result.quarantine_path.is_none() {
        diagnostic(
            "quarantine_rename_failed",
            agent_dir_name,
            [("errorClass".into(), Value::String(error_class))],
        );
        return None;
    }
    diagnostic(
        "quarantine_copied",
        agent_dir_name,
        [("errorClass".into(), Value::String(error_class))],
    );
    result.quarantine_path
}

pub fn salvage_store_db(source_path: &Path, fresh_db: &Connection) -> SalvageCounts {
    let mut counts = SalvageCounts {
        kv: 0,
        blobs: 0,
        transcript: 0,
    };
    let Some(source) = open_sqlite_for_salvage(source_path, None) else {
        return counts;
    };

    counts.kv = copy_salvageable_sqlite_rows(
        &source,
        "SELECT key, value FROM kv",
        |row| {
            let key = row.get::<_, String>(0)?;
            let value = row.get::<_, String>(1)?;
            fresh_db
                .execute(
                    "INSERT OR IGNORE INTO kv (key, value) VALUES (?1, ?2)",
                    params![key, value],
                )
                .map(|_| ())
        },
    );
    counts.blobs = copy_salvageable_sqlite_rows(
        &source,
        "SELECT id, data FROM blobs",
        |row| {
            let id = row.get::<_, String>(0)?;
            let data = row.get::<_, Vec<u8>>(1)?;
            fresh_db
                .execute(
                    "INSERT OR IGNORE INTO blobs (id, data) VALUES (?1, ?2)",
                    params![id, data],
                )
                .map(|_| ())
        },
    );
    counts.transcript = copy_salvageable_sqlite_rows(
        &source,
        "SELECT seq, id, entry FROM transcript_entries",
        |row| {
            let seq = row.get::<_, i64>(0)?;
            let id = row.get::<_, String>(1)?;
            let entry = row.get::<_, String>(2)?;
            fresh_db
                .execute(
                    "INSERT OR IGNORE INTO transcript_entries (seq, id, entry) VALUES (?1, ?2, ?3)",
                    params![seq, id, entry],
                )
                .map(|_| ())
        },
    );
    counts
}

pub fn recover_corrupt_store_db(
    db_path: &Path,
    agent_dir_name: &str,
    options: &DbRecoveryOptions,
    cause: &str,
) -> Result<Connection, AgentDbRecoveryError> {
    diagnostic(
        "corrupt_recovering",
        agent_dir_name,
        [("errorClass".into(), Value::String(cause.to_string()))],
    );
    let quarantine_path = quarantine_corrupt_db(db_path, agent_dir_name);
    let fresh_db = init_configured_db(db_path, agent_dir_name, options)?;
    let salvaged = quarantine_path
        .as_deref()
        .map(|path| salvage_store_db(path, &fresh_db))
        .unwrap_or(SalvageCounts {
            kv: 0,
            blobs: 0,
            transcript: 0,
        });
    let outcome = if salvaged.total() > 0 {
        "recovered"
    } else {
        "reset"
    };
    diagnostic(
        "recovery_outcome",
        agent_dir_name,
        [
            ("outcome".into(), Value::String(outcome.into())),
            (
                "quarantine".into(),
                Value::String(if quarantine_path.is_some() {
                    "preserved".into()
                } else {
                    "none".into()
                }),
            ),
            ("salvagedKv".into(), Value::from(salvaged.kv as u64)),
            ("salvagedBlobs".into(), Value::from(salvaged.blobs as u64)),
            (
                "salvagedTranscript".into(),
                Value::from(salvaged.transcript as u64),
            ),
        ],
    );
    if let Some(callback) = options.on_corruption_recovered.as_ref() {
        let event = CorruptionRecoveredEvent {
            outcome,
            quarantine_path: quarantine_path.clone(),
            salvaged,
        };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            callback(&event)
        }));
    }

    if let Some(callback) = options.on_agent_needs_reindex.as_ref() {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            callback(agent_dir_name)
        }));
    }
    Ok(fresh_db)
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn salvage_preserves_valid_kv_blob_and_transcript_rows() {
        let root = tempfile::tempdir().unwrap();
        let source_path = root.path().join("source.db");
        let fresh_path = root.path().join("fresh.db");
        let options = DbRecoveryOptions::default();
        let source = init_configured_db(&source_path, "agent-a", &options).unwrap();
        source.execute("INSERT INTO kv (key, value) VALUES (?1, ?2)", params!["k", "v"]).unwrap();
        source.execute("INSERT INTO blobs (id, data) VALUES (?1, ?2)", params!["b", vec![1_u8, 2, 3]]).unwrap();
        source.execute(
            "INSERT INTO transcript_entries (id, entry) VALUES (?1, ?2)",
            params!["m", r#"{"kind":"message","content":"hello"}"#],
        ).unwrap();
        drop(source);

        let fresh = init_configured_db(&fresh_path, "agent-a", &options).unwrap();
        let counts = salvage_store_db(&source_path, &fresh);
        assert_eq!(counts, SalvageCounts { kv: 1, blobs: 1, transcript: 1 });
        assert_eq!(
            fresh.query_row("SELECT value FROM kv WHERE key='k'", [], |row| row.get::<_, String>(0)).unwrap(),
            "v"
        );
        assert_eq!(
            fresh.query_row("SELECT count(*) FROM transcript_entries", [], |row| row.get::<_, i64>(0)).unwrap(),
            1
        );
    }

    #[test]
    fn corrupt_store_recovery_fails_over_to_fresh_db_and_requests_reindex() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("store.db");
        std::fs::write(&path, b"this is not sqlite").unwrap();

        let recovered = Arc::new(Mutex::new(Vec::<String>::new()));
        let reindexed = Arc::new(Mutex::new(Vec::<String>::new()));
        let recovered_sink = Arc::clone(&recovered);
        let reindex_sink = Arc::clone(&reindexed);
        let options = DbRecoveryOptions {
            on_corruption_recovered: Some(Arc::new(move |event| {
                recovered_sink.lock().unwrap().push(event.outcome.to_string());
            })),
            on_agent_needs_reindex: Some(Arc::new(move |agent_id| {
                reindex_sink.lock().unwrap().push(agent_id.to_string());
            })),
            ..DbRecoveryOptions::default()
        };

        let db = open_configured_db(&path, "agent-a", &options, false).unwrap();
        assert!(is_store_db_healthy(&db));
        assert_eq!(recovered.lock().unwrap().len(), 1);
        assert_eq!(reindexed.lock().unwrap().as_slice(), &["agent-a".to_string()]);
    }
}
