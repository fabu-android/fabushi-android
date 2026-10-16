use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use crate::agents::agent_profile::get_sand_profile_path;
use crate::automations::automation_store::agent_has_automations;
use crate::extensions::memory::memory_service::agent_memory_has_content;
use crate::storage::store_db::{
    get_sand_agent_db_write_generation, live_db_handle_count,
};
use crate::workflows::workflow_store::agent_has_workflows;

use super::agent_db::reseed_minimal_persisted_agent_db_if_missing;
use super::session_diagnostics::{SessionDiagnostic, report_session_diagnostic};
use super::session_mutations::recover_agent_with_missing_db;
use super::session_paths::get_agent_db_path;
use super::session_summaries::{
    AgentSummary, DbExtras, DurableFootprint, build_summary, file_mtime_ms,
    load_agent_db_extras, minimal_agent_summary,
};

#[derive(Debug, Clone)]
struct CachedDbExtras {
    key: String,
    extras: DbExtras,
}

#[derive(Debug, Default)]
pub struct RosterExtrasCache {
    entries: Mutex<BTreeMap<String, CachedDbExtras>>,
}

impl RosterExtrasCache {
    pub fn load(
        &self,
        db_path: &Path,
        busy_timeout_ms: u64,
        dir_name: &str,
        mtime_ms: Option<f64>,
    ) -> Result<DbExtras, String> {
        let key = roster_cache_key(db_path);
        let profile_exists = db_path
            .parent()
            .map(get_sand_profile_path)
            .is_some_and(|path| path.is_file());
        if profile_exists {
            if let Some(cached) = self
                .entries
                .lock()
                .ok()
                .and_then(|entries| entries.get(dir_name).cloned())
                .filter(|cached| cached.key == key)
            {
                return Ok(cached.extras);
            }
        }

        let extras = load_agent_db_extras(
            db_path,
            busy_timeout_ms,
            dir_name,
            mtime_ms,
        )?;
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(
                dir_name.to_string(),
                CachedDbExtras {
                    key,
                    extras: extras.clone(),
                },
            );
        }
        Ok(extras)
    }

    pub fn prune(&self, ids: &BTreeSet<String>) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|id, _| ids.contains(id));
        }
    }

    pub fn remove(&self, agent_id: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.remove(agent_id);
        }
    }

    pub fn entry_count(&self) -> usize {
        self.entries.lock().map(|entries| entries.len()).unwrap_or_default()
    }
}

fn roster_cache_key(db_path: &Path) -> String {
    let generation = get_sand_agent_db_write_generation(db_path);
    let (db_size, db_mtime) = stat_identity(db_path);
    let wal_path = PathBuf::from(format!("{}-wal", db_path.display()));
    let (wal_size, wal_mtime) = stat_identity(&wal_path);
    format!("{generation}:{db_size}:{db_mtime}:{wal_size}:{wal_mtime}")
}

fn stat_identity(path: &Path) -> (i128, i128) {
    let Ok(metadata) = fs::metadata(path) else {
        return (-1, -1);
    };
    let size = i128::from(metadata.len());
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i128::try_from(duration.as_nanos()).ok())
        .unwrap_or(-1);
    (size, mtime)
}

fn durable_footprint(agent_dir: &Path) -> DurableFootprint {
    DurableFootprint {
        has_memory: agent_memory_has_content(agent_dir),
        has_automations: agent_has_automations(agent_dir),
        has_workflows: agent_has_workflows(agent_dir),
    }
}

fn report_summary_diagnostic(kind: &str, agent_id: &str) {
    report_session_diagnostic(&SessionDiagnostic {
        family: "summary_build".into(),
        kind: kind.into(),
        metadata: BTreeMap::from([
            (
                "agentId".into(),
                serde_json::Value::String(agent_id.to_string()),
            ),
            (
                "errorClass".into(),
                serde_json::Value::String("Error".into()),
            ),
        ]),
    });
}

fn is_reserved_session_namespace(id: &str) -> bool {
    id.starts_with('.')
}

pub fn list_agents(
    root_dir: &Path,
    busy_timeout_ms: u64,
    active_agent_id: Option<&str>,
    is_agent_being_deleted: &dyn Fn(&str) -> bool,
    extras_cache: &RosterExtrasCache,
) -> Result<Vec<AgentSummary>, String> {
    let entries = match fs::read_dir(root_dir) {
        Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let roster_ids = entries
        .iter()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|id| !is_reserved_session_namespace(id))
        .collect::<BTreeSet<_>>();
    let mut summaries = Vec::new();

    for entry in entries {
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        if is_reserved_session_namespace(&dir_name) {
            continue;
        }
        if is_agent_being_deleted(&dir_name) {
            continue;
        }
        let db_path = get_agent_db_path(root_dir, &dir_name)
            .map_err(|error| error.to_string())?;
        let mtime_ms = file_mtime_ms(&db_path);
        let footprint = durable_footprint(entry.path().as_path());

        if !db_path.is_file() {
            match recover_agent_with_missing_db(
                &db_path,
                &dir_name,
                active_agent_id,
                footprint,
                live_db_handle_count(&db_path) > 0,
                || is_agent_being_deleted(&dir_name),
                |path| {
                    reseed_minimal_persisted_agent_db_if_missing(path, busy_timeout_ms)
                        .map_err(|error| error.to_string())
                },
            ) {
                Ok(Some(summary)) => summaries.push(summary),
                Ok(None) => {}
                Err(_) => {
                    report_summary_diagnostic("recovery_build_failed", &dir_name);
                    summaries.push(minimal_agent_summary(
                        &dir_name,
                        &db_path,
                        mtime_ms,
                        active_agent_id,
                    ));
                }
            }
            continue;
        }

        let primary = (|| -> Result<Option<AgentSummary>, String> {
            let extras = extras_cache.load(
                &db_path,
                busy_timeout_ms,
                &dir_name,
                mtime_ms,
            )?;
            if is_agent_being_deleted(&dir_name) {
                return Ok(None);
            }
            build_summary(
                Some(&extras),
                &db_path,
                &dir_name,
                mtime_ms,
                active_agent_id,
                false,
                footprint,
            )
        })();

        let summary = match primary {
            Ok(summary) => summary,
            Err(_) => {
                if is_agent_being_deleted(&dir_name) {
                    None
                } else {
                    report_summary_diagnostic("degraded", &dir_name);
                    match build_summary(
                        None,
                        &db_path,
                        &dir_name,
                        mtime_ms,
                        active_agent_id,
                        true,
                        footprint,
                    ) {
                        Ok(summary) => summary,
                        Err(_) => {
                            report_summary_diagnostic("degraded_failed", &dir_name);
                            Some(minimal_agent_summary(
                                &dir_name,
                                &db_path,
                                mtime_ms,
                                active_agent_id,
                            ))
                        }
                    }
                }
            }
        };
        if let Some(summary) = summary {
            summaries.push(summary);
        }
    }

    extras_cache.prune(&roster_ids);
    summaries.sort_by(|a, b| {
        b.updated_at
            .partial_cmp(&a.updated_at)
            .unwrap_or(Ordering::Equal)
    });
    Ok(summaries)
}

pub fn summarize_agent_by_id(
    root_dir: &Path,
    busy_timeout_ms: u64,
    agent_id: &str,
    active_agent_id: Option<&str>,
    is_agent_being_deleted: &dyn Fn(&str) -> bool,
    extras_cache: &RosterExtrasCache,
) -> Result<Option<AgentSummary>, String> {
    if is_reserved_session_namespace(agent_id) || is_agent_being_deleted(agent_id) {
        return Ok(None);
    }
    let db_path = get_agent_db_path(root_dir, agent_id)
        .map_err(|error| error.to_string())?;
    let mtime_ms = file_mtime_ms(&db_path);
    let footprint = durable_footprint(db_path.parent().unwrap_or_else(|| Path::new(".")));
    if !db_path.is_file() {
        return recover_agent_with_missing_db(
            &db_path,
            agent_id,
            active_agent_id,
            footprint,
            live_db_handle_count(&db_path) > 0,
            || is_agent_being_deleted(agent_id),
            |path| {
                reseed_minimal_persisted_agent_db_if_missing(path, busy_timeout_ms)
                    .map_err(|error| error.to_string())
            },
        );
    }
    let extras = extras_cache.load(
        &db_path,
        busy_timeout_ms,
        agent_id,
        mtime_ms,
    )?;
    if is_agent_being_deleted(agent_id) {
        return Ok(None);
    }
    build_summary(
        Some(&extras),
        &db_path,
        agent_id,
        mtime_ms,
        active_agent_id,
        true,
        footprint,
    )
}
