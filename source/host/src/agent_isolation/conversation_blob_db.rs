use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags, params};

use crate::storage::sqlite_busy::{
    is_sqlite_cant_open_error, is_sqlite_corrupt_error, is_sqlite_io_error,
};
use crate::storage::sqlite_recovery::{
    copy_salvageable_sqlite_rows, open_sqlite_for_salvage, quarantine_corrupt_sqlite_db,
    remove_sqlite_db,
};
pub use crate::storage::sqlite_recovery::remove_sqlite_sidecars;

pub const CONVERSATION_BLOB_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS blobs (
  id TEXT PRIMARY KEY,
  data BLOB NOT NULL
) STRICT;
"#;

pub const CONVERSATION_BLOB_MIGRATION_UNSTARTED: i64 = 0;
pub const CONVERSATION_BLOB_ADOPTION_COMPLETE: i64 = 1;
pub const CONVERSATION_BLOB_RECOVERY_REBUILT: i64 = 2;

const RECOVERY_RETRY_DELAY_MS: u64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationBlobMigrationState {
    Unstarted,
    AdoptionComplete,
    RecoveryRebuilt,
    Unknown(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationBlobRecoveryOutcome {
    Recovered,
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationBlobRecoveryInfo {
    pub outcome: ConversationBlobRecoveryOutcome,
    pub quarantine_path: Option<PathBuf>,
    pub salvaged_blobs: usize,
}

pub struct ConversationBlobDbOptions<'a> {
    pub db_path: &'a Path,
    pub busy_timeout_ms: u64,
    pub agent_id: &'a str,
    pub log: Option<&'a dyn Fn(&str)>,
    pub on_recovery: Option<&'a dyn Fn(&ConversationBlobRecoveryInfo)>,
}

impl<'a> ConversationBlobDbOptions<'a> {
    pub fn new(db_path: &'a Path, busy_timeout_ms: u64) -> Self {
        Self {
            db_path,
            busy_timeout_ms,
            agent_id: "unknown",
            log: None,
            on_recovery: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConversationBlobDbError {
    #[error("conversation blob sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("conversation blob filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("conversation blob database failed quick_check: {0}")]
    Corrupt(PathBuf),
    #[error("{message}")]
    Recovery {
        code: &'static str,
        message: String,
    },
}

impl ConversationBlobDbError {
    pub fn recovery_code(&self) -> Option<&'static str> {
        match self {
            Self::Recovery { code, .. } => Some(code),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DbHealth {
    Healthy,
    Corrupt,
    Unavailable,
}

#[derive(Debug, Clone)]
struct PendingConversationBlobRecovery {
    pending_path: PathBuf,
    quarantine_path: PathBuf,
}

pub fn read_conversation_blob_migration_state(
    db: &Connection,
) -> Result<ConversationBlobMigrationState, ConversationBlobDbError> {
    let version = db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;
    Ok(match version {
        CONVERSATION_BLOB_MIGRATION_UNSTARTED => ConversationBlobMigrationState::Unstarted,
        CONVERSATION_BLOB_ADOPTION_COMPLETE => ConversationBlobMigrationState::AdoptionComplete,
        CONVERSATION_BLOB_RECOVERY_REBUILT => ConversationBlobMigrationState::RecoveryRebuilt,
        other => ConversationBlobMigrationState::Unknown(other),
    })
}

pub fn set_conversation_blob_migration_state(
    db: &Connection,
    state: ConversationBlobMigrationState,
) -> Result<(), ConversationBlobDbError> {
    let value = match state {
        ConversationBlobMigrationState::Unstarted => CONVERSATION_BLOB_MIGRATION_UNSTARTED,
        ConversationBlobMigrationState::AdoptionComplete => CONVERSATION_BLOB_ADOPTION_COMPLETE,
        ConversationBlobMigrationState::RecoveryRebuilt => CONVERSATION_BLOB_RECOVERY_REBUILT,
        ConversationBlobMigrationState::Unknown(value) => value,
    };
    db.pragma_update(None, "user_version", value)?;
    Ok(())
}

pub fn open_configured_conversation_blob_db(
    db_path: &Path,
    busy_timeout_ms: u64,
) -> Result<Connection, ConversationBlobDbError> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let db = Connection::open(db_path)?;
    db.busy_timeout(Duration::from_millis(busy_timeout_ms))?;
    let _ = db.pragma_update(None, "journal_mode", "WAL");
    let _ = db.pragma_update(None, "synchronous", "NORMAL");
    db.execute_batch(CONVERSATION_BLOB_SCHEMA)?;
    Ok(db)
}

fn run_quick_check_raw(db: &Connection) -> Result<bool, rusqlite::Error> {
    let result = db.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))?;
    Ok(result == "ok")
}

pub fn run_quick_check(db: &Connection) -> Result<bool, ConversationBlobDbError> {
    run_quick_check_raw(db).map_err(ConversationBlobDbError::from)
}

fn recovery_error(code: &'static str, message: impl Into<String>) -> ConversationBlobDbError {
    ConversationBlobDbError::Recovery {
        code,
        message: message.into(),
    }
}

fn recovery_attempts(busy_timeout_ms: u64) -> usize {
    busy_timeout_ms
        .div_ceil(RECOVERY_RETRY_DELAY_MS)
        .max(1)
        .min(usize::MAX as u64) as usize
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{}", path.display(), suffix))
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn remove_file_if_exists(path: &Path) -> Result<(), ConversationBlobDbError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn get_health(db: &Connection) -> DbHealth {
    match run_quick_check_raw(db) {
        Ok(true) => DbHealth::Healthy,
        Ok(false) => DbHealth::Corrupt,
        Err(error) if is_sqlite_corrupt_error(&error) => DbHealth::Corrupt,
        Err(_) => DbHealth::Unavailable,
    }
}

fn get_health_on_disk(
    db_path: &Path,
    busy_timeout_ms: u64,
) -> Result<DbHealth, ConversationBlobDbError> {
    for attempt in 0..2 {
        let result = (|| -> Result<DbHealth, rusqlite::Error> {
            let db = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            db.busy_timeout(Duration::from_millis(busy_timeout_ms))?;
            Ok(match run_quick_check_raw(&db) {
                Ok(true) => DbHealth::Healthy,
                Ok(false) => DbHealth::Corrupt,
                Err(error) if is_sqlite_corrupt_error(&error) => DbHealth::Corrupt,
                Err(_) => DbHealth::Unavailable,
            })
        })();
        match result {
            Ok(health) => return Ok(health),
            Err(error)
                if attempt == 0
                    && (is_sqlite_io_error(&error) || is_sqlite_cant_open_error(&error)) =>
            {
                remove_sqlite_sidecars(db_path);
            }
            Err(error) if is_sqlite_corrupt_error(&error) => return Ok(DbHealth::Corrupt),
            Err(_) => return Ok(DbHealth::Unavailable),
        }
    }
    Ok(DbHealth::Unavailable)
}

fn quarantine_path_for_marker(db_path: &Path, marker_name: &str) -> PathBuf {
    let db_name = db_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if marker_name == format!("{db_name}.pending") {
        return db_path.to_path_buf();
    }
    let suffix = if marker_name.ends_with(".intent") {
        ".intent"
    } else {
        ".pending"
    };
    let base = marker_name.strip_suffix(suffix).unwrap_or(marker_name);
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(base)
}

fn find_pending_recovery(
    db_path: &Path,
) -> Result<Option<PendingConversationBlobRecovery>, ConversationBlobDbError> {
    let dir = db_path.parent().unwrap_or_else(|| Path::new("."));
    let db_name = db_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    let prefix = format!("{db_name}.corrupt-");
    let in_place_marker = format!("{db_name}.pending");
    let mut names = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();

    let mut marker_names = names
        .iter()
        .filter(|name| {
            name.starts_with(&prefix)
                && (name.ends_with(".intent") || name.ends_with(".pending"))
        })
        .cloned()
        .collect::<Vec<_>>();
    marker_names.sort();
    if names.iter().any(|name| name == &in_place_marker) {
        marker_names.insert(0, in_place_marker.clone());
    }
    let selected = marker_names.last().cloned();
    let active_replacement = selected
        .as_deref()
        .map(|name| path_with_suffix(&quarantine_path_for_marker(db_path, name), ".replacement"));

    for name in names.drain(..) {
        let is_replacement = (name.starts_with(&prefix)
            || name == format!("{db_name}.replacement"))
            && name.ends_with(".replacement");
        if !is_replacement {
            continue;
        }
        let replacement = dir.join(&name);
        if active_replacement.as_ref() != Some(&replacement) {
            remove_sqlite_db(&replacement, 1, 0)?;
        }
    }

    let Some(selected) = selected else {
        return Ok(None);
    };
    for stale in marker_names.iter().take(marker_names.len().saturating_sub(1)) {
        let marker = dir.join(stale);
        remove_file_if_exists(&marker)?;
        remove_sqlite_db(
            &path_with_suffix(&quarantine_path_for_marker(db_path, stale), ".replacement"),
            1,
            0,
        )?;
    }

    Ok(Some(PendingConversationBlobRecovery {
        pending_path: dir.join(&selected),
        quarantine_path: quarantine_path_for_marker(db_path, &selected),
    }))
}

fn open_conversation_blob_recovery_target(
    options: &ConversationBlobDbOptions<'_>,
    target_path: &Path,
) -> Result<Connection, ConversationBlobDbError> {
    let attempts = recovery_attempts(options.busy_timeout_ms);
    let mut retried_without_sidecars = false;
    for _ in 0..3 {
        let db = match open_configured_conversation_blob_db(target_path, options.busy_timeout_ms) {
            Ok(db) => db,
            Err(ConversationBlobDbError::Sqlite(error))
                if (is_sqlite_io_error(&error) || is_sqlite_cant_open_error(&error))
                    && !retried_without_sidecars =>
            {
                retried_without_sidecars = true;
                remove_sqlite_sidecars(target_path);
                continue;
            }
            Err(ConversationBlobDbError::Sqlite(error))
                if is_sqlite_io_error(&error)
                    || is_sqlite_corrupt_error(&error)
                    || is_sqlite_cant_open_error(&error) =>
            {
                remove_sqlite_db(target_path, attempts, RECOVERY_RETRY_DELAY_MS)?;
                continue;
            }
            Err(error) => return Err(error),
        };

        match get_health(&db) {
            DbHealth::Healthy => return Ok(db),
            DbHealth::Unavailable => {
                drop(db);
                return Err(recovery_error(
                    "SAND_BLOB_RECOVERY_TARGET_UNHEALTHY",
                    "conversation blob recovery target is temporarily unavailable",
                ));
            }
            DbHealth::Corrupt => {
                drop(db);
                remove_sqlite_db(target_path, attempts, RECOVERY_RETRY_DELAY_MS)?;
            }
        }
    }
    Err(recovery_error(
        "SAND_BLOB_RECOVERY_TARGET_UNHEALTHY",
        "failed to create a healthy conversation blob database",
    ))
}

fn install_completed_replacement(
    options: &ConversationBlobDbOptions<'_>,
    replacement_path: &Path,
    pending_path: Option<&Path>,
) -> Result<Option<Connection>, ConversationBlobDbError> {
    match get_health_on_disk(replacement_path, options.busy_timeout_ms)? {
        DbHealth::Unavailable => {
            return Err(recovery_error(
                "SAND_BLOB_RECOVERY_SOURCE_UNAVAILABLE",
                "completed conversation blob replacement is temporarily unavailable",
            ));
        }
        DbHealth::Corrupt => return Ok(None),
        DbHealth::Healthy => {}
    }

    let replacement =
        open_configured_conversation_blob_db(replacement_path, options.busy_timeout_ms)?;
    match get_health(&replacement) {
        DbHealth::Corrupt => return Ok(None),
        DbHealth::Unavailable => {
            return Err(recovery_error(
                "SAND_BLOB_RECOVERY_SOURCE_UNAVAILABLE",
                "completed conversation blob replacement became unavailable",
            ));
        }
        DbHealth::Healthy => {}
    }
    let replacement_state = read_conversation_blob_migration_state(&replacement)?;
    if !matches!(
        replacement_state,
        ConversationBlobMigrationState::RecoveryRebuilt
            | ConversationBlobMigrationState::AdoptionComplete
    ) {
        return Ok(None);
    }
    replacement.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode = DELETE;")?;
    drop(replacement);

    remove_sqlite_sidecars(replacement_path);
    remove_sqlite_db(
        options.db_path,
        recovery_attempts(options.busy_timeout_ms),
        RECOVERY_RETRY_DELAY_MS,
    )?;
    fs::rename(replacement_path, options.db_path)?;

    let installed =
        open_configured_conversation_blob_db(options.db_path, options.busy_timeout_ms)?;
    if get_health(&installed) != DbHealth::Healthy {
        drop(installed);
        return Err(recovery_error(
            "SAND_BLOB_RECOVERY_INSTALL_UNHEALTHY",
            "installed completed conversation blob replacement is unhealthy",
        ));
    }
    if let Some(pending_path) = pending_path {
        remove_file_if_exists(pending_path)?;
    }
    Ok(Some(installed))
}

fn finish_conversation_blob_recovery(
    options: &ConversationBlobDbOptions<'_>,
    mut quarantine_path: Option<PathBuf>,
    mut pending_path: Option<PathBuf>,
) -> Result<Connection, ConversationBlobDbError> {
    let attempts = recovery_attempts(options.busy_timeout_ms);
    let mut replacement_path =
        path_with_suffix(quarantine_path.as_deref().unwrap_or(options.db_path), ".replacement");

    if !options.db_path.exists() && replacement_path.exists() {
        if let Some(installed) = install_completed_replacement(
            options,
            &replacement_path,
            pending_path.as_deref(),
        )? {
            return Ok(installed);
        }
        let has_separate_quarantine = quarantine_path
            .as_ref()
            .is_some_and(|path| path != options.db_path && path.exists());
        if !has_separate_quarantine {
            quarantine_path = Some(replacement_path.clone());
            replacement_path = path_with_suffix(&replacement_path, ".replacement");
        }
    }

    if options.db_path.exists() {
        let source_health = get_health_on_disk(options.db_path, options.busy_timeout_ms)?;
        if source_health == DbHealth::Unavailable {
            return Err(recovery_error(
                "SAND_BLOB_RECOVERY_SOURCE_UNAVAILABLE",
                "conversation blob recovery source is temporarily unavailable",
            ));
        }

        if source_health == DbHealth::Healthy {
            match open_configured_conversation_blob_db(options.db_path, options.busy_timeout_ms) {
                Ok(existing) => match get_health(&existing) {
                    DbHealth::Healthy => {
                        if let Some(quarantine_path) = quarantine_path.as_deref() {
                            remove_sqlite_db(
                                &path_with_suffix(quarantine_path, ".replacement"),
                                1,
                                0,
                            )?;
                        }
                        if let Some(pending_path) = pending_path.as_deref() {
                            remove_file_if_exists(pending_path)?;
                        }
                        return Ok(existing);
                    }
                    DbHealth::Unavailable => {
                        return Err(recovery_error(
                            "SAND_BLOB_RECOVERY_SOURCE_UNAVAILABLE",
                            "conversation blob recovery source became temporarily unavailable",
                        ));
                    }
                    DbHealth::Corrupt => {}
                },
                Err(ConversationBlobDbError::Sqlite(error))
                    if is_sqlite_corrupt_error(&error) => {}
                Err(_) => {
                    return Err(recovery_error(
                        "SAND_BLOB_RECOVERY_SOURCE_UNAVAILABLE",
                        "healthy conversation blob recovery source could not be configured",
                    ));
                }
            }
        }

        if quarantine_path.as_deref() == Some(options.db_path) {
            // The source itself is the salvage path when it could not be moved.
        } else if quarantine_path
            .as_ref()
            .is_some_and(|path| !path.exists())
        {
            let requested = quarantine_path.clone().expect("checked as some");
            let quarantine = quarantine_corrupt_sqlite_db(
                options.db_path,
                Some(&requested),
                true,
                attempts,
                RECOVERY_RETRY_DELAY_MS,
            );
            quarantine_path = quarantine.quarantine_path;
            let resumed_pending_path = quarantine_path
                .as_deref()
                .map(|path| path_with_suffix(path, ".pending"));
            if let (Some(current), Some(resumed)) =
                (pending_path.as_deref(), resumed_pending_path.as_deref())
            {
                if current != resumed {
                    fs::rename(current, resumed)?;
                    pending_path = Some(resumed.to_path_buf());
                }
            }
            replacement_path =
                path_with_suffix(quarantine_path.as_deref().unwrap_or(options.db_path), ".replacement");
        } else {
            remove_sqlite_db(options.db_path, attempts, RECOVERY_RETRY_DELAY_MS)?;
        }
    }

    remove_sqlite_db(&replacement_path, 1, 0)?;
    let fresh = open_conversation_blob_recovery_target(options, &replacement_path)?;
    let mut salvaged_blobs = 0usize;
    if let Some(quarantine_path) = quarantine_path.as_deref() {
        let source = open_sqlite_for_salvage(quarantine_path, Some(options.busy_timeout_ms));
        if source.is_none() && quarantine_path.exists() {
            drop(fresh);
            return Err(recovery_error(
                "SAND_BLOB_RECOVERY_QUARANTINE_UNREADABLE",
                "quarantined conversation blob database is unreadable",
            ));
        }
        if let Some(source) = source {
            salvaged_blobs = copy_salvageable_sqlite_rows(
                &source,
                "SELECT id, data FROM blobs",
                |row| {
                    let id: String = row.get(0)?;
                    let data: Vec<u8> = row.get(1)?;
                    fresh
                        .execute(
                            "INSERT OR IGNORE INTO blobs (id, data) VALUES (?1, ?2)",
                            params![id, data],
                        )
                        .map(|_| ())
                },
            );
        }
    }
    set_conversation_blob_migration_state(
        &fresh,
        ConversationBlobMigrationState::RecoveryRebuilt,
    )?;
    fresh.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode = DELETE;")?;
    drop(fresh);

    remove_sqlite_sidecars(&replacement_path);
    remove_sqlite_db(options.db_path, attempts, RECOVERY_RETRY_DELAY_MS)?;
    fs::rename(&replacement_path, options.db_path)?;

    let installed =
        open_configured_conversation_blob_db(options.db_path, options.busy_timeout_ms)?;
    if get_health(&installed) != DbHealth::Healthy {
        drop(installed);
        return Err(recovery_error(
            "SAND_BLOB_RECOVERY_INSTALL_UNHEALTHY",
            "installed conversation blob recovery is unhealthy",
        ));
    }

    let info = ConversationBlobRecoveryInfo {
        outcome: if salvaged_blobs > 0 {
            ConversationBlobRecoveryOutcome::Recovered
        } else {
            ConversationBlobRecoveryOutcome::Reset
        },
        quarantine_path: quarantine_path.clone(),
        salvaged_blobs,
    };
    if let Some(log) = options.log {
        let quarantine_name = info
            .quarantine_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("none");
        log(&format!(
            "[agent-store-worker] conversation-blobs.db recovery: outcome={} agent={} quarantine={} salvaged.blobs={}",
            match info.outcome {
                ConversationBlobRecoveryOutcome::Recovered => "recovered",
                ConversationBlobRecoveryOutcome::Reset => "reset",
            },
            options.agent_id,
            quarantine_name,
            info.salvaged_blobs
        ));
    }
    if let Some(on_recovery) = options.on_recovery {
        let _ = catch_unwind(AssertUnwindSafe(|| on_recovery(&info)));
    }
    if let Some(pending_path) = pending_path.as_deref() {
        remove_file_if_exists(pending_path)?;
    }
    Ok(installed)
}

fn recover_conversation_blob_db(
    options: &ConversationBlobDbOptions<'_>,
) -> Result<Connection, ConversationBlobDbError> {
    let intended_quarantine_path =
        path_with_suffix(options.db_path, &format!(".corrupt-{}", now_ms()));
    let intent_path = path_with_suffix(&intended_quarantine_path, ".intent");
    fs::write(&intent_path, b"")?;

    let quarantine = quarantine_corrupt_sqlite_db(
        options.db_path,
        Some(&intended_quarantine_path),
        true,
        recovery_attempts(options.busy_timeout_ms),
        RECOVERY_RETRY_DELAY_MS,
    );
    let pending_path = path_with_suffix(
        quarantine
            .quarantine_path
            .as_deref()
            .unwrap_or(&intended_quarantine_path),
        ".pending",
    );
    fs::rename(&intent_path, &pending_path)?;

    if let Some(code) = quarantine.rename_error_code.as_deref() {
        if let Some(log) = options.log {
            let message = if quarantine.quarantine_path.is_none() {
                format!(
                    "[agent-store-worker] could not quarantine conversation-blobs.db for agent={} (rename {code}); resetting without a preserved copy",
                    options.agent_id
                )
            } else if quarantine.copied {
                format!(
                    "[agent-store-worker] quarantined conversation-blobs.db by copy for agent={} (rename {code})",
                    options.agent_id
                )
            } else {
                format!(
                    "[agent-store-worker] could not copy quarantine for conversation-blobs.db for agent={} (rename {code}); recovering from the source in place",
                    options.agent_id
                )
            };
            log(&message);
        }
    }

    finish_conversation_blob_recovery(
        options,
        quarantine.quarantine_path,
        Some(pending_path),
    )
}

pub fn open_conversation_blob_db_with_options(
    options: &ConversationBlobDbOptions<'_>,
) -> Result<Connection, ConversationBlobDbError> {
    if let Some(parent) = options.db_path.parent() {
        fs::create_dir_all(parent)?;
    }

    if let Some(pending) = find_pending_recovery(options.db_path)? {
        return finish_conversation_blob_recovery(
            options,
            Some(pending.quarantine_path),
            Some(pending.pending_path),
        );
    }

    let mut db = match open_configured_conversation_blob_db(
        options.db_path,
        options.busy_timeout_ms,
    ) {
        Ok(db) => db,
        Err(ConversationBlobDbError::Sqlite(error))
            if is_sqlite_io_error(&error) || is_sqlite_cant_open_error(&error) =>
        {
            remove_sqlite_sidecars(options.db_path);
            match open_configured_conversation_blob_db(
                options.db_path,
                options.busy_timeout_ms,
            ) {
                Ok(db) => db,
                Err(ConversationBlobDbError::Sqlite(error))
                    if is_sqlite_corrupt_error(&error) =>
                {
                    return recover_conversation_blob_db(options);
                }
                Err(error) => return Err(error),
            }
        }
        Err(ConversationBlobDbError::Sqlite(error)) if is_sqlite_corrupt_error(&error) => {
            return recover_conversation_blob_db(options);
        }
        Err(error) => return Err(error),
    };

    match run_quick_check_raw(&db) {
        Ok(true) => Ok(db),
        Ok(false) => {
            drop(db);
            recover_conversation_blob_db(options)
        }
        Err(error) if is_sqlite_io_error(&error) || is_sqlite_cant_open_error(&error) => {
            drop(db);
            remove_sqlite_sidecars(options.db_path);
            db = match open_configured_conversation_blob_db(
                options.db_path,
                options.busy_timeout_ms,
            ) {
                Ok(db) => db,
                Err(ConversationBlobDbError::Sqlite(error))
                    if is_sqlite_io_error(&error)
                        || is_sqlite_cant_open_error(&error)
                        || is_sqlite_corrupt_error(&error) =>
                {
                    return recover_conversation_blob_db(options);
                }
                Err(error) => return Err(error),
            };
            match run_quick_check_raw(&db) {
                Ok(true) => Ok(db),
                Ok(false) => {
                    drop(db);
                    recover_conversation_blob_db(options)
                }
                Err(error)
                    if is_sqlite_io_error(&error)
                        || is_sqlite_cant_open_error(&error)
                        || is_sqlite_corrupt_error(&error) =>
                {
                    drop(db);
                    recover_conversation_blob_db(options)
                }
                Err(error) => Err(error.into()),
            }
        }
        Err(error) if is_sqlite_corrupt_error(&error) => {
            drop(db);
            recover_conversation_blob_db(options)
        }
        Err(_) => {
            drop(db);
            Err(recovery_error(
                "SAND_BLOB_RECOVERY_SOURCE_UNAVAILABLE",
                "conversation blob database health check is temporarily unavailable",
            ))
        }
    }
}

pub fn open_conversation_blob_db(
    db_path: &Path,
    busy_timeout_ms: u64,
) -> Result<Connection, ConversationBlobDbError> {
    open_conversation_blob_db_with_options(&ConversationBlobDbOptions::new(
        db_path,
        busy_timeout_ms,
    ))
}
