use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::Value;

use crate::durable_file_policy::SAND_UPGRADE_RESUME_FILE_NAME;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeResumeMarker {
    pub agent_id: String,
    pub marked_at_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automation_run_id: Option<String>,
}

pub fn coerce_upgrade_resume_marker(entry: &Value) -> Option<UpgradeResumeMarker> {
    let object = entry.as_object()?;
    let agent_id = object
        .get("agentId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?;
    Some(UpgradeResumeMarker {
        agent_id: agent_id.to_string(),
        marked_at_ms: object
            .get("markedAtMs")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .unwrap_or(0.0),
        source: object.get("source").and_then(Value::as_str).map(ToOwned::to_owned),
        automation_id: object
            .get("automationId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        automation_run_id: object
            .get("automationRunId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
    })
}

pub fn parse_upgrade_resume_file(raw: Option<&str>) -> Vec<UpgradeResumeMarker> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    value
        .get("pending")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(coerce_upgrade_resume_marker).collect())
        .unwrap_or_default()
}

pub fn upsert_resume_marker(
    existing: &[UpgradeResumeMarker],
    marker: UpgradeResumeMarker,
) -> Vec<UpgradeResumeMarker> {
    let mut next = existing
        .iter()
        .filter(|entry| entry.agent_id != marker.agent_id)
        .cloned()
        .collect::<Vec<_>>();
    next.push(marker);
    next
}

#[derive(Clone)]
pub struct SandUpgradeResumeStore {
    file_path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl SandUpgradeResumeStore {
    pub fn new(root_dir: impl AsRef<Path>) -> Self {
        Self {
            file_path: root_dir.as_ref().join(SAND_UPGRADE_RESUME_FILE_NAME),
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    pub fn mark_pending(&self, marker: UpgradeResumeMarker) {
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let pending = upsert_resume_marker(&self.read_pending_unlocked(), marker);
        let _ = self.write_unlocked(&pending);
    }

    pub fn list_pending(&self) -> Vec<UpgradeResumeMarker> {
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.read_pending_unlocked()
    }

    pub fn clear(&self, agent_id: &str) {
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let remaining = self
            .read_pending_unlocked()
            .into_iter()
            .filter(|entry| entry.agent_id != agent_id)
            .collect::<Vec<_>>();
        if remaining.is_empty() {
            let _ = self.delete_unlocked();
        } else {
            let _ = self.write_unlocked(&remaining);
        }
    }

    pub fn clear_all(&self) {
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = self.delete_unlocked();
    }

    fn read_pending_unlocked(&self) -> Vec<UpgradeResumeMarker> {
        fs::read_to_string(&self.file_path)
            .ok()
            .map(|raw| parse_upgrade_resume_file(Some(&raw)))
            .unwrap_or_default()
    }

    fn write_unlocked(&self, pending: &[UpgradeResumeMarker]) -> std::io::Result<()> {
        #[derive(Serialize)]
        struct File<'a> {
            version: u8,
            pending: &'a [UpgradeResumeMarker],
        }
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let part = PathBuf::from(format!("{}.part", self.file_path.display()));
        let encoded = serde_json::to_vec(&File {
            version: 1,
            pending,
        })
        .map_err(std::io::Error::other)?;
        let mut file = fs::File::create(&part)?;
        file.write_all(&encoded)?;
        file.sync_all()?;
        replace_file(&part, &self.file_path)
    }

    fn delete_unlocked(&self) -> std::io::Result<()> {
        match fs::remove_file(&self.file_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

fn replace_file(part: &Path, target: &Path) -> std::io::Result<()> {
    match fs::rename(part, target) {
        Ok(()) => Ok(()),
        Err(first) if target.exists() => {
            fs::remove_file(target)?;
            fs::rename(part, target).map_err(|_| first)
        }
        Err(error) => Err(error),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn marker(agent_id: &str, run: &str) -> UpgradeResumeMarker {
        UpgradeResumeMarker {
            agent_id: agent_id.into(),
            marked_at_ms: 42.0,
            source: Some("forced-upgrade".into()),
            automation_id: Some("automation-a".into()),
            automation_run_id: Some(run.into()),
        }
    }

    #[test]
    fn marker_upsert_is_per_agent_and_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let store = SandUpgradeResumeStore::new(dir.path());
        store.mark_pending(marker("agent-a", "run-1"));
        store.mark_pending(marker("agent-b", "run-b"));
        store.mark_pending(marker("agent-a", "run-2"));

        let reopened = SandUpgradeResumeStore::new(dir.path());
        let pending = reopened.list_pending();
        assert_eq!(pending.len(), 2);
        assert_eq!(
            pending
                .iter()
                .find(|entry| entry.agent_id == "agent-a")
                .and_then(|entry| entry.automation_run_id.as_deref()),
            Some("run-2")
        );
        assert!(!PathBuf::from(format!("{}.part", reopened.file_path().display())).exists());
    }

    #[test]
    fn clear_and_clear_all_remove_only_requested_resume_state() {
        let dir = tempfile::tempdir().unwrap();
        let store = SandUpgradeResumeStore::new(dir.path());
        store.mark_pending(marker("agent-a", "run-a"));
        store.mark_pending(marker("agent-b", "run-b"));
        store.clear("agent-a");
        assert_eq!(
            store
                .list_pending()
                .into_iter()
                .map(|entry| entry.agent_id)
                .collect::<Vec<_>>(),
            vec!["agent-b"]
        );
        store.clear_all();
        assert!(store.list_pending().is_empty());
        assert!(!store.file_path().exists());
    }

    #[test]
    fn malformed_or_partial_document_never_fabricates_resume_markers() {
        assert!(parse_upgrade_resume_file(Some("{not-json")).is_empty());
        assert!(parse_upgrade_resume_file(Some(r#"{"pending":[{},{"agentId":""}]}"#)).is_empty());
        let parsed = parse_upgrade_resume_file(Some(
            r#"{"pending":[{"agentId":"agent-a","markedAtMs":9,"source":"upgrade"}]}"#,
        ));
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].agent_id, "agent-a");
        assert_eq!(parsed[0].marked_at_ms, 9.0);
    }
}
