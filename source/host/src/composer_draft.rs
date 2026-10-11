use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const SCHEMA_VERSION: u8 = 1;
const MAX_PROMPT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ComposerDraftRecord {
    pub account_slot: Option<String>,
    pub account_fence: String,
    pub agent_id: String,
    pub draft_id: String,
    pub prompt: String,
    pub updated_at_ms: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct ComposerDraftState {
    schema_version: u8,
    next_id: u64,
    records: BTreeMap<String, ComposerDraftRecord>,
}

impl Default for ComposerDraftState {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            next_id: 0,
            records: BTreeMap::new(),
        }
    }
}

pub struct DurableComposerDraftOwner {
    path: PathBuf,
    state: ComposerDraftState,
}

impl DurableComposerDraftOwner {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let path = path.into();
        let state = match fs::read(&path) {
            Ok(raw) => match serde_json::from_slice::<ComposerDraftState>(&raw) {
                Ok(state) if state.schema_version == SCHEMA_VERSION => state,
                Ok(_) | Err(_) => {
                    quarantine_corrupt(&path, now_ms)?;
                    ComposerDraftState::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => ComposerDraftState::default(),
            Err(error) => return Err(error.to_string()),
        };
        let owner = Self { path, state };
        owner.persist()?;
        Ok(owner)
    }

    pub fn read(
        &self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
    ) -> Result<Option<ComposerDraftRecord>, String> {
        validate_scope(account_slot, account_fence, agent_id)?;
        Ok(self
            .state
            .records
            .get(&scope_key(account_slot, account_fence, agent_id)?)
            .cloned())
    }

    pub fn write(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        prompt: &str,
        now_ms: u64,
    ) -> Result<ComposerDraftRecord, String> {
        validate_scope(account_slot, account_fence, agent_id)?;
        if prompt.len() > MAX_PROMPT_BYTES {
            return Err("composer draft prompt is too large".into());
        }
        let key = scope_key(account_slot, account_fence, agent_id)?;
        if let Some(existing) = self.state.records.get(&key) {
            if existing.prompt == prompt {
                return Ok(existing.clone());
            }
        }
        self.state.next_id = self.state.next_id.saturating_add(1).max(1);
        let record = ComposerDraftRecord {
            account_slot: account_slot.map(str::to_string),
            account_fence: account_fence.to_string(),
            agent_id: agent_id.to_string(),
            draft_id: format!("android-composer-draft-{}", self.state.next_id),
            prompt: prompt.to_string(),
            updated_at_ms: now_ms,
        };
        self.state.records.insert(key, record.clone());
        self.persist()?;
        Ok(record)
    }

    pub fn clear_if_current(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        draft_id: &str,
    ) -> Result<bool, String> {
        validate_scope(account_slot, account_fence, agent_id)?;
        validate_identity(draft_id, "draft id")?;
        let key = scope_key(account_slot, account_fence, agent_id)?;
        let is_current = self
            .state
            .records
            .get(&key)
            .is_some_and(|record| record.draft_id == draft_id);
        if !is_current {
            return Ok(false);
        }
        self.state.records.remove(&key);
        self.persist()?;
        Ok(true)
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let encoded = serde_json::to_vec_pretty(&self.state).map_err(|error| error.to_string())?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, encoded).map_err(|error| error.to_string())?;
        fs::rename(&tmp, &self.path).map_err(|error| error.to_string())
    }
}

fn scope_key(
    account_slot: Option<&str>,
    account_fence: &str,
    agent_id: &str,
) -> Result<String, String> {
    serde_json::to_string(&(account_slot, account_fence, agent_id))
        .map_err(|error| format!("failed to encode composer draft scope: {error}"))
}

fn validate_scope(
    account_slot: Option<&str>,
    account_fence: &str,
    agent_id: &str,
) -> Result<(), String> {
    if let Some(slot) = account_slot {
        validate_identity(slot, "account slot")?;
    }
    validate_identity(account_fence, "account fence")?;
    validate_identity(agent_id, "agent id")
}

fn validate_identity(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

fn quarantine_corrupt(path: &Path, now_ms: u64) -> Result<(), String> {
    let corrupt = path.with_extension(format!("corrupt-{now_ms}.json"));
    fs::rename(path, corrupt).map_err(|error| error.to_string())
}
