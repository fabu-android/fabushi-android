use std::thread;
use std::time::Duration;

use rusqlite::Error as SqliteError;

pub const SQLITE_BUSY: i32 = 5;
pub const SQLITE_LOCKED: i32 = 6;
pub const SQLITE_IOERR: i32 = 10;
pub const SQLITE_CORRUPT: i32 = 11;
pub const SQLITE_CANTOPEN: i32 = 14;
pub const SQLITE_NOTADB: i32 = 26;

fn primary_code(error: &SqliteError) -> Option<i32> {
    match error {
        SqliteError::SqliteFailure(inner, _) => Some(inner.extended_code & 255),
        _ => None,
    }
}

fn message(error: &SqliteError) -> String {
    error.to_string().to_ascii_lowercase()
}

pub fn is_sqlite_busy_error(error: &SqliteError) -> bool {
    matches!(
        primary_code(error),
        Some(SQLITE_BUSY) | Some(SQLITE_LOCKED)
    ) || {
        let message = message(error);
        message.contains("database is locked")
            || message.contains("database table is locked")
            || message.contains("database schema is locked")
    }
}

pub fn is_sqlite_io_error(error: &SqliteError) -> bool {
    primary_code(error) == Some(SQLITE_IOERR) || {
        let message = message(error);
        message.contains("disk i/o error") || message.contains("short read")
    }
}

pub fn is_sqlite_cant_open_error(error: &SqliteError) -> bool {
    primary_code(error) == Some(SQLITE_CANTOPEN)
        || message(error).contains("unable to open database")
}

pub fn is_sqlite_corrupt_error(error: &SqliteError) -> bool {
    matches!(
        primary_code(error),
        Some(SQLITE_CORRUPT) | Some(SQLITE_NOTADB)
    ) || {
        let message = message(error);
        message.contains("malformed")
            || message.contains("is not a database")
            || message.contains("database disk image")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetrySqliteBusyOptions {
    pub attempts: usize,
    pub base_delay_ms: u64,
}

impl Default for RetrySqliteBusyOptions {
    fn default() -> Self {
        Self {
            attempts: 5,
            base_delay_ms: 100,
        }
    }
}

pub fn retry_sqlite_busy<T, F>(
    operation: F,
    options: RetrySqliteBusyOptions,
) -> Result<T, SqliteError>
where
    F: FnMut() -> Result<T, SqliteError>,
{
    retry_sqlite_busy_with_delay(operation, options, |delay_ms| {
        thread::sleep(Duration::from_millis(delay_ms));
    })
}

pub fn retry_sqlite_busy_with_delay<T, F, D>(
    mut operation: F,
    options: RetrySqliteBusyOptions,
    mut delay: D,
) -> Result<T, SqliteError>
where
    F: FnMut() -> Result<T, SqliteError>,
    D: FnMut(u64),
{
    let attempts = options.attempts.max(1);
    let mut last = None;
    for attempt in 0..attempts {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) if is_sqlite_busy_error(&error) => {
                last = Some(error);
                if attempt + 1 < attempts {
                    let factor = 1_u64
                        .checked_shl(attempt.min(63) as u32)
                        .unwrap_or(u64::MAX);
                    delay(options.base_delay_ms.saturating_mul(factor));
                }
            }
            Err(error) => return Err(error),
        }
    }
    Err(last.expect("retry exhaustion follows at least one busy error"))
}
