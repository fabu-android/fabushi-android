use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::agent_isolation::{AgentWorkerPool, ProductionAgentStoreWorkerBackend};
use crate::storage::store_db::{
    get_sand_agent_db_write_generation, live_db_handle_count,
};

use super::agent_db::{
    append_persisted_transcript_entries, compare_and_set_persisted_latest_root_blob_id,
    delete_persisted_transcript_entry, has_persisted_legacy_conversation_blobs,
    hidden_entry_repair_version, legacy_blob_retirement_version,
    read_persisted_agent_name, read_persisted_latest_root_blob_id,
    read_persisted_transcript_entries, retire_persisted_legacy_conversation_blobs,
    set_hidden_entry_repair_version, set_persisted_agent_name,
    set_stale_root_cleanup_version, stale_root_cleanup_version,
};
use super::conversation_recovery::{
    OutlineItem, conversation_structure_fully_resolves, parse_conversation_state_structure,
    rebuild_transcript_entries_from_state, select_hidden_artifact_entry_ids,
};
use super::session_diagnostics::{SessionDiagnostic, report_session_diagnostic};
use super::session_recovery::{
    ConversationRecoveryScanError, transcript_entry_matches_recovered,
};

pub const LEGACY_BLOB_RETIREMENT_VERSION: u64 = 1;
pub const STALE_ROOT_CLEANUP_VERSION: u64 = 1;
pub const HIDDEN_ENTRY_REPAIR_VERSION: u64 = 1;

static LEGACY_RETIREMENT: AtomicBool = AtomicBool::new(false);
static STALE_ROOT_GC: AtomicBool = AtomicBool::new(false);

pub fn pin_legacy_store_blob_retirement(enabled: bool) {
    LEGACY_RETIREMENT.store(enabled, Ordering::Release);
}

pub fn is_legacy_store_blob_retirement_enabled() -> bool {
    // Android production configuration is injected through the typed Host runtime. Do not let
    // shell/environment state become a second authority for destructive maintenance.
    LEGACY_RETIREMENT.load(Ordering::Acquire)
}

pub fn pin_stale_root_gc(enabled: bool) {
    STALE_ROOT_GC.store(enabled, Ordering::Release);
}

pub type SessionMaintenanceTask = Box<dyn FnOnce() -> Result<(), String>>;

fn maintenance_agent_id(db_path: &Path) -> String {
    db_path
        .parent()
        .and_then(Path::file_name)
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn report_maintenance_diagnostic(
    kind: &str,
    db_path: &Path,
    error_class: Option<&str>,
) {
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "agentId".into(),
        serde_json::Value::String(maintenance_agent_id(db_path)),
    );
    if let Some(error_class) = error_class {
        metadata.insert(
            "errorClass".into(),
            serde_json::Value::String(error_class.to_string()),
        );
    }
    report_session_diagnostic(&SessionDiagnostic {
        family: "maintenance".into(),
        kind: kind.to_string(),
        metadata,
    });
}


pub fn run_session_maintenance<F>(
    tasks: Vec<SessionMaintenanceTask>,
    mut report: F,
)
where
    F: FnMut(&str, usize),
{
    for (index, task) in tasks.into_iter().enumerate() {
        if let Err(error) = task() {
            report(&error, index);
        }
    }
}

pub fn sync_recovered_profile_name(
    db_path: &Path,
    busy_timeout_ms: u64,
    profile_name: &str,
) -> Result<bool, String> {
    let profile_name = profile_name.trim();
    if profile_name.is_empty()
        || read_persisted_agent_name(db_path, busy_timeout_ms)
            .map_err(|error| error.to_string())?
            .as_deref()
            == Some(profile_name)
    {
        return Ok(false);
    }
    set_persisted_agent_name(db_path, busy_timeout_ms, profile_name)
        .map_err(|error| error.to_string())
}

pub fn backfill_transcript(
    db_path: &Path,
    busy_timeout_ms: u64,
    rebuilt: &[serde_json::Value],
) -> Result<usize, String> {
    let persisted = read_persisted_transcript_entries(db_path, busy_timeout_ms)
        .map_err(|error| error.to_string())?;
    if persisted.len() >= rebuilt.len()
        || !persisted.iter().enumerate().all(|(index, entry)| {
            rebuilt
                .get(index)
                .is_some_and(|candidate| transcript_entry_matches_recovered(entry, candidate))
        })
    {
        return Ok(0);
    }
    append_persisted_transcript_entries(
        db_path,
        busy_timeout_ms,
        &rebuilt[persisted.len()..],
    )
    .map_err(|error| error.to_string())
}

pub fn backfill_transcript_from_outline(
    db_path: &Path,
    busy_timeout_ms: u64,
    turns: &[Vec<OutlineItem>],
) -> Result<usize, String> {
    let rebuilt = rebuild_transcript_entries_from_state(turns);
    backfill_transcript(db_path, busy_timeout_ms, &rebuilt)
}

pub fn repair_hidden_transcript_entries_once(
    db_path: &Path,
    busy_timeout_ms: u64,
    outline: &[OutlineItem],
) -> Result<usize, String> {
    if hidden_entry_repair_version(db_path, busy_timeout_ms)
        .map_err(|error| error.to_string())?
        >= HIDDEN_ENTRY_REPAIR_VERSION
    {
        return Ok(0);
    }

    let outcome = (|| -> Result<usize, String> {
        let entries = read_persisted_transcript_entries(db_path, busy_timeout_ms)
            .map_err(|error| error.to_string())?;
        let has_recovered_user = entries.iter().any(|entry| {
            entry.get("kind").and_then(serde_json::Value::as_str) == Some("message")
                && entry.get("role").and_then(serde_json::Value::as_str) == Some("user")
                && entry
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|id| id.starts_with("recovered-"))
        });
        if !has_recovered_user {
            let _ = set_hidden_entry_repair_version(
                db_path,
                busy_timeout_ms,
                HIDDEN_ENTRY_REPAIR_VERSION,
            )
            .map_err(|error| error.to_string())?;
            return Ok(0);
        }
        if outline.is_empty() {
            return Ok(0);
        }
        let ids = select_hidden_artifact_entry_ids(&entries, outline);
        for id in &ids {
            if !delete_persisted_transcript_entry(db_path, busy_timeout_ms, id)
                .map_err(|error| error.to_string())?
            {
                return Ok(0);
            }
        }
        let _ = set_hidden_entry_repair_version(
            db_path,
            busy_timeout_ms,
            HIDDEN_ENTRY_REPAIR_VERSION,
        )
        .map_err(|error| error.to_string())?;
        Ok(ids.len())
    })();

    match outcome {
        Ok(value) => Ok(value),
        Err(_error) => {
            report_maintenance_diagnostic("hidden_repair_failed", db_path, Some("Error"));
            Ok(0)
        }
    }
}
pub fn find_latest_durable_root_blob_id(
    pool: Arc<AgentWorkerPool<ProductionAgentStoreWorkerBackend>>,
    agent_id: &str,
    blob_db_path: &Path,
    legacy_blob_db_path: &Path,
) -> Result<Option<Vec<u8>>, ConversationRecoveryScanError> {
    futures::executor::block_on(pool.find_latest_root_blob_id(
        agent_id,
        blob_db_path,
        Some(legacy_blob_db_path),
    ))
    .map_err(|error| ConversationRecoveryScanError {
        detail: error.to_string(),
    })
}

pub fn recover_conversation_root_if_missing(
    pool: Arc<AgentWorkerPool<ProductionAgentStoreWorkerBackend>>,
    agent_id: &str,
    db_path: &Path,
    blob_db_path: &Path,
    busy_timeout_ms: u64,
) -> Result<bool, String> {
    let existing = read_persisted_latest_root_blob_id(db_path, busy_timeout_ms)
        .map_err(|error| error.to_string())?;
    if !existing.is_empty() {
        return Ok(false);
    }

    let outcome = (|| -> Result<bool, String> {
        if !blob_db_path.exists()
            && !has_persisted_legacy_conversation_blobs(db_path, busy_timeout_ms)
                .map_err(|error| error.to_string())?
        {
            return Ok(false);
        }

        let recovery_generation = get_sand_agent_db_write_generation(db_path);
        let root = match find_latest_durable_root_blob_id(
            Arc::clone(&pool),
            agent_id,
            blob_db_path,
            db_path,
        ) {
            Ok(Some(root)) => root,
            Ok(None) => return Ok(false),
            Err(error) => {
                report_maintenance_diagnostic(
                    "recovery_scan_failed",
                    db_path,
                    Some("ConversationRecoveryScanError"),
                );
                return Err(error.to_string());
            }
        };
        let Some(root_blob) = futures::executor::block_on(pool.get_blob(
            agent_id,
            blob_db_path,
            &root,
            Some(db_path),
        ))
        .map_err(|error| error.to_string())?
        else {
            return Ok(false);
        };
        let Some(structure) = parse_conversation_state_structure(&root_blob) else {
            return Ok(false);
        };
        let refs = structure.refs();
        let resolves = futures::executor::block_on(conversation_structure_fully_resolves(
            &refs,
            |blob_id| {
                let pool = Arc::clone(&pool);
                let agent_id = agent_id.to_string();
                let blob_db_path = blob_db_path.to_path_buf();
                let db_path = db_path.to_path_buf();
                async move {
                    pool.get_blob(
                        &agent_id,
                        &blob_db_path,
                        &blob_id,
                        Some(&db_path),
                    )
                    .await
                    .map_err(|error| error.to_string())
                }
            },
        ));
        if !resolves || get_sand_agent_db_write_generation(db_path) != recovery_generation {
            return Ok(false);
        }
        compare_and_set_persisted_latest_root_blob_id(
            db_path,
            busy_timeout_ms,
            &existing,
            &root,
        )
        .map_err(|error| error.to_string())
    })();

    match outcome {
        Ok(value) => Ok(value),
        Err(error) if error.starts_with("conversation recovery scan failed:") => Err(error),
        Err(_error) => {
            report_maintenance_diagnostic("recovery_failed", db_path, Some("Error"));
            Ok(false)
        }
    }
}
pub fn clear_stale_checkpoint_roots_once(
    pool: Arc<AgentWorkerPool<ProductionAgentStoreWorkerBackend>>,
    agent_id: &str,
    db_path: &Path,
    blob_db_path: &Path,
    busy_timeout_ms: u64,
) -> Result<bool, String> {
    if !STALE_ROOT_GC.load(Ordering::Acquire)
        || stale_root_cleanup_version(db_path, busy_timeout_ms)
            .map_err(|error| error.to_string())?
            >= STALE_ROOT_CLEANUP_VERSION
    {
        return Ok(false);
    }

    let outcome = (|| -> Result<bool, String> {
        let root = read_persisted_latest_root_blob_id(db_path, busy_timeout_ms)
            .map_err(|error| error.to_string())?;
        if root.is_empty() || live_db_handle_count(db_path) > 1 {
            return Ok(false);
        }
        let Some(root_blob) = futures::executor::block_on(pool.get_blob(
            agent_id,
            blob_db_path,
            &root,
            Some(db_path),
        ))
        .map_err(|error| error.to_string())?
        else {
            return Ok(false);
        };
        let Some(structure) = parse_conversation_state_structure(&root_blob) else {
            return Ok(false);
        };
        if structure.turns.is_empty() {
            return Ok(false);
        }

        let before = get_sand_agent_db_write_generation(db_path);
        futures::executor::block_on(pool.clear_stale_checkpoint_roots(
            agent_id,
            blob_db_path,
            &hex(&root),
            Some(db_path),
        ))
        .map_err(|error| error.to_string())?;
        if get_sand_agent_db_write_generation(db_path) != before
            || read_persisted_latest_root_blob_id(db_path, busy_timeout_ms)
                .map_err(|error| error.to_string())?
                != root
            || live_db_handle_count(db_path) > 1
        {
            return Ok(false);
        }
        set_stale_root_cleanup_version(
            db_path,
            busy_timeout_ms,
            STALE_ROOT_CLEANUP_VERSION,
        )
        .map_err(|error| error.to_string())
    })();

    match outcome {
        Ok(value) => Ok(value),
        Err(_error) => {
            report_maintenance_diagnostic(
                "checkpoint_cleanup_failed",
                db_path,
                Some("Error"),
            );
            Ok(false)
        }
    }
}
pub fn retire_legacy_store_blobs_once(
    pool: Arc<AgentWorkerPool<ProductionAgentStoreWorkerBackend>>,
    agent_id: &str,
    db_path: &Path,
    blob_db_path: &Path,
    busy_timeout_ms: u64,
) -> Result<bool, String> {
    if !is_legacy_store_blob_retirement_enabled()
        || legacy_blob_retirement_version(db_path, busy_timeout_ms)
            .map_err(|error| error.to_string())?
            >= LEGACY_BLOB_RETIREMENT_VERSION
    {
        return Ok(false);
    }

    let outcome = (|| -> Result<bool, String> {
        if !has_persisted_legacy_conversation_blobs(db_path, busy_timeout_ms)
            .map_err(|error| error.to_string())?
        {
            return retire_persisted_legacy_conversation_blobs(
                db_path,
                busy_timeout_ms,
                LEGACY_BLOB_RETIREMENT_VERSION,
            )
            .map_err(|error| error.to_string());
        }

        let root = read_persisted_latest_root_blob_id(db_path, busy_timeout_ms)
            .map_err(|error| error.to_string())?;
        if root.is_empty() || live_db_handle_count(db_path) > 1 {
            return Ok(false);
        }
        let Some(root_blob) = futures::executor::block_on(pool.get_blob(
            agent_id,
            blob_db_path,
            &root,
            Some(db_path),
        ))
        .map_err(|error| error.to_string())?
        else {
            return Ok(false);
        };
        let Some(structure) = parse_conversation_state_structure(&root_blob) else {
            return Ok(false);
        };
        if structure.turns.is_empty() {
            return Ok(false);
        }

        let before = get_sand_agent_db_write_generation(db_path);
        let verdict = futures::executor::block_on(pool.verify_legacy_blob_retirement(
            agent_id,
            blob_db_path,
            &hex(&root),
            db_path,
        ))
        .map_err(|error| error.to_string())?;
        if !verdict.is_retirable
            || get_sand_agent_db_write_generation(db_path) != before
            || read_persisted_latest_root_blob_id(db_path, busy_timeout_ms)
                .map_err(|error| error.to_string())?
                != root
            || live_db_handle_count(db_path) > 1
        {
            return Ok(false);
        }
        retire_persisted_legacy_conversation_blobs(
            db_path,
            busy_timeout_ms,
            LEGACY_BLOB_RETIREMENT_VERSION,
        )
        .map_err(|error| error.to_string())
    })();

    match outcome {
        Ok(value) => Ok(value),
        Err(_error) => {
            report_maintenance_diagnostic("blob_retirement_failed", db_path, Some("Error"));
            Ok(false)
        }
    }
}
pub fn cleanup_legacy_group_member_dirs(
    root_dir: &Path,
    dirname_to_remove: &str,
) -> Vec<(String, String)> {
    let mut failures = Vec::new();
    let Ok(entries) = fs::read_dir(root_dir) else {
        return failures;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let target = entry.path().join(dirname_to_remove);
        if let Err(error) = fs::remove_dir_all(&target) {
            if error.kind() != std::io::ErrorKind::NotFound {
                let agent_id = entry.file_name().to_string_lossy().into_owned();
                let db_path = root_dir.join(&agent_id).join("store.db");
                report_maintenance_diagnostic(
                    "member_cleanup_failed",
                    &db_path,
                    Some("Error"),
                );
                failures.push((agent_id, error.to_string()));
            }
        }
    }
    failures
}

pub fn default_legacy_member_dirname() -> &'static str {
    "members"
}

fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn legacy_member_path(root: &Path, agent_id: &str) -> PathBuf {
    root.join(agent_id).join(default_legacy_member_dirname())
}
