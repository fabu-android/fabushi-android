use crate::host_runner_composition::AuthenticatedRemoteHostRunner;
use crate::remote_execution::RemoteExecutionState;
use fabushi_android_box_exec_daemon::{RemoteExecutionContext, RemoteExecutionTransport};
use fabushi_android_shared::{ExecutionError, ExecutionRequest, ExecutionResult};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

pub const REMOTE_BOX_STORE_SYNC_CAPABILITY: &str = "box.store.sync";
pub const REMOTE_BOX_STORE_SYNC_TIMEOUT_MS: u64 = 120_000;
const MAX_IDENTITY_BYTES: usize = 512;
const MAX_STORE_ID_BYTES: usize = 512;
const MAX_ETAG_BYTES: usize = 2_048;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteBoxStoreSyncAction {
    Hydrate,
    Snapshot,
    ChromeSessionStage,
    Vacuum,
}

impl RemoteBoxStoreSyncAction {
    fn wire_name(self) -> &'static str {
        match self {
            Self::Hydrate => "hydrate",
            Self::Snapshot => "snapshot",
            Self::ChromeSessionStage => "chrome_session_stage",
            Self::Vacuum => "vacuum",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteBoxStoreSyncState {
    Prepared,
    Sent,
    Completed,
    Rejected,
    Cancelled,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBoxStoreSyncRecord {
    pub operation_id: String,
    pub request_id: String,
    pub account_fence: String,
    pub account_epoch: u64,
    pub device_id: String,
    pub store_id: String,
    pub action: RemoteBoxStoreSyncAction,
    pub base_manifest_etag: Option<String>,
    pub state: RemoteBoxStoreSyncState,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub terminal_output_json: Option<String>,
}

impl RemoteBoxStoreSyncRecord {
    pub fn prepared(
        operation_id: impl Into<String>,
        request_id: impl Into<String>,
        account_fence: impl Into<String>,
        account_epoch: u64,
        device_id: impl Into<String>,
        store_id: impl Into<String>,
        action: RemoteBoxStoreSyncAction,
        base_manifest_etag: Option<String>,
        now_ms: u64,
    ) -> Result<Self, String> {
        let record = Self {
            operation_id: operation_id.into(),
            request_id: request_id.into(),
            account_fence: account_fence.into(),
            account_epoch,
            device_id: device_id.into(),
            store_id: store_id.into(),
            action,
            base_manifest_etag,
            state: RemoteBoxStoreSyncState::Prepared,
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
        validate_bounded("store id", &self.store_id, MAX_STORE_ID_BYTES)?;
        if self.account_epoch == 0 {
            return Err("box-store sync account epoch must be positive".into());
        }
        if let Some(etag) = self.base_manifest_etag.as_deref() {
            validate_bounded("manifest etag", etag, MAX_ETAG_BYTES)?;
        }
        Ok(())
    }

    pub fn execution_request(&self) -> Result<ExecutionRequest, String> {
        if self.state != RemoteBoxStoreSyncState::Prepared {
            return Err("box-store sync can dispatch only a prepared operation".into());
        }
        let input_json = serde_json::json!({
            "version": 1,
            "action": self.action.wire_name(),
            "storeId": self.store_id,
            "baseManifestEtag": self.base_manifest_etag,
        })
        .to_string();
        let request = ExecutionRequest {
            operation_id: self.operation_id.clone(),
            capability_id: REMOTE_BOX_STORE_SYNC_CAPABILITY.into(),
            input_json,
            timeout_ms: REMOTE_BOX_STORE_SYNC_TIMEOUT_MS,
        };
        request
            .validate()
            .map_err(|error| format!("invalid box-store sync execution request: {error:?}"))?;
        Ok(request)
    }
}

#[derive(Default, Deserialize, Serialize)]
struct DurableRemoteBoxStoreSyncState {
    operations: BTreeMap<String, RemoteBoxStoreSyncRecord>,
}

pub struct RemoteBoxStoreSyncJournal {
    path: PathBuf,
    state: DurableRemoteBoxStoreSyncState,
}

impl RemoteBoxStoreSyncJournal {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let path = path.into();
        let mut state = if path.exists() {
            serde_json::from_slice::<DurableRemoteBoxStoreSyncState>(
                &fs::read(&path).map_err(|error| format!("read box-store sync journal: {error}"))?,
            )
            .map_err(|error| format!("invalid box-store sync journal: {error}"))?
        } else {
            DurableRemoteBoxStoreSyncState::default()
        };
        let mut changed = false;
        for record in state.operations.values_mut() {
            let recovered = match record.state {
                RemoteBoxStoreSyncState::Prepared => Some(RemoteBoxStoreSyncState::Cancelled),
                RemoteBoxStoreSyncState::Sent => Some(RemoteBoxStoreSyncState::OutcomeUnknown),
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

    pub fn begin(&mut self, record: RemoteBoxStoreSyncRecord) -> Result<(), String> {
        record.validate()?;
        if record.state != RemoteBoxStoreSyncState::Prepared {
            return Err("new box-store sync operation must be prepared".into());
        }
        if self.state.operations.contains_key(&record.operation_id) {
            return Err("duplicate box-store sync operation id".into());
        }
        if self
            .state
            .operations
            .values()
            .any(|existing| existing.request_id == record.request_id)
        {
            return Err("duplicate box-store sync request id".into());
        }
        self.state.operations.insert(record.operation_id.clone(), record);
        self.persist()
    }

    pub fn record(&self, operation_id: &str) -> Option<&RemoteBoxStoreSyncRecord> {
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
        if record.state != RemoteBoxStoreSyncState::Prepared {
            return Err("box-store sync dispatch requires prepared state".into());
        }
        record.state = RemoteBoxStoreSyncState::Sent;
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
        if record.state != RemoteBoxStoreSyncState::Sent {
            return Err("only a sent box-store sync can become outcome-unknown".into());
        }
        record.state = RemoteBoxStoreSyncState::OutcomeUnknown;
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
            .map_err(|_| "box-store sync completion output must be JSON")?;
        let record = self.checked_mut(operation_id, account_fence, account_epoch, device_id)?;
        if record.request_id != request_id {
            return Err("stale or mismatched box-store sync completion".into());
        }
        if !matches!(
            record.state,
            RemoteBoxStoreSyncState::Sent | RemoteBoxStoreSyncState::OutcomeUnknown
        ) {
            return Err("box-store sync completion arrived from invalid state".into());
        }
        record.state = RemoteBoxStoreSyncState::Completed;
        record.terminal_output_json = Some(output_json);
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn cancel_before_dispatch(
        &mut self,
        operation_id: &str,
        account_fence: &str,
        account_epoch: u64,
        device_id: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        let record = self.checked_mut(operation_id, account_fence, account_epoch, device_id)?;
        if record.state != RemoteBoxStoreSyncState::Prepared {
            return Err("only prepared box-store sync can cancel without reconciliation".into());
        }
        record.state = RemoteBoxStoreSyncState::Cancelled;
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn reconciliation_required(
        &self,
        account_fence: &str,
        account_epoch: u64,
        device_id: &str,
    ) -> Vec<&RemoteBoxStoreSyncRecord> {
        self.state
            .operations
            .values()
            .filter(|record| {
                record.state == RemoteBoxStoreSyncState::OutcomeUnknown
                    && record.account_fence == account_fence
                    && record.account_epoch == account_epoch
                    && record.device_id == device_id
            })
            .collect()
    }

    pub fn dispatch_via_authorized_runner<T: RemoteExecutionTransport>(
        &mut self,
        runner: &mut AuthenticatedRemoteHostRunner<T>,
        context: &RemoteExecutionContext,
        record: RemoteBoxStoreSyncRecord,
        now_ms: u64,
    ) -> Result<ExecutionResult, ExecutionError> {
        record.validate().map_err(ExecutionError::InvalidRequest)?;
        context.validate()?;
        if context.operation_id != record.operation_id
            || context.request_id != record.request_id
            || context.account_fence != record.account_fence
            || context.account_epoch != record.account_epoch
            || context.device_id != record.device_id
        {
            return Err(ExecutionError::InvalidRequest(
                "box-store sync authorization identity disagrees with durable record".into(),
            ));
        }
        if runner.journal().record(&record.operation_id).is_some() {
            return Err(ExecutionError::InvalidRequest(
                "box-store sync operation already exists in Remote execution authority".into(),
            ));
        }
        let request = record.execution_request().map_err(ExecutionError::InvalidRequest)?;
        self.begin(record.clone()).map_err(ExecutionError::Transport)?;
        if let Err(error) = runner.prepare_authorized(context, &request, now_ms) {
            let _ = self.cancel_before_dispatch(
                &record.operation_id, &record.account_fence, record.account_epoch, &record.device_id, now_ms,
            );
            return Err(error);
        }
        if let Err(error) = self.mark_sent(
            &record.operation_id, &record.account_fence, record.account_epoch, &record.device_id, now_ms,
        ) {
            let _ = runner.cancel_prepared_authorized(context, now_ms);
            let _ = self.cancel_before_dispatch(
                &record.operation_id, &record.account_fence, record.account_epoch, &record.device_id, now_ms,
            );
            return Err(ExecutionError::Transport(error));
        }
        match runner.dispatch_prepared_authorized(context, &request, now_ms) {
            Ok(result) => {
                self.complete(
                    &record.operation_id, &record.request_id, &record.account_fence,
                    record.account_epoch, &record.device_id, result.output_json.clone(), now_ms,
                ).map_err(ExecutionError::Transport)?;
                Ok(result)
            }
            Err(error) => {
                match runner.journal().record(&record.operation_id).map(|entry| entry.state.clone()) {
                    Some(RemoteExecutionState::Rejected) => {
                        self.settle_after_dispatch(
                            &record.operation_id, &record.account_fence, record.account_epoch,
                            &record.device_id, RemoteBoxStoreSyncState::Rejected, now_ms,
                        ).map_err(ExecutionError::Transport)?;
                    }
                    Some(RemoteExecutionState::Pending) | Some(RemoteExecutionState::Cancelled) => {
                        let _ = runner.cancel_prepared_authorized(context, now_ms);
                        self.settle_after_dispatch(
                            &record.operation_id, &record.account_fence, record.account_epoch,
                            &record.device_id, RemoteBoxStoreSyncState::Cancelled, now_ms,
                        ).map_err(ExecutionError::Transport)?;
                    }
                    _ => {
                        self.mark_outcome_unknown(
                            &record.operation_id, &record.account_fence, record.account_epoch,
                            &record.device_id, now_ms,
                        ).map_err(ExecutionError::Transport)?;
                    }
                }
                Err(error)
            }
        }
    }

    pub fn reconcile_via_authorized_runner<T: RemoteExecutionTransport>(
        &mut self,
        runner: &mut AuthenticatedRemoteHostRunner<T>,
        context: &RemoteExecutionContext,
        now_ms: u64,
    ) -> Result<ExecutionResult, ExecutionError> {
        context.validate()?;
        let record = self.record(&context.operation_id).cloned()
            .ok_or_else(|| ExecutionError::InvalidRequest("box-store sync operation is unknown".into()))?;
        if record.state != RemoteBoxStoreSyncState::OutcomeUnknown {
            return Err(ExecutionError::InvalidRequest(
                "box-store sync reconciliation requires outcome-unknown state".into(),
            ));
        }
        if context.request_id != record.request_id
            || context.account_fence != record.account_fence
            || context.account_epoch != record.account_epoch
            || context.device_id != record.device_id
        {
            return Err(ExecutionError::InvalidRequest(
                "box-store sync reconciliation crossed account/device identity".into(),
            ));
        }
        let remote = runner.journal().record(&record.operation_id).cloned()
            .ok_or_else(|| ExecutionError::Transport(
                "box-store sync Remote execution authority is missing; manual reconciliation required".into(),
            ))?;
        match remote.state {
            RemoteExecutionState::Completed => {
                let output = remote.terminal_output_json.unwrap_or_else(|| "{}".into());
                self.complete(
                    &record.operation_id, &record.request_id, &record.account_fence,
                    record.account_epoch, &record.device_id, output.clone(), now_ms,
                ).map_err(ExecutionError::Transport)?;
                Ok(ExecutionResult { operation_id: record.operation_id, output_json: output })
            }
            RemoteExecutionState::Rejected => {
                self.settle_after_dispatch(
                    &record.operation_id, &record.account_fence, record.account_epoch,
                    &record.device_id, RemoteBoxStoreSyncState::Rejected, now_ms,
                ).map_err(ExecutionError::Transport)?;
                Err(ExecutionError::Transport("remote_execution_rejected".into()))
            }
            RemoteExecutionState::Pending | RemoteExecutionState::Cancelled => {
                if remote.state == RemoteExecutionState::Pending {
                    runner.cancel_prepared_authorized(context, now_ms)?;
                }
                self.settle_after_dispatch(
                    &record.operation_id, &record.account_fence, record.account_epoch,
                    &record.device_id, RemoteBoxStoreSyncState::Cancelled, now_ms,
                ).map_err(ExecutionError::Transport)?;
                Err(ExecutionError::Transport("box_store_sync_proven_not_sent_cancelled".into()))
            }
            RemoteExecutionState::OutcomeUnknown => {
                match runner.reconcile_authorized(context, now_ms) {
                    Ok(result) => {
                        self.complete(
                            &record.operation_id, &record.request_id, &record.account_fence,
                            record.account_epoch, &record.device_id, result.output_json.clone(), now_ms,
                        ).map_err(ExecutionError::Transport)?;
                        Ok(result)
                    }
                    Err(error) => {
                        if runner.journal().record(&record.operation_id)
                            .is_some_and(|entry| entry.state == RemoteExecutionState::Rejected)
                        {
                            self.settle_after_dispatch(
                                &record.operation_id, &record.account_fence, record.account_epoch,
                                &record.device_id, RemoteBoxStoreSyncState::Rejected, now_ms,
                            ).map_err(ExecutionError::Transport)?;
                        }
                        Err(error)
                    }
                }
            }
            RemoteExecutionState::Sent | RemoteExecutionState::Acked => Err(
                ExecutionError::Transport("box_store_sync_remote_reconciliation_not_ready".into()),
            ),
        }
    }

    fn settle_after_dispatch(
        &mut self,
        operation_id: &str,
        account_fence: &str,
        account_epoch: u64,
        device_id: &str,
        state: RemoteBoxStoreSyncState,
        now_ms: u64,
    ) -> Result<(), String> {
        if !matches!(state, RemoteBoxStoreSyncState::Rejected | RemoteBoxStoreSyncState::Cancelled) {
            return Err("box-store sync terminal settlement is invalid".into());
        }
        let record = self.checked_mut(operation_id, account_fence, account_epoch, device_id)?;
        if !matches!(
            record.state,
            RemoteBoxStoreSyncState::Sent | RemoteBoxStoreSyncState::OutcomeUnknown
        ) {
            return Err("box-store sync terminal settlement requires sent or outcome-unknown state".into());
        }
        record.state = state;
        record.updated_at_ms = now_ms;
        self.persist()
    }

    fn checked_mut(
        &mut self,
        operation_id: &str,
        account_fence: &str,
        account_epoch: u64,
        device_id: &str,
    ) -> Result<&mut RemoteBoxStoreSyncRecord, String> {
        let record = self
            .state
            .operations
            .get_mut(operation_id)
            .ok_or("box-store sync operation is unknown")?;
        if record.account_fence != account_fence
            || record.account_epoch != account_epoch
            || record.device_id != device_id
        {
            return Err("stale box-store sync callback crossed account/device epoch".into());
        }
        Ok(record)
    }

    fn persist(&self) -> Result<(), String> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|error| format!("create box-store sync journal parent: {error}"))?;
        let temp = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        let result = (|| -> Result<(), String> {
            let bytes = serde_json::to_vec(&self.state)
                .map_err(|error| format!("encode box-store sync journal: {error}"))?;
            let mut file = File::create(&temp)
                .map_err(|error| format!("create box-store sync journal temp: {error}"))?;
            file.write_all(&bytes)
                .map_err(|error| format!("write box-store sync journal temp: {error}"))?;
            file.sync_all()
                .map_err(|error| format!("sync box-store sync journal temp: {error}"))?;
            fs::rename(&temp, &self.path)
                .map_err(|error| format!("replace box-store sync journal: {error}"))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}

fn validate_identity(label: &str, value: &str) -> Result<(), String> {
    validate_bounded(label, value, MAX_IDENTITY_BYTES)
}

fn validate_bounded(label: &str, value: &str, max: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(format!("box-store sync {label} is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-remote-box-store-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn record(action: RemoteBoxStoreSyncAction, operation: &str, request: &str) -> RemoteBoxStoreSyncRecord {
        RemoteBoxStoreSyncRecord::prepared(
            operation,
            request,
            "account-a:epoch-7",
            7,
            "device-a",
            "store-a",
            action,
            Some("etag-1".into()),
            1,
        )
        .unwrap()
    }

    #[test]
    fn execution_request_uses_dedicated_bounded_capability_not_shell() {
        let request = record(RemoteBoxStoreSyncAction::Snapshot, "op-1", "req-1")
            .execution_request()
            .unwrap();
        assert_eq!(request.capability_id, REMOTE_BOX_STORE_SYNC_CAPABILITY);
        assert_eq!(request.timeout_ms, REMOTE_BOX_STORE_SYNC_TIMEOUT_MS);
        assert!(request.input_json.contains("snapshot"));
        assert!(!request.input_json.contains("Shell"));
    }

    #[test]
    fn process_reopen_never_blind_replays_prepared_or_sent_work() {
        let journal_path = path("reopen");
        {
            let mut journal = RemoteBoxStoreSyncJournal::open(&journal_path, 1).unwrap();
            journal.begin(record(RemoteBoxStoreSyncAction::Hydrate, "prepared", "req-1")).unwrap();
            journal.begin(record(RemoteBoxStoreSyncAction::Snapshot, "sent", "req-2")).unwrap();
            journal.mark_sent("sent", "account-a:epoch-7", 7, "device-a", 2).unwrap();
        }
        let reopened = RemoteBoxStoreSyncJournal::open(&journal_path, 9).unwrap();
        assert_eq!(reopened.record("prepared").unwrap().state, RemoteBoxStoreSyncState::Cancelled);
        assert_eq!(reopened.record("sent").unwrap().state, RemoteBoxStoreSyncState::OutcomeUnknown);
        assert_eq!(reopened.reconciliation_required("account-a:epoch-7", 7, "device-a").len(), 1);
        let _ = fs::remove_file(journal_path);
    }

    #[test]
    fn stale_account_or_device_completion_fails_closed() {
        let journal_path = path("fence");
        let mut journal = RemoteBoxStoreSyncJournal::open(&journal_path, 1).unwrap();
        journal.begin(record(RemoteBoxStoreSyncAction::Snapshot, "op-1", "req-1")).unwrap();
        journal.mark_sent("op-1", "account-a:epoch-7", 7, "device-a", 2).unwrap();
        assert!(journal.complete("op-1", "req-1", "account-b:epoch-8", 8, "device-a", "{}".into(), 3).is_err());
        assert!(journal.complete("op-1", "req-1", "account-a:epoch-7", 7, "device-b", "{}".into(), 3).is_err());
        journal.mark_outcome_unknown("op-1", "account-a:epoch-7", 7, "device-a", 3).unwrap();
        journal.complete("op-1", "req-1", "account-a:epoch-7", 7, "device-a", r#"{"manifestEtag":"etag-2"}"#.into(), 4).unwrap();
        assert_eq!(journal.record("op-1").unwrap().state, RemoteBoxStoreSyncState::Completed);
        let _ = fs::remove_file(journal_path);
    }

    #[test]
    fn duplicate_operation_or_request_identity_is_rejected() {
        let journal_path = path("dedupe");
        let mut journal = RemoteBoxStoreSyncJournal::open(&journal_path, 1).unwrap();
        journal.begin(record(RemoteBoxStoreSyncAction::Hydrate, "op-1", "req-1")).unwrap();
        assert!(journal.begin(record(RemoteBoxStoreSyncAction::Snapshot, "op-1", "req-2")).is_err());
        assert!(journal.begin(record(RemoteBoxStoreSyncAction::Snapshot, "op-2", "req-1")).is_err());
        let _ = fs::remove_file(journal_path);
    }

    #[derive(Default)]
    struct RecordingTransport {
        outcome: Option<fabushi_android_box_exec_daemon::RemoteDispatchOutcome>,
    }

    impl RemoteExecutionTransport for RecordingTransport {
        fn execute(
            &mut self,
            _context: &RemoteExecutionContext,
            request: &ExecutionRequest,
        ) -> Result<fabushi_android_box_exec_daemon::RemoteDispatchOutcome, ExecutionError> {
            Ok(self.outcome.take().unwrap_or_else(|| {
                fabushi_android_box_exec_daemon::RemoteDispatchOutcome::Completed {
                    ack_id: "ack-box-store".into(),
                    result: ExecutionResult {
                        operation_id: request.operation_id.clone(),
                        output_json: r#"{"manifestEtag":"etag-2"}"#.into(),
                    },
                }
            }))
        }

        fn reconcile(
            &mut self,
            _context: &RemoteExecutionContext,
            operation_id: &str,
            _request_id: &str,
        ) -> Result<fabushi_android_box_exec_daemon::RemoteReconcileOutcome, ExecutionError> {
            Ok(fabushi_android_box_exec_daemon::RemoteReconcileOutcome::Completed {
                ack_id: "ack-reconcile".into(),
                result: ExecutionResult {
                    operation_id: operation_id.into(),
                    output_json: r#"{"manifestEtag":"etag-reconciled"}"#.into(),
                },
            })
        }

        fn cancel(
            &mut self,
            _context: &RemoteExecutionContext,
            _operation_id: &str,
            _request_id: &str,
        ) -> Result<fabushi_android_box_exec_daemon::RemoteCancelOutcome, ExecutionError> {
            Ok(fabushi_android_box_exec_daemon::RemoteCancelOutcome::Confirmed {
                ack_id: Some("ack-cancel".into()),
            })
        }
    }

    fn remote_context(operation_id: &str, request_id: &str) -> RemoteExecutionContext {
        RemoteExecutionContext {
            credential_id: "credential-1".into(),
            bearer: fabushi_android_box_exec_daemon::RemoteBearerCredential::new(
                "box-store-test-bearer-token-long-enough",
            ).unwrap(),
            account_fence: "account-a:epoch-7".into(),
            account_epoch: 7,
            operation_id: operation_id.into(),
            request_id: request_id.into(),
            permission_grant_id: "grant-box-store".into(),
            device_id: "device-a".into(),
        }
    }

    #[test]
    fn authorized_runner_settles_box_store_domain_without_second_runner() {
        let root = tempfile::tempdir().unwrap();
        let mut domain = RemoteBoxStoreSyncJournal::open(root.path().join("box-store.json"), 1).unwrap();
        let remote_journal = crate::remote_execution::RemoteExecutionJournal::open(
            root.path().join("remote.json"), 1
        ).unwrap();
        let mut runner = AuthenticatedRemoteHostRunner::new(
            RecordingTransport::default(), remote_journal
        );
        let context = remote_context("box-op-1", "box-req-1");
        let result = domain.dispatch_via_authorized_runner(
            &mut runner,
            &context,
            record(RemoteBoxStoreSyncAction::Snapshot, "box-op-1", "box-req-1"),
            2,
        ).unwrap();
        assert!(result.output_json.contains("etag-2"));
        assert_eq!(domain.record("box-op-1").unwrap().state, RemoteBoxStoreSyncState::Completed);
        assert_eq!(runner.journal().record("box-op-1").unwrap().state, RemoteExecutionState::Completed);
    }

    #[test]
    fn uncertain_box_store_dispatch_requires_reconciliation_and_never_replays() {
        let root = tempfile::tempdir().unwrap();
        let mut domain = RemoteBoxStoreSyncJournal::open(root.path().join("box-store.json"), 1).unwrap();
        let remote_journal = crate::remote_execution::RemoteExecutionJournal::open(
            root.path().join("remote.json"), 1
        ).unwrap();
        let mut runner = AuthenticatedRemoteHostRunner::new(
            RecordingTransport {
                outcome: Some(fabushi_android_box_exec_daemon::RemoteDispatchOutcome::OutcomeUnknown {
                    ack_id: Some("ack-unknown".into()),
                    reason: "connection-lost".into(),
                }),
            },
            remote_journal,
        );
        let context = remote_context("box-op-unknown", "box-req-unknown");
        assert!(domain.dispatch_via_authorized_runner(
            &mut runner,
            &context,
            record(RemoteBoxStoreSyncAction::Snapshot, "box-op-unknown", "box-req-unknown"),
            2,
        ).is_err());
        assert_eq!(
            domain.record("box-op-unknown").unwrap().state,
            RemoteBoxStoreSyncState::OutcomeUnknown
        );
        let result = domain.reconcile_via_authorized_runner(&mut runner, &context, 3).unwrap();
        assert!(result.output_json.contains("etag-reconciled"));
        assert_eq!(
            domain.record("box-op-unknown").unwrap().state,
            RemoteBoxStoreSyncState::Completed
        );
    }

}
