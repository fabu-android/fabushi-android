use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteExecutionState {
    Pending,
    Sent,
    Acked,
    Completed,
    Rejected,
    Cancelled,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RemoteExecutionRecord {
    pub operation_id: String,
    pub request_id: String,
    pub capability_id: String,
    pub account_fence: String,
    pub account_epoch: u64,
    pub permission_grant_id: String,
    #[serde(default)]
    pub device_id: String,
    pub state: RemoteExecutionState,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub remote_ack_id: Option<String>,
    pub terminal_output_json: Option<String>,
}

#[derive(Default, Deserialize, Serialize)]
struct DurableRemoteState {
    operations: BTreeMap<String, RemoteExecutionRecord>,
}

pub struct RemoteExecutionJournal {
    path: PathBuf,
    state: DurableRemoteState,
}

impl RemoteExecutionJournal {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let path = path.into();
        let mut state = if path.exists() {
            serde_json::from_slice::<DurableRemoteState>(
                &fs::read(&path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| format!("invalid remote execution journal: {error}"))?
        } else {
            DurableRemoteState::default()
        };

        let mut changed = false;
        for record in state.operations.values_mut() {
            if matches!(
                record.state,
                RemoteExecutionState::Sent | RemoteExecutionState::Acked
            ) {
                record.state = RemoteExecutionState::OutcomeUnknown;
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

    pub fn begin(&mut self, record: RemoteExecutionRecord) -> Result<(), String> {
        validate_identity("operation", &record.operation_id)?;
        validate_identity("request", &record.request_id)?;
        validate_identity("capability", &record.capability_id)?;
        validate_identity("account fence", &record.account_fence)?;
        validate_identity("permission grant", &record.permission_grant_id)?;
        validate_identity("device", &record.device_id)?;
        if record.account_epoch == 0 {
            return Err("account epoch must be positive".into());
        }
        if record.state != RemoteExecutionState::Pending {
            return Err("new remote execution must start pending".into());
        }
        if self.state.operations.contains_key(&record.operation_id) {
            return Err("duplicate remote operation identity".into());
        }
        if self
            .state
            .operations
            .values()
            .any(|existing| existing.request_id == record.request_id)
        {
            return Err("duplicate remote request identity".into());
        }
        self.state
            .operations
            .insert(record.operation_id.clone(), record);
        self.persist()
    }

    pub fn mark_sent(
        &mut self,
        operation_id: &str,
        current_account_fence: &str,
        current_account_epoch: u64,
        permission_grant_id: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        let record = self
            .state
            .operations
            .get_mut(operation_id)
            .ok_or("remote operation is unknown")?;
        assert_current(record, current_account_fence, current_account_epoch, permission_grant_id)?;
        if record.state != RemoteExecutionState::Pending {
            return Err(format!(
                "remote operation cannot be sent from state {:?}",
                record.state
            ));
        }
        record.state = RemoteExecutionState::Sent;
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn acknowledge(
        &mut self,
        operation_id: &str,
        request_id: &str,
        current_account_fence: &str,
        current_account_epoch: u64,
        remote_ack_id: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        validate_identity("remote acknowledgement", remote_ack_id)?;
        let record = self
            .state
            .operations
            .get_mut(operation_id)
            .ok_or("remote operation is unknown")?;
        if record.request_id != request_id {
            return Err("stale or mismatched remote request callback".into());
        }
        if record.account_fence != current_account_fence
            || record.account_epoch != current_account_epoch
        {
            return Err("stale remote callback from old account epoch".into());
        }
        if record.state != RemoteExecutionState::Sent {
            return Err(format!(
                "remote acknowledgement is invalid from state {:?}",
                record.state
            ));
        }
        record.state = RemoteExecutionState::Acked;
        record.remote_ack_id = Some(remote_ack_id.into());
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn complete(
        &mut self,
        operation_id: &str,
        request_id: &str,
        current_account_fence: &str,
        current_account_epoch: u64,
        output_json: String,
        now_ms: u64,
    ) -> Result<(), String> {
        let record = self
            .state
            .operations
            .get_mut(operation_id)
            .ok_or("remote operation is unknown")?;
        if record.request_id != request_id {
            return Err("stale or mismatched remote completion callback".into());
        }
        if record.account_fence != current_account_fence
            || record.account_epoch != current_account_epoch
        {
            return Err("stale remote completion from old account epoch".into());
        }
        if !matches!(
            record.state,
            RemoteExecutionState::Sent | RemoteExecutionState::Acked
        ) {
            return Err(format!(
                "remote completion is invalid from state {:?}",
                record.state
            ));
        }
        record.state = RemoteExecutionState::Completed;
        record.terminal_output_json = Some(output_json);
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn timeout(&mut self, operation_id: &str, now_ms: u64) -> Result<(), String> {
        self.mark_outcome_unknown(operation_id, now_ms)
    }

    pub fn cancel_after_dispatch(
        &mut self,
        operation_id: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        self.mark_outcome_unknown(operation_id, now_ms)
    }

    pub fn cancel_before_dispatch(
        &mut self,
        operation_id: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        let record = self
            .state
            .operations
            .get_mut(operation_id)
            .ok_or("remote operation is unknown")?;
        if record.state != RemoteExecutionState::Pending {
            return Err("only pending remote operations can cancel before dispatch".into());
        }
        record.state = RemoteExecutionState::Cancelled;
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn reject_before_dispatch(
        &mut self,
        operation_id: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        let record = self
            .state
            .operations
            .get_mut(operation_id)
            .ok_or("remote operation is unknown")?;
        if record.state != RemoteExecutionState::Pending {
            return Err("only pending remote operations can be rejected".into());
        }
        record.state = RemoteExecutionState::Rejected;
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn reject_after_dispatch(
        &mut self,
        operation_id: &str,
        request_id: &str,
        current_account_fence: &str,
        current_account_epoch: u64,
        remote_ack_id: Option<&str>,
        now_ms: u64,
    ) -> Result<(), String> {
        if let Some(remote_ack_id) = remote_ack_id {
            validate_identity("remote acknowledgement", remote_ack_id)?;
        }
        let record = self
            .state
            .operations
            .get_mut(operation_id)
            .ok_or("remote operation is unknown")?;
        if record.request_id != request_id {
            return Err("stale or mismatched remote rejection callback".into());
        }
        if record.account_fence != current_account_fence
            || record.account_epoch != current_account_epoch
        {
            return Err("stale remote rejection from old account epoch".into());
        }
        if !matches!(record.state, RemoteExecutionState::Sent | RemoteExecutionState::Acked) {
            return Err(format!(
                "remote rejection is invalid from state {:?}",
                record.state
            ));
        }
        record.state = RemoteExecutionState::Rejected;
        if let Some(remote_ack_id) = remote_ack_id {
            record.remote_ack_id = Some(remote_ack_id.into());
        }
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn reconcile_rejected(
        &mut self,
        operation_id: &str,
        current_account_fence: &str,
        current_account_epoch: u64,
        remote_ack_id: Option<&str>,
        now_ms: u64,
    ) -> Result<(), String> {
        if let Some(remote_ack_id) = remote_ack_id {
            validate_identity("remote acknowledgement", remote_ack_id)?;
        }
        let record = self
            .state
            .operations
            .get_mut(operation_id)
            .ok_or("remote operation is unknown")?;
        if record.account_fence != current_account_fence
            || record.account_epoch != current_account_epoch
        {
            return Err("cannot reconcile remote rejection across account epochs".into());
        }
        if record.state != RemoteExecutionState::OutcomeUnknown {
            return Err("only outcome-unknown remote operations can reconcile rejection".into());
        }
        record.state = RemoteExecutionState::Rejected;
        if let Some(remote_ack_id) = remote_ack_id {
            record.remote_ack_id = Some(remote_ack_id.into());
        }
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn confirm_cancel_after_dispatch(
        &mut self,
        operation_id: &str,
        current_account_fence: &str,
        current_account_epoch: u64,
        remote_ack_id: Option<&str>,
        now_ms: u64,
    ) -> Result<(), String> {
        if let Some(remote_ack_id) = remote_ack_id {
            validate_identity("remote acknowledgement", remote_ack_id)?;
        }
        let record = self
            .state
            .operations
            .get_mut(operation_id)
            .ok_or("remote operation is unknown")?;
        if record.account_fence != current_account_fence
            || record.account_epoch != current_account_epoch
        {
            return Err("stale remote cancel confirmation from old account epoch".into());
        }
        if !matches!(
            record.state,
            RemoteExecutionState::Sent
                | RemoteExecutionState::Acked
                | RemoteExecutionState::OutcomeUnknown
        ) {
            return Err(format!(
                "remote cancel confirmation is invalid from state {:?}",
                record.state
            ));
        }
        record.state = RemoteExecutionState::Cancelled;
        if let Some(remote_ack_id) = remote_ack_id {
            record.remote_ack_id = Some(remote_ack_id.into());
        }
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn disconnect(&mut self, now_ms: u64) -> Result<usize, String> {
        let mut changed = 0usize;
        for record in self.state.operations.values_mut() {
            if matches!(
                record.state,
                RemoteExecutionState::Sent | RemoteExecutionState::Acked
            ) {
                record.state = RemoteExecutionState::OutcomeUnknown;
                record.updated_at_ms = now_ms;
                changed += 1;
            }
        }
        if changed > 0 {
            self.persist()?;
        }
        Ok(changed)
    }

    pub fn reconciliation_required(
        &self,
        current_account_fence: &str,
        current_account_epoch: u64,
    ) -> Vec<&RemoteExecutionRecord> {
        self.state
            .operations
            .values()
            .filter(|record| {
                record.account_fence == current_account_fence
                    && record.account_epoch == current_account_epoch
                    && record.state == RemoteExecutionState::OutcomeUnknown
            })
            .collect()
    }

    pub fn reconcile(
        &mut self,
        operation_id: &str,
        current_account_fence: &str,
        current_account_epoch: u64,
        output_json: String,
        now_ms: u64,
    ) -> Result<(), String> {
        let record = self
            .state
            .operations
            .get_mut(operation_id)
            .ok_or("remote operation is unknown")?;
        if record.account_fence != current_account_fence
            || record.account_epoch != current_account_epoch
        {
            return Err("cannot reconcile remote operation across account epochs".into());
        }
        if record.state != RemoteExecutionState::OutcomeUnknown {
            return Err("only outcome-unknown remote operations can reconcile".into());
        }
        record.state = RemoteExecutionState::Completed;
        record.terminal_output_json = Some(output_json);
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn fence_account_switch(
        &mut self,
        previous_account_fence: &str,
        previous_account_epoch: u64,
        now_ms: u64,
    ) -> Result<usize, String> {
        let mut changed = 0usize;
        for record in self.state.operations.values_mut() {
            if record.account_fence == previous_account_fence
                && record.account_epoch == previous_account_epoch
                && matches!(
                    record.state,
                    RemoteExecutionState::Pending
                        | RemoteExecutionState::Sent
                        | RemoteExecutionState::Acked
                )
            {
                record.state = if record.state == RemoteExecutionState::Pending {
                    RemoteExecutionState::Cancelled
                } else {
                    RemoteExecutionState::OutcomeUnknown
                };
                record.updated_at_ms = now_ms;
                changed += 1;
            }
        }
        if changed > 0 {
            self.persist()?;
        }
        Ok(changed)
    }

    pub fn revoke_permission(
        &mut self,
        permission_grant_id: &str,
        now_ms: u64,
    ) -> Result<usize, String> {
        let mut changed = 0usize;
        for record in self.state.operations.values_mut() {
            if record.permission_grant_id == permission_grant_id
                && matches!(
                    record.state,
                    RemoteExecutionState::Pending
                        | RemoteExecutionState::Sent
                        | RemoteExecutionState::Acked
                )
            {
                record.state = if record.state == RemoteExecutionState::Pending {
                    RemoteExecutionState::Cancelled
                } else {
                    RemoteExecutionState::OutcomeUnknown
                };
                record.updated_at_ms = now_ms;
                changed += 1;
            }
        }
        if changed > 0 {
            self.persist()?;
        }
        Ok(changed)
    }

    pub fn record(&self, operation_id: &str) -> Option<&RemoteExecutionRecord> {
        self.state.operations.get(operation_id)
    }

    fn mark_outcome_unknown(&mut self, operation_id: &str, now_ms: u64) -> Result<(), String> {
        let record = self
            .state
            .operations
            .get_mut(operation_id)
            .ok_or("remote operation is unknown")?;
        if !matches!(
            record.state,
            RemoteExecutionState::Sent | RemoteExecutionState::Acked
        ) {
            return Err("only dispatched remote operations can become outcome-unknown".into());
        }
        record.state = RemoteExecutionState::OutcomeUnknown;
        record.updated_at_ms = now_ms;
        self.persist()
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(&self.state).map_err(|error| error.to_string())?;
        let temp = self.path.with_extension("tmp");
        fs::write(&temp, bytes).map_err(|error| error.to_string())?;
        fs::rename(&temp, &self.path).map_err(|error| error.to_string())
    }
}

fn assert_current(
    record: &RemoteExecutionRecord,
    account_fence: &str,
    account_epoch: u64,
    permission_grant_id: &str,
) -> Result<(), String> {
    if record.account_fence != account_fence || record.account_epoch != account_epoch {
        return Err("remote execution account epoch is stale".into());
    }
    if record.permission_grant_id != permission_grant_id {
        return Err("remote execution permission grant is stale or revoked".into());
    }
    Ok(())
}

fn validate_identity(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(format!("{label} identity is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(operation_id: &str, request_id: &str) -> RemoteExecutionRecord {
        RemoteExecutionRecord {
            operation_id: operation_id.into(),
            request_id: request_id.into(),
            capability_id: "computer.use".into(),
            account_fence: "account-a:epoch-7".into(),
            account_epoch: 7,
            permission_grant_id: "grant-1".into(),
            device_id: "device-1".into(),
            state: RemoteExecutionState::Pending,
            created_at_ms: 1,
            updated_at_ms: 1,
            remote_ack_id: None,
            terminal_output_json: None,
        }
    }

    #[test]
    fn duplicate_operation_and_request_identities_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let mut journal = RemoteExecutionJournal::open(root.path().join("remote.json"), 1).unwrap();
        journal.begin(record("op-1", "req-1")).unwrap();
        assert!(journal.begin(record("op-1", "req-2")).is_err());
        assert!(journal.begin(record("op-2", "req-1")).is_err());
    }

    #[test]
    fn success_requires_matching_ack_and_completion_identity() {
        let root = tempfile::tempdir().unwrap();
        let mut journal = RemoteExecutionJournal::open(root.path().join("remote.json"), 1).unwrap();
        journal.begin(record("op-1", "req-1")).unwrap();
        journal
            .mark_sent("op-1", "account-a:epoch-7", 7, "grant-1", 2)
            .unwrap();
        journal
            .acknowledge("op-1", "req-1", "account-a:epoch-7", 7, "ack-1", 3)
            .unwrap();
        journal
            .complete(
                "op-1",
                "req-1",
                "account-a:epoch-7",
                7,
                r#"{"status":"completed"}"#.into(),
                4,
            )
            .unwrap();
        assert_eq!(
            journal.record("op-1").unwrap().state,
            RemoteExecutionState::Completed
        );
    }

    #[test]
    fn explicit_rejection_is_terminal_before_remote_dispatch() {
        let root = tempfile::tempdir().unwrap();
        let mut journal = RemoteExecutionJournal::open(root.path().join("remote.json"), 1).unwrap();
        journal.begin(record("op-1", "req-1")).unwrap();
        journal.reject_before_dispatch("op-1", 2).unwrap();
        assert_eq!(
            journal.record("op-1").unwrap().state,
            RemoteExecutionState::Rejected
        );
        assert!(journal
            .mark_sent("op-1", "account-a:epoch-7", 7, "grant-1", 3)
            .is_err());
    }

    #[test]
    fn cancellation_before_dispatch_is_terminal_but_inflight_cancel_requires_reconciliation() {
        let root = tempfile::tempdir().unwrap();
        let mut journal = RemoteExecutionJournal::open(root.path().join("remote.json"), 1).unwrap();
        journal.begin(record("op-pending", "req-pending")).unwrap();
        journal.cancel_before_dispatch("op-pending", 2).unwrap();
        assert_eq!(
            journal.record("op-pending").unwrap().state,
            RemoteExecutionState::Cancelled
        );

        journal.begin(record("op-sent", "req-sent")).unwrap();
        journal
            .mark_sent("op-sent", "account-a:epoch-7", 7, "grant-1", 3)
            .unwrap();
        journal.cancel_after_dispatch("op-sent", 4).unwrap();
        assert_eq!(
            journal.record("op-sent").unwrap().state,
            RemoteExecutionState::OutcomeUnknown
        );
    }

    #[test]
    fn disconnect_never_blind_replays_and_reconnect_requires_reconciliation() {
        let root = tempfile::tempdir().unwrap();
        let mut journal = RemoteExecutionJournal::open(root.path().join("remote.json"), 1).unwrap();
        journal.begin(record("op-1", "req-1")).unwrap();
        journal
            .mark_sent("op-1", "account-a:epoch-7", 7, "grant-1", 2)
            .unwrap();
        assert_eq!(journal.disconnect(3).unwrap(), 1);
        let pending = journal.reconciliation_required("account-a:epoch-7", 7);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].operation_id, "op-1");
        assert!(journal
            .mark_sent("op-1", "account-a:epoch-7", 7, "grant-1", 4)
            .is_err());
    }

    #[test]
    fn timeout_after_dispatch_is_outcome_unknown_and_reconciles() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("remote.json");
        let mut journal = RemoteExecutionJournal::open(&path, 1).unwrap();
        journal.begin(record("op-1", "req-1")).unwrap();
        journal
            .mark_sent("op-1", "account-a:epoch-7", 7, "grant-1", 2)
            .unwrap();
        journal.timeout("op-1", 3).unwrap();
        assert_eq!(
            journal.record("op-1").unwrap().state,
            RemoteExecutionState::OutcomeUnknown
        );
        journal
            .reconcile(
                "op-1",
                "account-a:epoch-7",
                7,
                r#"{"status":"applied"}"#.into(),
                4,
            )
            .unwrap();
        assert_eq!(
            journal.record("op-1").unwrap().state,
            RemoteExecutionState::Completed
        );
    }

    #[test]
    fn process_reopen_fences_inflight_remote_side_effect() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("remote.json");
        {
            let mut journal = RemoteExecutionJournal::open(&path, 1).unwrap();
            journal.begin(record("op-1", "req-1")).unwrap();
            journal
                .mark_sent("op-1", "account-a:epoch-7", 7, "grant-1", 2)
                .unwrap();
        }
        let reopened = RemoteExecutionJournal::open(&path, 99).unwrap();
        assert_eq!(
            reopened.record("op-1").unwrap().state,
            RemoteExecutionState::OutcomeUnknown
        );
    }

    #[test]
    fn stale_account_callback_and_revoked_permission_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let mut journal = RemoteExecutionJournal::open(root.path().join("remote.json"), 1).unwrap();
        journal.begin(record("op-1", "req-1")).unwrap();
        journal
            .mark_sent("op-1", "account-a:epoch-7", 7, "grant-1", 2)
            .unwrap();
        assert!(journal
            .acknowledge("op-1", "req-1", "account-b:epoch-8", 8, "ack-1", 3)
            .is_err());
        assert_eq!(journal.revoke_permission("grant-1", 4).unwrap(), 1);
        assert_eq!(
            journal.record("op-1").unwrap().state,
            RemoteExecutionState::OutcomeUnknown
        );
    }

    #[test]
    fn account_switch_cancels_unsent_and_fences_sent_work() {
        let root = tempfile::tempdir().unwrap();
        let mut journal = RemoteExecutionJournal::open(root.path().join("remote.json"), 1).unwrap();
        journal.begin(record("op-pending", "req-pending")).unwrap();
        journal.begin(record("op-sent", "req-sent")).unwrap();
        journal
            .mark_sent("op-sent", "account-a:epoch-7", 7, "grant-1", 2)
            .unwrap();
        assert_eq!(
            journal
                .fence_account_switch("account-a:epoch-7", 7, 3)
                .unwrap(),
            2
        );
        assert_eq!(
            journal.record("op-pending").unwrap().state,
            RemoteExecutionState::Cancelled
        );
        assert_eq!(
            journal.record("op-sent").unwrap().state,
            RemoteExecutionState::OutcomeUnknown
        );
    }
}
