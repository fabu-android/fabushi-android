use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use rusqlite::Connection;

pub const DB_BUSY_TIMEOUT_MS: u64 = 5_000;
pub const SQLITE_DB_SIDECAR_SUFFIXES: [&str; 3] = ["-wal", "-shm", "-journal"];

fn write_generations() -> &'static Mutex<HashMap<PathBuf, u64>> {
    static VALUES: OnceLock<Mutex<HashMap<PathBuf, u64>>> = OnceLock::new();
    VALUES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn live_handles() -> &'static Mutex<HashMap<PathBuf, usize>> {
    static VALUES: OnceLock<Mutex<HashMap<PathBuf, usize>>> = OnceLock::new();
    VALUES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn resolve_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

pub fn get_sand_agent_db_write_generation(db_path: &Path) -> u64 {
    let path = resolve_path(db_path);
    write_generations()
        .lock()
        .ok()
        .and_then(|values| values.get(&path).copied())
        .unwrap_or_default()
}

pub fn bump_db_write_generation(db_path: &Path) {
    let path = resolve_path(db_path);
    if let Ok(mut values) = write_generations().lock() {
        let next = values
            .get(&path)
            .copied()
            .unwrap_or_default()
            .saturating_add(1);
        values.insert(path, next);
    }
}

pub fn delete_sand_agent_db_write_generation(db_path: &Path) {
    let path = resolve_path(db_path);
    if let Ok(mut values) = write_generations().lock() {
        values.remove(&path);
    }
}

pub fn live_db_handle_count(db_path: &Path) -> usize {
    let path = resolve_path(db_path);
    live_handles()
        .lock()
        .ok()
        .and_then(|values| values.get(&path).copied())
        .unwrap_or_default()
}

pub fn register_live_db_handle(db_path: &Path) {
    let path = resolve_path(db_path);
    if let Ok(mut values) = live_handles().lock() {
        let next = values
            .get(&path)
            .copied()
            .unwrap_or_default()
            .saturating_add(1);
        values.insert(path, next);
    }
}

pub fn release_live_db_handle(db_path: &Path) {
    let path = resolve_path(db_path);
    if let Ok(mut values) = live_handles().lock() {
        let next = values
            .get(&path)
            .copied()
            .unwrap_or_default()
            .saturating_sub(1);
        if next == 0 {
            values.remove(&path);
        } else {
            values.insert(path, next);
        }
    }
}

pub fn has_live_sand_agent_db_handle(db_path: &Path) -> bool {
    live_db_handle_count(db_path) > 0
}

pub fn wal_frames_fully_folded(log: i64, checkpointed: i64) -> Option<bool> {
    if log < 0 || checkpointed < 0 {
        None
    } else {
        Some(checkpointed >= log)
    }
}

pub fn checkpoint_sand_agent_db(db_path: &Path, busy_timeout_ms: u64) -> bool {
    if !db_path.exists() {
        return true;
    }
    let db = match Connection::open(db_path) {
        Ok(db) => db,
        Err(_) => return false,
    };
    if db
        .busy_timeout(Duration::from_millis(busy_timeout_ms))
        .is_err()
    {
        return false;
    }
    let result = db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
        Ok((row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
    });
    drop(db);
    let (log, checkpointed) = match result {
        Ok(value) => value,
        Err(_) => return false,
    };
    if let Some(folded) = wal_frames_fully_folded(log, checkpointed) {
        return folded;
    }
    let wal_path = PathBuf::from(format!("{}-wal", db_path.display()));
    match fs::metadata(wal_path) {
        Ok(metadata) => metadata.len() == 0,
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_handle_registry_is_reference_counted_per_resolved_path() {
        let path = PathBuf::from(format!("agent-store-live-{}", std::process::id()));
        assert_eq!(live_db_handle_count(&path), 0);
        register_live_db_handle(&path);
        register_live_db_handle(&path);
        assert_eq!(live_db_handle_count(&path), 2);
        release_live_db_handle(&path);
        assert_eq!(live_db_handle_count(&path), 1);
        release_live_db_handle(&path);
        assert_eq!(live_db_handle_count(&path), 0);
    }

    #[test]
    fn write_generation_is_monotonic_and_explicitly_clearable() {
        let path = PathBuf::from(format!("agent-store-generation-{}", std::process::id()));
        assert_eq!(get_sand_agent_db_write_generation(&path), 0);
        bump_db_write_generation(&path);
        bump_db_write_generation(&path);
        assert_eq!(get_sand_agent_db_write_generation(&path), 2);
        delete_sand_agent_db_write_generation(&path);
        assert_eq!(get_sand_agent_db_write_generation(&path), 0);
    }

    #[test]
    fn wal_checkpoint_fold_contract_rejects_invalid_counts() {
        assert_eq!(wal_frames_fully_folded(5, 5), Some(true));
        assert_eq!(wal_frames_fully_folded(5, 4), Some(false));
        assert_eq!(wal_frames_fully_folded(-1, 0), None);
    }
}
