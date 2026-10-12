use fabushi_android_shared::ExecutionRequest;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

pub const REMOTE_CODEBASE_SNAPSHOT_CAPABILITY: &str = "codebase.telemetry.snapshot";
pub const REMOTE_CODEBASE_SNAPSHOT_TIMEOUT_MS: u64 = 120_000;
const MAX_IDENTITY_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteCodebaseSnapshotReason {
    AgentRequestStart,
    AgentRequestEnd,
}

impl RemoteCodebaseSnapshotReason {
    fn wire_name(self) -> &'static str {
        match self {
            Self::AgentRequestStart => "agent_request_start",
            Self::AgentRequestEnd => "agent_request_end",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteCodebaseSnapshotState {
    Prepared,
    Sent,
    Completed,
    Rejected,
    Cancelled,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCodebaseSnapshotRecord {
    pub operation_id: String,
    pub request_id: String,
    pub account_fence: String,
    pub account_epoch: u64,
    pub device_id: String,
    pub reason: RemoteCodebaseSnapshotReason,
    pub state: RemoteCodebaseSnapshotState,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub terminal_output_json: Option<String>,
}

impl RemoteCodebaseSnapshotRecord {
    pub fn prepared(
        operation_id: impl Into<String>,
        request_id: impl Into<String>,
        account_fence: impl Into<String>,
        account_epoch: u64,
        device_id: impl Into<String>,
        reason: RemoteCodebaseSnapshotReason,
        now_ms: u64,
    ) -> Result<Self, String> {
        let record = Self {
            operation_id: operation_id.into(),
            request_id: request_id.into(),
            account_fence: account_fence.into(),
            account_epoch,
            device_id: device_id.into(),
            reason,
            state: RemoteCodebaseSnapshotState::Prepared,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            terminal_output_json: None,
        };
        record.validate()?;
        Ok(record)
    }

    fn validate(&self) -> Result<(), String> {
        validate_identity("operation", &self.operation_id)?;
        validate_identity("request", &self.request_id)?;
        validate_identity("account fence", &self.account_fence)?;
        validate_identity("device", &self.device_id)?;
        if self.account_epoch == 0 {
            return Err("codebase snapshot account epoch must be positive".into());
        }
        Ok(())
    }

    pub fn execution_request(&self) -> Result<ExecutionRequest, String> {
        if self.state != RemoteCodebaseSnapshotState::Prepared {
            return Err("codebase snapshot can dispatch only a prepared operation".into());
        }
        let request = ExecutionRequest {
            operation_id: self.operation_id.clone(),
            capability_id: REMOTE_CODEBASE_SNAPSHOT_CAPABILITY.into(),
            input_json: serde_json::json!({
                "version": 1,
                "reason": self.reason.wire_name(),
                "agentRequestId": self.request_id,
            })
            .to_string(),
            timeout_ms: REMOTE_CODEBASE_SNAPSHOT_TIMEOUT_MS,
        };
        request
            .validate()
            .map_err(|error| format!("invalid codebase snapshot execution request: {error:?}"))?;
        Ok(request)
    }
}

#[derive(Default, Deserialize, Serialize)]
struct DurableRemoteCodebaseSnapshotState {
    operations: BTreeMap<String, RemoteCodebaseSnapshotRecord>,
}

pub struct RemoteCodebaseSnapshotJournal {
    path: PathBuf,
    state: DurableRemoteCodebaseSnapshotState,
}

impl RemoteCodebaseSnapshotJournal {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let path = path.into();
        let mut state = if path.exists() {
            serde_json::from_slice::<DurableRemoteCodebaseSnapshotState>(
                &fs::read(&path).map_err(|error| format!("read codebase snapshot journal: {error}"))?,
            )
            .map_err(|error| format!("invalid codebase snapshot journal: {error}"))?
        } else {
            DurableRemoteCodebaseSnapshotState::default()
        };
        let mut changed = false;
        for record in state.operations.values_mut() {
            let recovered = match record.state {
                RemoteCodebaseSnapshotState::Prepared => Some(RemoteCodebaseSnapshotState::Cancelled),
                RemoteCodebaseSnapshotState::Sent => Some(RemoteCodebaseSnapshotState::OutcomeUnknown),
                _ => None,
            };
            if let Some(recovered) = recovered {
                record.state = recovered;
                record.updated_at_ms = now_ms;
                changed = true;
            }
        }
        let journal = Self { path, state };
        if changed {
            journal.persist()?;
        }
        Ok(journal)
    }

    pub fn begin(&mut self, record: RemoteCodebaseSnapshotRecord) -> Result<(), String> {
        record.validate()?;
        if record.state != RemoteCodebaseSnapshotState::Prepared {
            return Err("new codebase snapshot must be prepared".into());
        }
        if self.state.operations.contains_key(&record.operation_id) {
            return Err("duplicate codebase snapshot operation id".into());
        }
        if self.state.operations.values().any(|existing| {
            existing.request_id == record.request_id && existing.reason == record.reason
        }) {
            return Err("duplicate codebase snapshot request/reason".into());
        }
        self.state.operations.insert(record.operation_id.clone(), record);
        self.persist()
    }

    pub fn record(&self, operation_id: &str) -> Option<&RemoteCodebaseSnapshotRecord> {
        self.state.operations.get(operation_id)
    }

    pub fn mark_sent(
        &mut self,
        operation_id: &str,
        account_fence: &str,
        account_epoch: u64,
        device_id: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        let record = self.checked_mut(operation_id, account_fence, account_epoch, device_id)?;
        if record.state != RemoteCodebaseSnapshotState::Prepared {
            return Err("codebase snapshot dispatch requires prepared state".into());
        }
        record.state = RemoteCodebaseSnapshotState::Sent;
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn mark_outcome_unknown(
        &mut self,
        operation_id: &str,
        account_fence: &str,
        account_epoch: u64,
        device_id: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        let record = self.checked_mut(operation_id, account_fence, account_epoch, device_id)?;
        if record.state != RemoteCodebaseSnapshotState::Sent {
            return Err("only a sent codebase snapshot can become outcome-unknown".into());
        }
        record.state = RemoteCodebaseSnapshotState::OutcomeUnknown;
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn complete(
        &mut self,
        operation_id: &str,
        request_id: &str,
        account_fence: &str,
        account_epoch: u64,
        device_id: &str,
        output_json: String,
        now_ms: u64,
    ) -> Result<(), String> {
        serde_json::from_str::<serde_json::Value>(&output_json)
            .map_err(|_| "codebase snapshot completion output must be JSON")?;
        let record = self.checked_mut(operation_id, account_fence, account_epoch, device_id)?;
        if record.request_id != request_id {
            return Err("stale or mismatched codebase snapshot completion".into());
        }
        if !matches!(
            record.state,
            RemoteCodebaseSnapshotState::Sent | RemoteCodebaseSnapshotState::OutcomeUnknown
        ) {
            return Err("codebase snapshot completion arrived from invalid state".into());
        }
        record.state = RemoteCodebaseSnapshotState::Completed;
        record.terminal_output_json = Some(output_json);
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn reconciliation_required(
        &self,
        account_fence: &str,
        account_epoch: u64,
        device_id: &str,
    ) -> Vec<&RemoteCodebaseSnapshotRecord> {
        self.state.operations.values().filter(|record| {
            record.state == RemoteCodebaseSnapshotState::OutcomeUnknown
                && record.account_fence == account_fence
                && record.account_epoch == account_epoch
                && record.device_id == device_id
        }).collect()
    }

    fn checked_mut(
        &mut self,
        operation_id: &str,
        account_fence: &str,
        account_epoch: u64,
        device_id: &str,
    ) -> Result<&mut RemoteCodebaseSnapshotRecord, String> {
        let record = self.state.operations.get_mut(operation_id)
            .ok_or("codebase snapshot operation is unknown")?;
        if record.account_fence != account_fence
            || record.account_epoch != account_epoch
            || record.device_id != device_id
        {
            return Err("stale codebase snapshot callback crossed account/device epoch".into());
        }
        Ok(record)
    }

    fn persist(&self) -> Result<(), String> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|error| format!("create codebase snapshot journal parent: {error}"))?;
        let temp = self.path.with_extension(format!("tmp-{}", std::process::id()));
        let result = (|| -> Result<(), String> {
            let bytes = serde_json::to_vec(&self.state)
                .map_err(|error| format!("encode codebase snapshot journal: {error}"))?;
            let mut file = File::create(&temp)
                .map_err(|error| format!("create codebase snapshot journal temp: {error}"))?;
            file.write_all(&bytes)
                .map_err(|error| format!("write codebase snapshot journal temp: {error}"))?;
            file.sync_all()
                .map_err(|error| format!("sync codebase snapshot journal temp: {error}"))?;
            fs::rename(&temp, &self.path)
                .map_err(|error| format!("replace codebase snapshot journal: {error}"))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}

fn validate_identity(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.len() > MAX_IDENTITY_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(format!("codebase snapshot {label} identity is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-codebase-snapshot-{label}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn record(reason: RemoteCodebaseSnapshotReason, operation: &str, request: &str) -> RemoteCodebaseSnapshotRecord {
        RemoteCodebaseSnapshotRecord::prepared(
            operation, request, "account-a:epoch-7", 7, "device-a", reason, 1,
        ).unwrap()
    }

    #[test]
    fn request_uses_dedicated_snapshot_capability_not_shell() {
        let request = record(RemoteCodebaseSnapshotReason::AgentRequestStart, "op-1", "req-1")
            .execution_request().unwrap();
        assert_eq!(request.capability_id, REMOTE_CODEBASE_SNAPSHOT_CAPABILITY);
        assert!(request.input_json.contains("agent_request_start"));
        assert!(!request.input_json.contains("Shell"));
        assert!(!request.input_json.contains("csnaps"));
    }

    #[test]
    fn start_and_end_for_same_agent_request_are_distinct_but_each_deduplicated() {
        let path = temp_path("dedupe");
        let mut journal = RemoteCodebaseSnapshotJournal::open(&path, 1).unwrap();
        journal.begin(record(RemoteCodebaseSnapshotReason::AgentRequestStart, "op-start", "req-1")).unwrap();
        journal.begin(record(RemoteCodebaseSnapshotReason::AgentRequestEnd, "op-end", "req-1")).unwrap();
        assert!(journal.begin(record(RemoteCodebaseSnapshotReason::AgentRequestStart, "op-2", "req-1")).is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn reopen_never_blind_replays_sent_snapshot() {
        let path = temp_path("reopen");
        {
            let mut journal = RemoteCodebaseSnapshotJournal::open(&path, 1).unwrap();
            journal.begin(record(RemoteCodebaseSnapshotReason::AgentRequestStart, "op-1", "req-1")).unwrap();
            journal.mark_sent("op-1", "account-a:epoch-7", 7, "device-a", 2).unwrap();
        }
        let reopened = RemoteCodebaseSnapshotJournal::open(&path, 9).unwrap();
        assert_eq!(reopened.record("op-1").unwrap().state, RemoteCodebaseSnapshotState::OutcomeUnknown);
        assert_eq!(reopened.reconciliation_required("account-a:epoch-7", 7, "device-a").len(), 1);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn stale_account_or_device_completion_fails_closed() {
        let path = temp_path("fence");
        let mut journal = RemoteCodebaseSnapshotJournal::open(&path, 1).unwrap();
        journal.begin(record(RemoteCodebaseSnapshotReason::AgentRequestEnd, "op-1", "req-1")).unwrap();
        journal.mark_sent("op-1", "account-a:epoch-7", 7, "device-a", 2).unwrap();
        assert!(journal.complete("op-1", "req-1", "account-b:epoch-8", 8, "device-a", "{}".into(), 3).is_err());
        assert!(journal.complete("op-1", "req-1", "account-a:epoch-7", 7, "device-b", "{}".into(), 3).is_err());
        journal.mark_outcome_unknown("op-1", "account-a:epoch-7", 7, "device-a", 3).unwrap();
        journal.complete("op-1", "req-1", "account-a:epoch-7", 7, "device-a", "{}".into(), 4).unwrap();
        assert_eq!(journal.record("op-1").unwrap().state, RemoteCodebaseSnapshotState::Completed);
        let _ = fs::remove_file(path);
    }
}
