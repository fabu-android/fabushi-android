use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const SAND_TEACH_MAX_DURATION_MS: u64 = 10 * 60_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TeachRecordingPhase {
    Idle,
    Starting,
    Recording,
    Stopping,
    OutcomeUnknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeachRecordingSnapshot {
    pub phase: TeachRecordingPhase,
    pub account_fence: Option<String>,
    pub account_epoch: u64,
    pub agent_id: Option<String>,
    pub request_id: Option<String>,
    pub operation_id: Option<String>,
    pub started_at_ms: Option<u64>,
    pub save_requested: Option<bool>,
    pub max_duration_ms: u64,
    pub outcome_unknown_reason: Option<String>,
}

impl TeachRecordingSnapshot {
    pub fn idle() -> Self {
        Self {
            phase: TeachRecordingPhase::Idle,
            account_fence: None,
            account_epoch: 0,
            agent_id: None,
            request_id: None,
            operation_id: None,
            started_at_ms: None,
            save_requested: None,
            max_duration_ms: SAND_TEACH_MAX_DURATION_MS,
            outcome_unknown_reason: None,
        }
    }

    pub fn requires_reconciliation(&self) -> bool {
        self.phase == TeachRecordingPhase::OutcomeUnknown
    }
}

pub struct DurableTeachRecordingOwner {
    path: PathBuf,
    snapshot: TeachRecordingSnapshot,
}

impl DurableTeachRecordingOwner {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut snapshot = if path.exists() {
            serde_json::from_slice::<TeachRecordingSnapshot>(
                &fs::read(&path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| format!("invalid teach recording state: {error}"))?
        } else {
            TeachRecordingSnapshot::idle()
        };
        validate_snapshot(&snapshot)?;

        // The remote recorder may have crossed the side-effect boundary while this process was
        // gone. Never infer "still recording", "stopped", or "not started" from local state.
        if matches!(
            snapshot.phase,
            TeachRecordingPhase::Starting
                | TeachRecordingPhase::Recording
                | TeachRecordingPhase::Stopping
        ) {
            snapshot.phase = TeachRecordingPhase::OutcomeUnknown;
            snapshot.outcome_unknown_reason =
                Some(format!("process_reopen_requires_remote_reconciliation:{now_ms}"));
        }

        let owner = Self { path, snapshot };
        owner.persist()?;
        Ok(owner)
    }

    pub fn snapshot(&self) -> &TeachRecordingSnapshot {
        &self.snapshot
    }

    pub fn begin_start(
        &mut self,
        account_fence: &str,
        account_epoch: u64,
        agent_id: &str,
        request_id: &str,
        operation_id: &str,
    ) -> Result<(), String> {
        validate_identity("account fence", account_fence)?;
        validate_identity("agent", agent_id)?;
        validate_identity("request", request_id)?;
        validate_identity("operation", operation_id)?;
        if account_epoch == 0 {
            return Err("teach recording account epoch must be positive".into());
        }
        if self.snapshot.phase != TeachRecordingPhase::Idle {
            return Err("teach recording already has an unsettled episode".into());
        }
        self.snapshot = TeachRecordingSnapshot {
            phase: TeachRecordingPhase::Starting,
            account_fence: Some(account_fence.into()),
            account_epoch,
            agent_id: Some(agent_id.into()),
            request_id: Some(request_id.into()),
            operation_id: Some(operation_id.into()),
            started_at_ms: None,
            save_requested: None,
            max_duration_ms: SAND_TEACH_MAX_DURATION_MS,
            outcome_unknown_reason: None,
        };
        self.persist()
    }

    pub fn mark_recording(
        &mut self,
        account_fence: &str,
        account_epoch: u64,
        operation_id: &str,
        started_at_ms: u64,
    ) -> Result<(), String> {
        self.assert_current(account_fence, account_epoch, operation_id)?;
        if self.snapshot.phase != TeachRecordingPhase::Starting {
            return Err("teach recording cannot enter recording from current phase".into());
        }
        if started_at_ms == 0 {
            return Err("teach recording start timestamp must be positive".into());
        }
        self.snapshot.phase = TeachRecordingPhase::Recording;
        self.snapshot.started_at_ms = Some(started_at_ms);
        self.persist()
    }

    pub fn begin_stop(
        &mut self,
        account_fence: &str,
        account_epoch: u64,
        operation_id: &str,
        save: bool,
    ) -> Result<(), String> {
        self.assert_current(account_fence, account_epoch, operation_id)?;
        if self.snapshot.phase != TeachRecordingPhase::Recording {
            return Err("teach recording can stop only from recording phase".into());
        }
        self.snapshot.phase = TeachRecordingPhase::Stopping;
        self.snapshot.save_requested = Some(save);
        self.persist()
    }

    pub fn settle_idle(
        &mut self,
        account_fence: &str,
        account_epoch: u64,
        operation_id: &str,
    ) -> Result<(), String> {
        self.assert_current(account_fence, account_epoch, operation_id)?;
        if !matches!(
            self.snapshot.phase,
            TeachRecordingPhase::Starting
                | TeachRecordingPhase::Stopping
                | TeachRecordingPhase::OutcomeUnknown
        ) {
            return Err("teach recording cannot settle from current phase".into());
        }
        self.snapshot = TeachRecordingSnapshot::idle();
        self.persist()
    }

    pub fn mark_outcome_unknown(
        &mut self,
        account_fence: &str,
        account_epoch: u64,
        operation_id: &str,
        reason: &str,
    ) -> Result<(), String> {
        self.assert_current(account_fence, account_epoch, operation_id)?;
        validate_text("outcome unknown reason", reason, 320)?;
        if self.snapshot.phase == TeachRecordingPhase::Idle {
            return Err("idle teach recording cannot become outcome unknown".into());
        }
        self.snapshot.phase = TeachRecordingPhase::OutcomeUnknown;
        self.snapshot.outcome_unknown_reason = Some(reason.into());
        self.persist()
    }

    pub fn fence_account_switch(
        &mut self,
        previous_account_fence: &str,
        previous_account_epoch: u64,
    ) -> Result<(), String> {
        if self.snapshot.phase == TeachRecordingPhase::Idle {
            return Ok(());
        }
        if self.snapshot.account_fence.as_deref() != Some(previous_account_fence)
            || self.snapshot.account_epoch != previous_account_epoch
        {
            return Err("teach recording account fence mismatch".into());
        }
        self.snapshot.phase = TeachRecordingPhase::OutcomeUnknown;
        self.snapshot.outcome_unknown_reason = Some("account_switch_requires_remote_reconciliation".into());
        self.persist()
    }

    fn assert_current(
        &self,
        account_fence: &str,
        account_epoch: u64,
        operation_id: &str,
    ) -> Result<(), String> {
        if self.snapshot.account_fence.as_deref() != Some(account_fence)
            || self.snapshot.account_epoch != account_epoch
        {
            return Err("stale teach recording callback from old account epoch".into());
        }
        if self.snapshot.operation_id.as_deref() != Some(operation_id) {
            return Err("stale or mismatched teach recording operation".into());
        }
        Ok(())
    }

    fn persist(&self) -> Result<(), String> {
        atomic_write_json(&self.path, &self.snapshot)
    }
}

fn validate_snapshot(snapshot: &TeachRecordingSnapshot) -> Result<(), String> {
    if snapshot.max_duration_ms != SAND_TEACH_MAX_DURATION_MS {
        return Err("teach recording duration contract is invalid".into());
    }
    if snapshot.phase == TeachRecordingPhase::Idle {
        if snapshot.account_fence.is_some()
            || snapshot.account_epoch != 0
            || snapshot.agent_id.is_some()
            || snapshot.request_id.is_some()
            || snapshot.operation_id.is_some()
            || snapshot.started_at_ms.is_some()
            || snapshot.save_requested.is_some()
        {
            return Err("idle teach recording state contains stale identity".into());
        }
        return Ok(());
    }
    validate_identity(
        "account fence",
        snapshot.account_fence.as_deref().ok_or("teach recording account fence missing")?,
    )?;
    if snapshot.account_epoch == 0 {
        return Err("teach recording account epoch must be positive".into());
    }
    validate_identity(
        "agent",
        snapshot.agent_id.as_deref().ok_or("teach recording agent missing")?,
    )?;
    validate_identity(
        "request",
        snapshot.request_id.as_deref().ok_or("teach recording request missing")?,
    )?;
    validate_identity(
        "operation",
        snapshot.operation_id.as_deref().ok_or("teach recording operation missing")?,
    )?;
    if matches!(snapshot.phase, TeachRecordingPhase::Recording | TeachRecordingPhase::Stopping)
        && snapshot.started_at_ms.is_none()
    {
        return Err("active teach recording is missing start timestamp".into());
    }
    Ok(())
}

fn validate_identity(label: &str, value: &str) -> Result<(), String> {
    validate_text(label, value, 192)?;
    if value.trim() != value {
        return Err(format!("{label} must not contain surrounding whitespace"));
    }
    Ok(())
}

fn validate_text(label: &str, value: &str, max: usize) -> Result<(), String> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

fn atomic_write_json(path: &Path, snapshot: &TeachRecordingSnapshot) -> Result<(), String> {
    let parent = path.parent().ok_or("teach recording store has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(snapshot).map_err(|error| error.to_string())?;
    fs::write(&tmp, bytes).map_err(|error| error.to_string())?;
    fs::rename(&tmp, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_stop_and_save_intent_are_durable_and_identity_fenced() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("teach-recording.json");
        let mut owner = DurableTeachRecordingOwner::open(&path, 1).unwrap();
        owner
            .begin_start("session:account-a", 7, "agent-a", "req-1", "op-1")
            .unwrap();
        assert_eq!(owner.snapshot().phase, TeachRecordingPhase::Starting);
        assert!(owner
            .mark_recording("session:account-b", 8, "op-1", 10)
            .unwrap_err()
            .contains("old account"));
        owner
            .mark_recording("session:account-a", 7, "op-1", 10)
            .unwrap();
        owner
            .begin_stop("session:account-a", 7, "op-1", true)
            .unwrap();
        assert_eq!(owner.snapshot().save_requested, Some(true));
        owner
            .settle_idle("session:account-a", 7, "op-1")
            .unwrap();
        assert_eq!(owner.snapshot(), &TeachRecordingSnapshot::idle());
    }

    #[test]
    fn process_reopen_never_replays_or_assumes_remote_side_effect_outcome() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("teach-recording.json");
        {
            let mut owner = DurableTeachRecordingOwner::open(&path, 1).unwrap();
            owner
                .begin_start("session:account-a", 7, "agent-a", "req-1", "op-1")
                .unwrap();
            owner
                .mark_recording("session:account-a", 7, "op-1", 10)
                .unwrap();
        }
        let reopened = DurableTeachRecordingOwner::open(&path, 20).unwrap();
        assert_eq!(reopened.snapshot().phase, TeachRecordingPhase::OutcomeUnknown);
        assert!(reopened.snapshot().requires_reconciliation());
        assert!(reopened
            .snapshot()
            .outcome_unknown_reason
            .as_deref()
            .unwrap()
            .starts_with("process_reopen_requires_remote_reconciliation"));
    }

    #[test]
    fn duplicate_episode_and_account_switch_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("teach-recording.json");
        let mut owner = DurableTeachRecordingOwner::open(&path, 1).unwrap();
        owner
            .begin_start("session:account-a", 7, "agent-a", "req-1", "op-1")
            .unwrap();
        assert!(owner
            .begin_start("session:account-a", 7, "agent-a", "req-2", "op-2")
            .unwrap_err()
            .contains("unsettled"));
        owner.fence_account_switch("session:account-a", 7).unwrap();
        assert_eq!(owner.snapshot().phase, TeachRecordingPhase::OutcomeUnknown);
        assert!(owner
            .settle_idle("session:account-b", 8, "op-1")
            .unwrap_err()
            .contains("old account"));
    }

    #[test]
    fn duration_contract_matches_desktop_ten_minute_cap() {
        assert_eq!(SAND_TEACH_MAX_DURATION_MS, 600_000);
        assert_eq!(TeachRecordingSnapshot::idle().max_duration_ms, 600_000);
    }
}
