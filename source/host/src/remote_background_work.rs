use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::remote_execution::RemoteExecutionRecord;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteBackgroundWorkKind {
    CloudAgent,
    Shell,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteBackgroundWorkState {
    Prepared,
    Watching,
    Settled,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RemoteBackgroundWorkRecord {
    pub parent_agent_id: String,
    pub work_id: String,
    pub title: String,
    pub kind: RemoteBackgroundWorkKind,
    pub account_fence: String,
    pub account_epoch: u64,
    pub device_id: String,
    pub operation_id: String,
    pub request_id: String,
    pub permission_grant_id: String,
    pub capability_id: String,
    pub state: RemoteBackgroundWorkState,
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
    #[serde(default)]
    pub interrupted_by_recreate: bool,
    #[serde(default)]
    pub terminal_status: Option<String>,
    #[serde(default)]
    pub terminal_detail: Option<String>,
    #[serde(default)]
    pub output_path: Option<String>,
}

#[derive(Default, Deserialize, Serialize)]
struct DurableRemoteBackgroundWork {
    records: BTreeMap<String, RemoteBackgroundWorkRecord>,
}

pub struct DurableRemoteBackgroundWorkOwner {
    path: PathBuf,
    state: DurableRemoteBackgroundWork,
}

impl DurableRemoteBackgroundWorkOwner {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let path = path.into();
        let mut state = if path.exists() {
            serde_json::from_slice::<DurableRemoteBackgroundWork>(
                &fs::read(&path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| format!("invalid remote background work store: {error}"))?
        } else {
            DurableRemoteBackgroundWork::default()
        };
        let mut changed = false;
        for record in state.records.values_mut() {
            if record.state == RemoteBackgroundWorkState::Watching
                && !record.interrupted_by_recreate
            {
                record.interrupted_by_recreate = true;
                record.updated_at_ms = now_ms;
                changed = true;
            }
        }
        let owner = Self { path, state };
        if changed {
            owner.persist()?;
        }
        Ok(owner)
    }

    pub fn register(
        &mut self,
        parent_agent_id: &str,
        work_id: &str,
        title: &str,
        kind: RemoteBackgroundWorkKind,
        remote: &RemoteExecutionRecord,
        now_ms: u64,
    ) -> Result<RemoteBackgroundWorkRecord, String> {
        validate_identity("parent Agent", parent_agent_id)?;
        validate_identity("background work", work_id)?;
        validate_identity("remote operation", &remote.operation_id)?;
        validate_identity("remote request", &remote.request_id)?;
        validate_identity("remote grant", &remote.permission_grant_id)?;
        validate_identity("remote device", &remote.device_id)?;
        validate_identity("account fence", &remote.account_fence)?;
        if remote.account_epoch == 0 {
            return Err("remote background work account epoch must be positive".into());
        }
        if let Some(existing) = self.state.records.get(work_id) {
            if existing.parent_agent_id == parent_agent_id
                && existing.kind == kind
                && existing.account_fence == remote.account_fence
                && existing.account_epoch == remote.account_epoch
                && existing.device_id == remote.device_id
                && existing.operation_id == remote.operation_id
                && existing.request_id == remote.request_id
                && existing.permission_grant_id == remote.permission_grant_id
                && existing.capability_id == remote.capability_id
            {
                return Ok(existing.clone());
            }
            return Err("background work identity conflicts with durable owner".into());
        }
        let record = RemoteBackgroundWorkRecord {
            parent_agent_id: parent_agent_id.to_string(),
            work_id: work_id.to_string(),
            title: title.trim().to_string(),
            kind,
            account_fence: remote.account_fence.clone(),
            account_epoch: remote.account_epoch,
            device_id: remote.device_id.clone(),
            operation_id: remote.operation_id.clone(),
            request_id: remote.request_id.clone(),
            permission_grant_id: remote.permission_grant_id.clone(),
            capability_id: remote.capability_id.clone(),
            state: RemoteBackgroundWorkState::Prepared,
            started_at_ms: now_ms,
            updated_at_ms: now_ms,
            interrupted_by_recreate: false,
            terminal_status: None,
            terminal_detail: None,
            output_path: None,
        };
        self.state.records.insert(work_id.to_string(), record.clone());
        self.persist()?;
        Ok(record)
    }

    pub fn mark_watching(
        &mut self,
        work_id: &str,
        account_fence: &str,
        account_epoch: u64,
        now_ms: u64,
    ) -> Result<RemoteBackgroundWorkRecord, String> {
        let record = self
            .state
            .records
            .get_mut(work_id)
            .ok_or("remote background work is unknown")?;
        assert_current(record, account_fence, account_epoch)?;
        match record.state {
            RemoteBackgroundWorkState::Prepared => {
                record.state = RemoteBackgroundWorkState::Watching;
                record.updated_at_ms = now_ms;
                let cloned = record.clone();
                self.persist()?;
                Ok(cloned)
            }
            RemoteBackgroundWorkState::Watching => Ok(record.clone()),
            RemoteBackgroundWorkState::Settled | RemoteBackgroundWorkState::Cancelled => {
                Err("terminal remote background work cannot start watching".into())
            }
        }
    }

    pub fn settle(
        &mut self,
        work_id: &str,
        account_fence: &str,
        account_epoch: u64,
        status: &str,
        detail: Option<String>,
        output_path: Option<String>,
        now_ms: u64,
    ) -> Result<RemoteBackgroundWorkRecord, String> {
        let record = self
            .state
            .records
            .get_mut(work_id)
            .ok_or("remote background work is unknown")?;
        assert_current(record, account_fence, account_epoch)?;
        if matches!(
            record.state,
            RemoteBackgroundWorkState::Prepared | RemoteBackgroundWorkState::Cancelled
        ) {
            return Err("remote background work must be watching before settlement".into());
        }
        if record.state == RemoteBackgroundWorkState::Settled {
            return Ok(record.clone());
        }
        record.state = RemoteBackgroundWorkState::Settled;
        record.terminal_status = Some(status.trim().to_string());
        record.terminal_detail = detail;
        record.output_path = output_path;
        record.updated_at_ms = now_ms;
        let cloned = record.clone();
        self.persist()?;
        Ok(cloned)
    }

    pub fn cancel(
        &mut self,
        work_id: &str,
        account_fence: &str,
        account_epoch: u64,
        now_ms: u64,
    ) -> Result<(), String> {
        let record = self
            .state
            .records
            .get_mut(work_id)
            .ok_or("remote background work is unknown")?;
        assert_current(record, account_fence, account_epoch)?;
        if record.state == RemoteBackgroundWorkState::Settled {
            return Err("settled remote background work cannot cancel".into());
        }
        record.state = RemoteBackgroundWorkState::Cancelled;
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn get(&self, work_id: &str) -> Option<&RemoteBackgroundWorkRecord> {
        self.state.records.get(work_id)
    }

    pub fn watching_for_account(
        &self,
        account_fence: &str,
        account_epoch: u64,
    ) -> Vec<RemoteBackgroundWorkRecord> {
        self.state
            .records
            .values()
            .filter(|record| {
                record.account_fence == account_fence
                    && record.account_epoch == account_epoch
                    && record.state == RemoteBackgroundWorkState::Watching
            })
            .cloned()
            .collect()
    }

    pub fn prepared_for_account(
        &self,
        account_fence: &str,
        account_epoch: u64,
    ) -> Vec<RemoteBackgroundWorkRecord> {
        self.state
            .records
            .values()
            .filter(|record| {
                record.account_fence == account_fence
                    && record.account_epoch == account_epoch
                    && record.state == RemoteBackgroundWorkState::Prepared
            })
            .cloned()
            .collect()
    }

    pub fn settled_for_fence(
        &self,
        account_fence: &str,
    ) -> Vec<RemoteBackgroundWorkRecord> {
        self.state
            .records
            .values()
            .filter(|record| {
                record.account_fence == account_fence
                    && record.state == RemoteBackgroundWorkState::Settled
            })
            .cloned()
            .collect()
    }

    pub fn settled_for_account(
        &self,
        account_fence: &str,
        account_epoch: u64,
    ) -> Vec<RemoteBackgroundWorkRecord> {
        self.state
            .records
            .values()
            .filter(|record| {
                record.account_fence == account_fence
                    && record.account_epoch == account_epoch
                    && record.state == RemoteBackgroundWorkState::Settled
            })
            .cloned()
            .collect()
    }

    pub fn remove_settled(
        &mut self,
        work_id: &str,
        account_fence: &str,
        account_epoch: u64,
    ) -> Result<bool, String> {
        let Some(record) = self.state.records.get(work_id) else {
            return Ok(false);
        };
        assert_current(record, account_fence, account_epoch)?;
        if record.state != RemoteBackgroundWorkState::Settled {
            return Err("only settled remote background work can be removed".into());
        }
        self.state.records.remove(work_id);
        self.persist()?;
        Ok(true)
    }

    pub fn fence_account_switch(
        &mut self,
        account_fence: &str,
        account_epoch: u64,
        now_ms: u64,
    ) -> Result<usize, String> {
        let mut changed = 0;
        for record in self.state.records.values_mut() {
            if record.account_fence == account_fence
                && record.account_epoch == account_epoch
                && record.state == RemoteBackgroundWorkState::Watching
            {
                record.interrupted_by_recreate = true;
                record.updated_at_ms = now_ms;
                changed += 1;
            }
        }
        if changed > 0 {
            self.persist()?;
        }
        Ok(changed)
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(&self.state).map_err(|error| error.to_string())?;
        let tmp = self.path.with_extension("tmp");
        let mut file = fs::File::create(&tmp).map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&tmp, &self.path).map_err(|error| error.to_string())
    }
}

fn assert_current(
    record: &RemoteBackgroundWorkRecord,
    account_fence: &str,
    account_epoch: u64,
) -> Result<(), String> {
    if record.account_fence != account_fence || record.account_epoch != account_epoch {
        return Err("stale remote background work from old account epoch".into());
    }
    Ok(())
}

fn validate_identity(label: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err(format!("{label} identity is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote_execution::{RemoteExecutionRecord, RemoteExecutionState};

    fn remote(operation_id: &str) -> RemoteExecutionRecord {
        RemoteExecutionRecord {
            operation_id: operation_id.into(),
            request_id: format!("request-{operation_id}"),
            capability_id: "background-shell".into(),
            account_fence: "account-a".into(),
            account_epoch: 7,
            permission_grant_id: "grant-1".into(),
            device_id: "device-1".into(),
            state: RemoteExecutionState::Acked,
            created_at_ms: 1,
            updated_at_ms: 2,
            remote_ack_id: Some("ack-1".into()),
            terminal_output_json: None,
        }
    }

    #[test]
    fn reopen_retains_original_identity_and_marks_interrupted_watch() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("remote-background.json");
        {
            let mut owner = DurableRemoteBackgroundWorkOwner::open(&path, 1).unwrap();
            owner
                .register(
                    "agent-1",
                    "work-1",
                    "Background command",
                    RemoteBackgroundWorkKind::Shell,
                    &remote("op-1"),
                    2,
                )
                .unwrap();
            owner.mark_watching("work-1", "account-a", 7, 3).unwrap();
        }
        let owner = DurableRemoteBackgroundWorkOwner::open(&path, 99).unwrap();
        let record = owner.get("work-1").unwrap();
        assert_eq!(record.operation_id, "op-1");
        assert_eq!(record.request_id, "request-op-1");
        assert_eq!(record.permission_grant_id, "grant-1");
        assert!(record.interrupted_by_recreate);
        assert_eq!(record.state, RemoteBackgroundWorkState::Watching);
    }

    #[test]
    fn duplicate_work_id_cannot_change_remote_or_account_identity() {
        let root = tempfile::tempdir().unwrap();
        let mut owner =
            DurableRemoteBackgroundWorkOwner::open(root.path().join("state.json"), 1).unwrap();
        owner
            .register(
                "agent-1",
                "same",
                "one",
                RemoteBackgroundWorkKind::Shell,
                &remote("op-1"),
                2,
            )
            .unwrap();
        assert!(owner
            .register(
                "agent-1",
                "same",
                "two",
                RemoteBackgroundWorkKind::Shell,
                &remote("op-2"),
                3,
            )
            .is_err());
    }

    #[test]
    fn settlement_and_removal_are_account_fenced() {
        let root = tempfile::tempdir().unwrap();
        let mut owner =
            DurableRemoteBackgroundWorkOwner::open(root.path().join("state.json"), 1).unwrap();
        owner
            .register(
                "agent-1",
                "work-1",
                "shell",
                RemoteBackgroundWorkKind::Shell,
                &remote("op-1"),
                2,
            )
            .unwrap();
        owner.mark_watching("work-1", "account-a", 7, 3).unwrap();
        assert!(owner
            .settle("work-1", "account-b", 7, "success", None, None, 4)
            .is_err());
        owner
            .settle(
                "work-1",
                "account-a",
                7,
                "success",
                Some("done".into()),
                Some("/terminals/work-1.txt".into()),
                5,
            )
            .unwrap();
        assert_eq!(owner.settled_for_account("account-a", 7).len(), 1);
        assert!(owner.remove_settled("work-1", "account-b", 7).is_err());
        assert!(owner.remove_settled("work-1", "account-a", 7).unwrap());
    }
}
