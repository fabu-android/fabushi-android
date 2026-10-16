use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

pub const ENABLEMENT_FILENAME: &str = "enabled-workflows.json";

pub fn get_agent_workflow_enablement_path(agent_dir: impl AsRef<Path>) -> PathBuf {
    agent_dir.as_ref().join(ENABLEMENT_FILENAME)
}

fn to_string_set(value: Option<&Value>) -> BTreeSet<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
}

fn read_file(path: &Path) -> Map<String, Value> {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

pub struct AgentWorkflowEnablement {
    agent_dir: PathBuf,
}

impl AgentWorkflowEnablement {
    pub fn new(agent_dir: impl Into<PathBuf>) -> Self {
        Self {
            agent_dir: agent_dir.into(),
        }
    }

    pub fn path(&self) -> PathBuf {
        get_agent_workflow_enablement_path(&self.agent_dir)
    }

    fn read_disabled(&self) -> BTreeSet<String> {
        to_string_set(read_file(&self.path()).get("disabled"))
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        !self.read_disabled().contains(id)
    }

    pub fn has_explicit_entries(&self) -> bool {
        let file = read_file(&self.path());
        !to_string_set(file.get("disabled")).is_empty()
            || !to_string_set(file.get("enabled")).is_empty()
    }

    fn write(&self, disabled: &BTreeSet<String>) -> io::Result<()> {
        fs::create_dir_all(&self.agent_dir)?;
        let current = read_file(&self.path());
        let enabled = to_string_set(current.get("enabled"));
        let mut object = Map::new();
        object.insert(
            "disabled".into(),
            Value::Array(disabled.iter().cloned().map(Value::String).collect()),
        );
        if !enabled.is_empty() {
            object.insert(
                "enabled".into(),
                Value::Array(enabled.into_iter().map(Value::String).collect()),
            );
        }
        let mut raw = serde_json::to_string_pretty(&Value::Object(object))
            .map_err(io::Error::other)?;
        raw.push('\n');
        let path = self.path();
        let temporary = PathBuf::from(format!("{}.tmp", path.display()));
        fs::write(&temporary, raw)?;
        fs::rename(temporary, path)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> io::Result<bool> {
        let mut disabled = self.read_disabled();
        let changed = if enabled {
            disabled.remove(id)
        } else {
            disabled.insert(id.to_string())
        };
        if changed {
            self.write(&disabled)?;
        }
        Ok(changed)
    }

    pub fn forget(&self, id: &str) -> io::Result<bool> {
        let mut disabled = self.read_disabled();
        let changed = disabled.remove(id);
        if changed {
            self.write(&disabled)?;
        }
        Ok(changed)
    }

    pub fn enable_all(&self, ids: &[String]) -> io::Result<bool> {
        let mut disabled = self.read_disabled();
        let mut changed = false;
        for id in ids {
            changed |= disabled.remove(id);
        }
        if changed {
            self.write(&disabled)?;
        }
        Ok(changed)
    }
}
