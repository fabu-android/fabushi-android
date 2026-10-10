use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::Value;

use crate::durable_file_policy::SAND_PENDING_WAKE_FILE_NAME;

/// Durable background-work kind mirrored from Desktop. Android persists an
/// account fence alongside the Desktop fields because its Host data root is
/// shared across account sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PendingWakeKind {
    CloudAgent,
    Subagent,
    Shell,
}

impl PendingWakeKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "cloud-agent" => Some(Self::CloudAgent),
            "subagent" => Some(Self::Subagent),
            "shell" => Some(Self::Shell),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationQuietOrigin {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuietWakeOrigin {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automation: Option<AutomationQuietOrigin>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DurablePendingWakeMarker {
    pub account_fence: String,
    pub agent_id: String,
    pub kind: PendingWakeKind,
    pub work_id: String,
    pub marked_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quiet_origin: Option<QuietWakeOrigin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_type: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub interrupted_by_recreate: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

pub fn coerce_quiet_origin(value: &Value) -> Option<QuietWakeOrigin> {
    let object = value.as_object()?;
    let automation = object
        .get("automation")
        .and_then(Value::as_object)
        .and_then(|automation| {
            let id = automation.get("id").and_then(Value::as_str)?;
            let name = automation.get("name").and_then(Value::as_str)?;
            (!id.is_empty()).then(|| AutomationQuietOrigin {
                id: id.to_string(),
                name: name.to_string(),
            })
        });
    Some(QuietWakeOrigin { automation })
}

pub fn coerce_marker(entry: &Value) -> Option<DurablePendingWakeMarker> {
    let object = entry.as_object()?;
    // Fail closed for legacy/global rows without an account fence. Android's
    // Host root is not account-scoped, so accepting such a row could expose
    // background work to a different account that reused an Agent id.
    let account_fence = object
        .get("accountFence")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?;
    let agent_id = object
        .get("agentId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?;
    let work_id = object
        .get("workId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?;
    let kind = PendingWakeKind::parse(object.get("kind")?.as_str()?)?;
    let marked_at_ms = object
        .get("markedAtMs")
        .and_then(|value| {
            value.as_u64().or_else(|| {
                value.as_f64().and_then(|value| {
                    (value.is_finite() && value >= 0.0 && value <= u64::MAX as f64)
                        .then_some(value as u64)
                })
            })
        })
        .unwrap_or(0);
    let quiet_origin = object.get("quietOrigin").and_then(coerce_quiet_origin);
    let title = object
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let subagent_type = object
        .get("subagentType")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    Some(DurablePendingWakeMarker {
        account_fence: account_fence.to_string(),
        agent_id: agent_id.to_string(),
        kind,
        work_id: work_id.to_string(),
        marked_at_ms,
        quiet_origin,
        title,
        subagent_type,
        interrupted_by_recreate: object
            .get("interruptedByRecreate")
            .and_then(Value::as_bool)
            == Some(true),
    })
}

pub fn parse_pending_wake_file(raw: Option<&str>) -> Vec<DurablePendingWakeMarker> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    value
        .get("pending")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(coerce_marker).collect())
        .unwrap_or_default()
}

fn marker_key_matches(
    marker: &DurablePendingWakeMarker,
    account_fence: &str,
    agent_id: &str,
    kind: PendingWakeKind,
    work_id: &str,
) -> bool {
    marker.account_fence == account_fence
        && marker.agent_id == agent_id
        && marker.kind == kind
        && marker.work_id == work_id
}

fn upsert_pending_wake_marker(
    existing: &[DurablePendingWakeMarker],
    marker: DurablePendingWakeMarker,
) -> Vec<DurablePendingWakeMarker> {
    let mut next = existing
        .iter()
        .filter(|entry| {
            !marker_key_matches(
                entry,
                &marker.account_fence,
                &marker.agent_id,
                marker.kind,
                &marker.work_id,
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    next.push(marker);
    next
}

#[derive(Clone)]
pub struct SandPendingWakeStore {
    file_path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl SandPendingWakeStore {
    pub fn new(root_dir: impl AsRef<Path>) -> Self {
        Self {
            file_path: root_dir.as_ref().join(SAND_PENDING_WAKE_FILE_NAME),
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn mark_pending(&self, marker: DurablePendingWakeMarker) -> bool {
        if marker.account_fence.is_empty() || marker.agent_id.is_empty() || marker.work_id.is_empty() {
            return false;
        }
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let pending = upsert_pending_wake_marker(&self.read_pending_unlocked(), marker);
        self.write_unlocked(&pending).is_ok()
    }

    pub fn list_pending_for(
        &self,
        account_fence: &str,
        agent_id: &str,
    ) -> Vec<DurablePendingWakeMarker> {
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.read_pending_unlocked()
            .into_iter()
            .filter(|entry| entry.account_fence == account_fence && entry.agent_id == agent_id)
            .collect()
    }

    pub fn list_pending_for_account(
        &self,
        account_fence: &str,
    ) -> Vec<DurablePendingWakeMarker> {
        if account_fence.trim().is_empty() {
            return Vec::new();
        }
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.read_pending_unlocked()
            .into_iter()
            .filter(|entry| entry.account_fence == account_fence)
            .collect()
    }

    pub fn has_pending(
        &self,
        account_fence: &str,
        agent_id: &str,
        kind: PendingWakeKind,
        work_id: &str,
    ) -> bool {
        if account_fence.trim().is_empty() {
            return false;
        }
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.read_pending_unlocked().iter().any(|entry| {
            marker_key_matches(entry, account_fence, agent_id, kind, work_id)
        })
    }

    pub fn clear_one(
        &self,
        account_fence: &str,
        agent_id: &str,
        kind: PendingWakeKind,
        work_id: &str,
    ) -> bool {
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let existing = self.read_pending_unlocked();
        let remaining = existing
            .iter()
            .filter(|entry| !marker_key_matches(entry, account_fence, agent_id, kind, work_id))
            .cloned()
            .collect::<Vec<_>>();
        if remaining.len() == existing.len() {
            return false;
        }
        if remaining.is_empty() {
            self.delete_unlocked().is_ok()
        } else {
            self.write_unlocked(&remaining).is_ok()
        }
    }

    pub fn clear_account(&self, account_fence: &str) {
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let existing = self.read_pending_unlocked();
        let remaining = existing
            .iter()
            .filter(|entry| entry.account_fence != account_fence)
            .cloned()
            .collect::<Vec<_>>();
        if remaining.len() == existing.len() {
            return;
        }
        if remaining.is_empty() {
            let _ = self.delete_unlocked();
        } else {
            let _ = self.write_unlocked(&remaining);
        }
    }

    pub fn prune_stale_for_account(
        &self,
        account_fence: &str,
        max_age_ms: u64,
        now_ms: u64,
    ) -> Vec<DurablePendingWakeMarker> {
        if account_fence.trim().is_empty() {
            return Vec::new();
        }
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let existing = self.read_pending_unlocked();
        let mut pruned = Vec::new();
        let mut remaining = Vec::new();
        for entry in existing {
            if entry.account_fence == account_fence
                && now_ms.saturating_sub(entry.marked_at_ms) > max_age_ms
            {
                pruned.push(entry);
            } else {
                remaining.push(entry);
            }
        }
        if pruned.is_empty() {
            return Vec::new();
        }
        if remaining.is_empty() {
            let _ = self.delete_unlocked();
        } else {
            let _ = self.write_unlocked(&remaining);
        }
        pruned
    }

    pub fn prune_stale(
        &self,
        max_age_ms: u64,
        now_ms: u64,
    ) -> Vec<DurablePendingWakeMarker> {
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let existing = self.read_pending_unlocked();
        let (pruned, remaining): (Vec<_>, Vec<_>) = existing
            .into_iter()
            .partition(|entry| now_ms.saturating_sub(entry.marked_at_ms) > max_age_ms);
        if pruned.is_empty() {
            return Vec::new();
        }
        if remaining.is_empty() {
            let _ = self.delete_unlocked();
        } else {
            let _ = self.write_unlocked(&remaining);
        }
        pruned
    }

    fn read_pending_unlocked(&self) -> Vec<DurablePendingWakeMarker> {
        fs::read_to_string(&self.file_path)
            .ok()
            .map(|raw| parse_pending_wake_file(Some(&raw)))
            .unwrap_or_default()
    }

    fn write_unlocked(&self, pending: &[DurablePendingWakeMarker]) -> std::io::Result<()> {
        #[derive(Serialize)]
        struct File<'a> {
            version: u8,
            pending: &'a [DurablePendingWakeMarker],
        }
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let part = PathBuf::from(format!("{}.part", self.file_path.display()));
        fs::write(
            &part,
            serde_json::to_vec(&File { version: 2, pending })
                .map_err(std::io::Error::other)?,
        )?;
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
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-pending-wake-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn marker(account: &str, agent: &str, kind: PendingWakeKind, work: &str) -> DurablePendingWakeMarker {
        DurablePendingWakeMarker {
            account_fence: account.into(),
            agent_id: agent.into(),
            kind,
            work_id: work.into(),
            marked_at_ms: 10,
            quiet_origin: None,
            title: None,
            subagent_type: None,
            interrupted_by_recreate: false,
        }
    }

    #[test]
    fn persisted_markers_are_fenced_by_account_and_agent_after_reopen() {
        let root = root("account-fence");
        {
            let store = SandPendingWakeStore::new(&root);
            assert!(store.mark_pending(marker("acct-a", "agent-1", PendingWakeKind::Shell, "shell-1")));
            assert!(store.mark_pending(marker("acct-b", "agent-1", PendingWakeKind::CloudAgent, "cloud-1")));
        }
        let reopened = SandPendingWakeStore::new(&root);
        let a = reopened.list_pending_for("acct-a", "agent-1");
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].work_id, "shell-1");
        let b = reopened.list_pending_for("acct-b", "agent-1");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].work_id, "cloud-1");
        assert!(reopened.list_pending_for("acct-c", "agent-1").is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_unfenced_rows_fail_closed() {
        let raw = r#"{"version":1,"pending":[{"agentId":"agent-1","kind":"shell","workId":"shell-1","markedAtMs":1}]}"#;
        assert!(parse_pending_wake_file(Some(raw)).is_empty());
    }

    #[test]
    fn clear_one_does_not_cross_account_fence() {
        let root = root("clear");
        let store = SandPendingWakeStore::new(&root);
        assert!(store.mark_pending(marker("acct-a", "agent-1", PendingWakeKind::Shell, "same")));
        assert!(store.mark_pending(marker("acct-b", "agent-1", PendingWakeKind::Shell, "same")));
        assert!(store.clear_one("acct-a", "agent-1", PendingWakeKind::Shell, "same"));
        assert!(store.list_pending_for("acct-a", "agent-1").is_empty());
        assert_eq!(store.list_pending_for("acct-b", "agent-1").len(), 1);
        let _ = fs::remove_dir_all(root);
    }
}
