use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, Error as SqliteError, Row};

const SQLITE_DB_SIDECAR_SUFFIXES: [&str; 3] = ["-wal", "-shm", "-journal"];

fn sidecar_path(db_path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{}", db_path.display(), suffix))
}

fn remove_force(path: &Path, recursive: bool) -> io::Result<()> {
    let result = if recursive && path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub fn remove_sqlite_sidecars(db_path: &Path) {
    for suffix in SQLITE_DB_SIDECAR_SUFFIXES {
        let _ = remove_force(&sidecar_path(db_path, suffix), true);
    }
}

pub fn remove_path_with_retries(
    path: &Path,
    attempts: usize,
    retry_delay_ms: u64,
    recursive: bool,
) -> io::Result<()> {
    let attempts = attempts.max(1);
    let mut failure = None;
    for attempt in 0..attempts {
        match remove_force(path, recursive) {
            Ok(()) => return Ok(()),
            Err(error) => {
                failure = Some(error);
                if attempt + 1 < attempts {
                    thread::sleep(Duration::from_millis(retry_delay_ms));
                }
            }
        }
    }
    Err(failure.expect("failed remove records the final error"))
}

pub fn remove_sqlite_db(
    db_path: &Path,
    attempts: usize,
    retry_delay_ms: u64,
) -> io::Result<()> {
    remove_path_with_retries(db_path, attempts, retry_delay_ms, false)?;
    for suffix in SQLITE_DB_SIDECAR_SUFFIXES {
        remove_path_with_retries(
            &sidecar_path(db_path, suffix),
            attempts,
            retry_delay_ms,
            true,
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarantineCorruptSqliteResult {
    pub quarantine_path: Option<PathBuf>,
    pub copied: bool,
    pub rename_error_code: Option<String>,
}

fn io_error_code(error: &io::Error) -> String {
    error
        .raw_os_error()
        .map(|value| value.to_string())
        .unwrap_or_else(|| format!("{:?}", error.kind()))
}

fn default_quarantine_path(db_path: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    PathBuf::from(format!("{}.corrupt-{stamp}", db_path.display()))
}

pub fn quarantine_corrupt_sqlite_db(
    db_path: &Path,
    quarantine_path: Option<&Path>,
    preserve_source_on_copy_failure: bool,
    remove_attempts: usize,
    remove_retry_delay_ms: u64,
) -> QuarantineCorruptSqliteResult {
    let quarantine_path = quarantine_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_quarantine_path(db_path));
    let _ = remove_force(&quarantine_path, false);
    remove_sqlite_sidecars(&quarantine_path);

    match fs::rename(db_path, &quarantine_path) {
        Ok(()) => {
            for suffix in SQLITE_DB_SIDECAR_SUFFIXES {
                let source = sidecar_path(db_path, suffix);
                if !source.exists() {
                    continue;
                }
                let target = sidecar_path(&quarantine_path, suffix);
                if fs::rename(&source, &target).is_err() {
                    let _ = remove_force(&source, true);
                }
            }
            QuarantineCorruptSqliteResult {
                quarantine_path: Some(quarantine_path),
                copied: false,
                rename_error_code: None,
            }
        }
        Err(rename_error) => {
            let rename_error_code = Some(io_error_code(&rename_error));
            let copied = fs::copy(db_path, &quarantine_path).is_ok();
            if copied {
                for suffix in SQLITE_DB_SIDECAR_SUFFIXES {
                    let source = sidecar_path(db_path, suffix);
                    if source.exists() {
                        let _ = fs::copy(&source, sidecar_path(&quarantine_path, suffix));
                    }
                }
            } else if preserve_source_on_copy_failure {
                return QuarantineCorruptSqliteResult {
                    quarantine_path: Some(db_path.to_path_buf()),
                    copied: false,
                    rename_error_code,
                };
            }
            let _ = remove_sqlite_db(db_path, remove_attempts, remove_retry_delay_ms);
            QuarantineCorruptSqliteResult {
                quarantine_path: copied.then_some(quarantine_path),
                copied,
                rename_error_code,
            }
        }
    }
}

fn open_checked_sqlite_for_salvage(
    db_path: &Path,
    busy_timeout_ms: Option<u64>,
) -> Result<Connection, SqliteError> {
    let db = Connection::open(db_path)?;
    if let Some(timeout) = busy_timeout_ms {
        db.busy_timeout(Duration::from_millis(timeout))?;
    }
    db.query_row("PRAGMA schema_version", [], |_| Ok(()))?;
    Ok(db)
}

pub fn open_sqlite_for_salvage(
    db_path: &Path,
    busy_timeout_ms: Option<u64>,
) -> Option<Connection> {
    if !db_path.exists() {
        return None;
    }
    match open_checked_sqlite_for_salvage(db_path, busy_timeout_ms) {
        Ok(db) => Some(db),
        Err(_) => {
            remove_sqlite_sidecars(db_path);
            match open_checked_sqlite_for_salvage(db_path, busy_timeout_ms) {
                Ok(db) => Some(db),
                Err(_) => Connection::open(db_path).ok(),
            }
        }
    }
}

pub fn copy_salvageable_sqlite_rows<F>(
    source: &Connection,
    select_sql: &str,
    mut insert_row: F,
) -> usize
where
    F: FnMut(&Row<'_>) -> Result<(), SqliteError>,
{
    let mut statement = match source.prepare(select_sql) {
        Ok(statement) => statement,
        Err(_) => return 0,
    };
    let mut rows = match statement.query([]) {
        Ok(rows) => rows,
        Err(_) => return 0,
    };
    let mut copied = 0;
    loop {
        match rows.next() {
            Ok(Some(row)) => {
                if insert_row(row).is_ok() {
                    copied += 1;
                }
            }
            Ok(None) | Err(_) => break,
        }
    }
    copied
}
