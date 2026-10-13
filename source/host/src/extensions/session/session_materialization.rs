use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};
use uuid::Uuid;

use crate::agents::agent_profile::{
    SandAgentProfile, get_sand_profile_path, read_sand_profile_file, write_sand_profile_file,
};
use crate::agents::settings_file::{
    get_sand_settings_path, write_sand_settings_file,
};

use super::agent_db::{
    AgentDbProjectionError, initialize_persisted_agent_record, read_persisted_agent_name,
    read_persisted_agent_serde_snapshot,
};
use super::session_diagnostics::{SessionDiagnostic, report_session_diagnostic};
use super::session_paths::get_agent_db_path;
use super::session_recovery::{ensure_profile_file, ensure_settings_file};

pub const MAX_AGENTS_PER_USER: usize = 50;
pub const DEFAULT_AGENT_AUTOMATIONS: &[serde_json::Value] = &[];

#[derive(Default)]
pub struct SessionMintQueue {
    lock: Mutex<()>,
}

impl SessionMintQueue {
    pub fn run<T>(&self, mint: impl FnOnce() -> T) -> T {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        mint()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionMaterializationError {
    #[error("Sand agent {0} does not exist")]
    Missing(String),
    #[error("Agent limit of {MAX_AGENTS_PER_USER} reached")]
    Limit,
    #[error("session materialization filesystem error: {0}")]
    Io(#[from] io::Error),
    #[error("session materialization database error: {0}")]
    Database(#[from] AgentDbProjectionError),
    #[error("session materialization path error: {0}")]
    Path(String),
}

pub fn report_materialization_failure(
    kind: &str,
    agent_id: &str,
    error_class: &str,
) {
    let mut metadata = BTreeMap::new();
    metadata.insert("agentId".into(), Value::String(agent_id.to_string()));
    metadata.insert("errorClass".into(), Value::String(error_class.to_string()));
    report_session_diagnostic(&SessionDiagnostic {
        family: "materialize".into(),
        kind: kind.to_string(),
        metadata,
    });
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedAgentRecord {
    pub id: String,
    pub db_path: PathBuf,
    pub profile: SandAgentProfile,
}

pub fn list_agent_record_ids(root_dir: &Path) -> Result<Vec<String>, io::Error> {
    let entries = match fs::read_dir(root_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut ids = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_type().ok().filter(|kind| kind.is_dir()).map(|_| entry))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|id| !id.starts_with('.'))
        .collect::<Vec<_>>();
    ids.sort();
    Ok(ids)
}

pub fn count_owned_agents(root_dir: &Path) -> Result<usize, io::Error> {
    Ok(list_agent_record_ids(root_dir)?.len())
}

pub fn is_agent_cap_reached(root_dir: &Path) -> Result<bool, io::Error> {
    Ok(count_owned_agents(root_dir)? >= MAX_AGENTS_PER_USER)
}

pub fn list_pruned_placeholder_ids(
    root_dir: &Path,
    active_agent_id: Option<&str>,
    visible_agent_ids: &BTreeSet<String>,
) -> Result<Vec<String>, io::Error> {
    let mut candidates = Vec::new();
    for agent_id in list_agent_record_ids(root_dir)? {
        if active_agent_id == Some(agent_id.as_str()) || visible_agent_ids.contains(&agent_id) {
            continue;
        }
        let Ok(db_path) = get_agent_db_path(root_dir, &agent_id) else {
            continue;
        };
        if db_path.is_file() {
            candidates.push(agent_id);
        }
    }
    Ok(candidates)
}

pub fn agent_exists(root_dir: &Path, agent_id: &str) -> bool {
    get_agent_db_path(root_dir, agent_id)
        .map(|path| path.is_file())
        .unwrap_or(false)
}

pub fn materialize_new_session(
    root_dir: &Path,
    busy_timeout_ms: u64,
    profile: Option<&SandAgentProfile>,
    origin: &str,
    purpose: Option<&str>,
) -> Result<MaterializedAgentRecord, SessionMaterializationError> {
    fs::create_dir_all(root_dir)?;
    if is_agent_cap_reached(root_dir)? {
        return Err(SessionMaterializationError::Limit);
    }

    let agent_id = loop {
        let candidate = Uuid::new_v4().to_string();
        if !root_dir.join(&candidate).exists() {
            break candidate;
        }
    };
    let agent_dir = root_dir.join(&agent_id);
    fs::create_dir_all(&agent_dir)?;
    let result = (|| {
        let db_path = get_agent_db_path(root_dir, &agent_id)
            .map_err(|error| SessionMaterializationError::Path(error.to_string()))?;
        initialize_persisted_agent_record(
            &db_path,
            busy_timeout_ms,
            &agent_id,
            origin,
            purpose,
            now_ms(),
            &generate_blob_encryption_key_hex(),
        )?;

        let normalized = normalize_profile(profile);
        write_sand_profile_file(get_sand_profile_path(&agent_dir), &normalized)?;
        let mut settings = Map::new();
        settings.insert("notifyOnAgentUpdates".into(), Value::Bool(true));
        write_sand_settings_file(get_sand_settings_path(&agent_dir), &settings)?;

        Ok(MaterializedAgentRecord {
            id: agent_id.clone(),
            db_path,
            profile: normalized,
        })
    })();

    if result.is_err() {
        if let Err(cleanup_error) = fs::remove_dir_all(&agent_dir) {
            if cleanup_error.kind() != io::ErrorKind::NotFound {
                report_materialization_failure(
                    "mint_cleanup_failed",
                    &agent_id,
                    &format!("io::{:?}", cleanup_error.kind()),
                );
            }
        }
    }
    result
}

pub fn open_existing_session(
    root_dir: &Path,
    busy_timeout_ms: u64,
    agent_id: &str,
) -> Result<Option<MaterializedAgentRecord>, SessionMaterializationError> {
    let db_path = get_agent_db_path(root_dir, agent_id)
        .map_err(|error| SessionMaterializationError::Path(error.to_string()))?;
    if !db_path.is_file() {
        return Ok(None);
    }
    let session_state = read_persisted_agent_serde_snapshot(&db_path, busy_timeout_ms)?;
    let name = read_persisted_agent_name(&db_path, busy_timeout_ms)?;
    let profile_path = ensure_profile_file(
        &db_path,
        name.as_deref(),
        &session_state.profile.description,
    )?;
    let _settings_path = ensure_settings_file(&db_path)?;
    let profile = read_sand_profile_file(&profile_path).unwrap_or_else(|| SandAgentProfile {
        name: name.unwrap_or_else(|| "Grok".to_string()),
        description: session_state.profile.description.trim().to_string(),
        title: String::new(),
        avatar_shape: String::new(),
        avatar_color: String::new(),
    });
    Ok(Some(MaterializedAgentRecord {
        id: agent_id.to_string(),
        db_path,
        profile,
    }))
}

fn normalize_profile(profile: Option<&SandAgentProfile>) -> SandAgentProfile {
    let profile = profile.cloned().unwrap_or_else(|| SandAgentProfile {
        name: "Grok".to_string(),
        description: String::new(),
        title: String::new(),
        avatar_shape: String::new(),
        avatar_color: String::new(),
    });
    let name = profile.name.trim();
    SandAgentProfile {
        name: if name.is_empty() { "Grok".to_string() } else { name.to_string() },
        description: profile.description.trim().to_string(),
        title: profile.title.trim().to_string(),
        avatar_shape: profile.avatar_shape.trim().to_string(),
        avatar_color: profile.avatar_color.trim().to_string(),
    }
}

fn generate_blob_encryption_key_hex() -> String {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    first
        .as_bytes()
        .iter()
        .chain(second.as_bytes().iter())
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
