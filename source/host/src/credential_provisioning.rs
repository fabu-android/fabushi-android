use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredentialProvisioningPhase {
    Idle,
    Prepared,
    Delivering,
    OutcomeUnknown,
    Delivered,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialProvisioningSnapshot {
    pub phase: CredentialProvisioningPhase,
    pub provider: Option<String>,
    pub account_fence: Option<String>,
    pub account_epoch: u64,
    pub request_id: Option<String>,
    pub operation_id: Option<String>,
    pub consumer_reference: Option<String>,
    pub outcome_unknown_reason: Option<String>,
}

impl CredentialProvisioningSnapshot {
    pub fn idle() -> Self {
        Self {
            phase: CredentialProvisioningPhase::Idle,
            provider: None,
            account_fence: None,
            account_epoch: 0,
            request_id: None,
            operation_id: None,
            consumer_reference: None,
            outcome_unknown_reason: None,
        }
    }

    pub fn requires_reconciliation(&self) -> bool {
        self.phase == CredentialProvisioningPhase::OutcomeUnknown
    }
}

pub struct DurableCredentialProvisioningOwner {
    path: PathBuf,
    snapshot: CredentialProvisioningSnapshot,
}

impl DurableCredentialProvisioningOwner {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut snapshot = if path.exists() {
            serde_json::from_slice::<CredentialProvisioningSnapshot>(
                &fs::read(&path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| format!("invalid credential provisioning state: {error}"))?
        } else {
            CredentialProvisioningSnapshot::idle()
        };
        validate_snapshot(&snapshot)?;

        // Prepared means no external delivery is allowed to have started yet, so reopening can
        // safely discard it. Once Delivering was persisted, a process death makes the external
        // side effect ambiguous and must never be retried automatically.
        match snapshot.phase {
            CredentialProvisioningPhase::Prepared => {
                snapshot = CredentialProvisioningSnapshot::idle();
            }
            CredentialProvisioningPhase::Delivering => {
                snapshot.phase = CredentialProvisioningPhase::OutcomeUnknown;
                snapshot.outcome_unknown_reason =
                    Some("process_reopen_requires_delivery_reconciliation".into());
            }
            _ => {}
        }

        let owner = Self { path, snapshot };
        owner.persist()?;
        Ok(owner)
    }

    pub fn snapshot(&self) -> &CredentialProvisioningSnapshot {
        &self.snapshot
    }

    pub fn begin(
        &mut self,
        provider: &str,
        account_fence: &str,
        account_epoch: u64,
        request_id: &str,
        operation_id: &str,
    ) -> Result<(), String> {
        validate_provider(provider)?;
        validate_identity("account fence", account_fence)?;
        validate_identity("request", request_id)?;
        validate_identity("operation", operation_id)?;
        if account_epoch == 0 {
            return Err("credential provisioning account epoch must be positive".into());
        }
        if matches!(
            self.snapshot.phase,
            CredentialProvisioningPhase::Prepared
                | CredentialProvisioningPhase::Delivering
                | CredentialProvisioningPhase::OutcomeUnknown
        ) {
            return Err("credential provisioning already has an unsettled operation".into());
        }
        self.snapshot = CredentialProvisioningSnapshot {
            phase: CredentialProvisioningPhase::Prepared,
            provider: Some(provider.into()),
            account_fence: Some(account_fence.into()),
            account_epoch,
            request_id: Some(request_id.into()),
            operation_id: Some(operation_id.into()),
            consumer_reference: None,
            outcome_unknown_reason: None,
        };
        self.persist()
    }

    /// Persist this state immediately before crossing the external provisioning boundary.
    pub fn mark_delivering(
        &mut self,
        account_fence: &str,
        account_epoch: u64,
        operation_id: &str,
    ) -> Result<(), String> {
        self.assert_current(account_fence, account_epoch, operation_id)?;
        if self.snapshot.phase != CredentialProvisioningPhase::Prepared {
            return Err("credential provisioning cannot begin delivery from current phase".into());
        }
        self.snapshot.phase = CredentialProvisioningPhase::Delivering;
        self.persist()
    }

    pub fn settle_delivered(
        &mut self,
        account_fence: &str,
        account_epoch: u64,
        operation_id: &str,
        consumer_reference: &str,
    ) -> Result<(), String> {
        self.assert_current(account_fence, account_epoch, operation_id)?;
        if !matches!(
            self.snapshot.phase,
            CredentialProvisioningPhase::Delivering
                | CredentialProvisioningPhase::OutcomeUnknown
        ) {
            return Err("credential provisioning cannot settle delivered from current phase".into());
        }
        validate_identity("consumer reference", consumer_reference)?;
        self.snapshot.phase = CredentialProvisioningPhase::Delivered;
        self.snapshot.consumer_reference = Some(consumer_reference.into());
        self.snapshot.outcome_unknown_reason = None;
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
        if self.snapshot.phase != CredentialProvisioningPhase::Delivering {
            return Err("only an in-flight delivery can become outcome unknown".into());
        }
        validate_text("outcome unknown reason", reason, 320)?;
        self.snapshot.phase = CredentialProvisioningPhase::OutcomeUnknown;
        self.snapshot.outcome_unknown_reason = Some(reason.into());
        self.persist()
    }

    /// Reconciliation proved that the external consumer did not accept the credential.
    pub fn reconcile_not_delivered(
        &mut self,
        account_fence: &str,
        account_epoch: u64,
        operation_id: &str,
    ) -> Result<(), String> {
        self.assert_current(account_fence, account_epoch, operation_id)?;
        if self.snapshot.phase != CredentialProvisioningPhase::OutcomeUnknown {
            return Err("credential provisioning does not require reconciliation".into());
        }
        self.snapshot = CredentialProvisioningSnapshot::idle();
        self.persist()
    }

    pub fn fence_account_switch(
        &mut self,
        previous_account_fence: &str,
        previous_account_epoch: u64,
    ) -> Result<(), String> {
        if !matches!(
            self.snapshot.phase,
            CredentialProvisioningPhase::Prepared
                | CredentialProvisioningPhase::Delivering
                | CredentialProvisioningPhase::OutcomeUnknown
        ) {
            return Ok(());
        }
        if self.snapshot.account_fence.as_deref() != Some(previous_account_fence)
            || self.snapshot.account_epoch != previous_account_epoch
        {
            return Err("credential provisioning account fence mismatch".into());
        }
        match self.snapshot.phase {
            CredentialProvisioningPhase::Prepared => {
                self.snapshot = CredentialProvisioningSnapshot::idle();
            }
            CredentialProvisioningPhase::Delivering => {
                self.snapshot.phase = CredentialProvisioningPhase::OutcomeUnknown;
                self.snapshot.outcome_unknown_reason =
                    Some("account_switch_requires_delivery_reconciliation".into());
            }
            CredentialProvisioningPhase::OutcomeUnknown => {}
            _ => unreachable!(),
        }
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
            return Err("stale credential provisioning callback from old account epoch".into());
        }
        if self.snapshot.operation_id.as_deref() != Some(operation_id) {
            return Err("stale or mismatched credential provisioning operation".into());
        }
        Ok(())
    }

    fn persist(&self) -> Result<(), String> {
        atomic_write_json(&self.path, &self.snapshot)
    }
}

fn validate_snapshot(snapshot: &CredentialProvisioningSnapshot) -> Result<(), String> {
    if snapshot.phase == CredentialProvisioningPhase::Idle {
        if snapshot.provider.is_some()
            || snapshot.account_fence.is_some()
            || snapshot.account_epoch != 0
            || snapshot.request_id.is_some()
            || snapshot.operation_id.is_some()
            || snapshot.consumer_reference.is_some()
            || snapshot.outcome_unknown_reason.is_some()
        {
            return Err("idle credential provisioning state contains stale identity".into());
        }
        return Ok(());
    }
    validate_provider(
        snapshot
            .provider
            .as_deref()
            .ok_or("credential provisioning provider missing")?,
    )?;
    validate_identity(
        "account fence",
        snapshot
            .account_fence
            .as_deref()
            .ok_or("credential provisioning account fence missing")?,
    )?;
    if snapshot.account_epoch == 0 {
        return Err("credential provisioning account epoch must be positive".into());
    }
    validate_identity(
        "request",
        snapshot
            .request_id
            .as_deref()
            .ok_or("credential provisioning request missing")?,
    )?;
    validate_identity(
        "operation",
        snapshot
            .operation_id
            .as_deref()
            .ok_or("credential provisioning operation missing")?,
    )?;
    if snapshot.phase == CredentialProvisioningPhase::Delivered {
        validate_identity(
            "consumer reference",
            snapshot
                .consumer_reference
                .as_deref()
                .ok_or("delivered credential provisioning consumer reference missing")?,
        )?;
    } else if snapshot.consumer_reference.is_some() {
        return Err("unsettled credential provisioning cannot contain a consumer reference".into());
    }
    if snapshot.phase == CredentialProvisioningPhase::OutcomeUnknown {
        validate_text(
            "outcome unknown reason",
            snapshot
                .outcome_unknown_reason
                .as_deref()
                .ok_or("credential provisioning outcome unknown reason missing")?,
            320,
        )?;
    } else if snapshot.outcome_unknown_reason.is_some() {
        return Err("credential provisioning state contains a stale outcome-unknown reason".into());
    }
    Ok(())
}

fn validate_provider(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err("credential provisioning provider is invalid".into());
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

fn atomic_write_json(path: &Path, snapshot: &CredentialProvisioningSnapshot) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("credential provisioning store has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(snapshot).map_err(|error| error.to_string())?;
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_is_single_flight_and_account_fenced() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("credential-provisioning.json");
        let mut owner = DurableCredentialProvisioningOwner::open(&path).unwrap();
        owner
            .begin("onepassword", "session:a", 7, "req-1", "op-1")
            .unwrap();
        assert!(owner
            .begin("onepassword", "session:a", 7, "req-2", "op-2")
            .unwrap_err()
            .contains("unsettled"));
        assert!(owner
            .mark_delivering("session:b", 8, "op-1")
            .unwrap_err()
            .contains("old account"));
        owner.mark_delivering("session:a", 7, "op-1").unwrap();
        owner
            .settle_delivered("session:a", 7, "op-1", "vault:credential-a")
            .unwrap();
        assert_eq!(owner.snapshot().phase, CredentialProvisioningPhase::Delivered);
        assert_eq!(
            owner.snapshot().consumer_reference.as_deref(),
            Some("vault:credential-a")
        );
    }

    #[test]
    fn process_reopen_requires_reconciliation_after_delivery_boundary() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("credential-provisioning.json");
        {
            let mut owner = DurableCredentialProvisioningOwner::open(&path).unwrap();
            owner
                .begin("onepassword", "session:a", 7, "req-1", "op-1")
                .unwrap();
            owner.mark_delivering("session:a", 7, "op-1").unwrap();
        }
        let reopened = DurableCredentialProvisioningOwner::open(&path).unwrap();
        assert_eq!(
            reopened.snapshot().phase,
            CredentialProvisioningPhase::OutcomeUnknown
        );
        assert!(reopened.snapshot().requires_reconciliation());
        assert!(reopened
            .begin("onepassword", "session:a", 7, "req-2", "op-2")
            .unwrap_err()
            .contains("unsettled"));
        reopened
            .snapshot()
            .outcome_unknown_reason
            .as_deref()
            .unwrap()
            .contains("process_reopen");
    }

    #[test]
    fn prepared_reopen_is_safe_to_discard_because_dispatch_has_not_started() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("credential-provisioning.json");
        {
            let mut owner = DurableCredentialProvisioningOwner::open(&path).unwrap();
            owner
                .begin("onepassword", "session:a", 7, "req-1", "op-1")
                .unwrap();
        }
        let reopened = DurableCredentialProvisioningOwner::open(&path).unwrap();
        assert_eq!(reopened.snapshot(), &CredentialProvisioningSnapshot::idle());
    }

    #[test]
    fn account_switch_never_replays_ambiguous_delivery() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("credential-provisioning.json");
        let mut owner = DurableCredentialProvisioningOwner::open(&path).unwrap();
        owner
            .begin("onepassword", "session:a", 7, "req-1", "op-1")
            .unwrap();
        owner.mark_delivering("session:a", 7, "op-1").unwrap();
        owner.fence_account_switch("session:a", 7).unwrap();
        assert_eq!(
            owner.snapshot().phase,
            CredentialProvisioningPhase::OutcomeUnknown
        );
        assert!(owner
            .begin("onepassword", "session:b", 8, "req-2", "op-2")
            .is_err());
        owner
            .settle_delivered("session:a", 7, "op-1", "vault:credential-a")
            .unwrap();
    }

    #[test]
    fn snapshot_never_serializes_secret_material() {
        let snapshot = CredentialProvisioningSnapshot {
            phase: CredentialProvisioningPhase::Delivered,
            provider: Some("onepassword".into()),
            account_fence: Some("session:a".into()),
            account_epoch: 7,
            request_id: Some("req-1".into()),
            operation_id: Some("op-1".into()),
            consumer_reference: Some("vault:credential-a".into()),
            outcome_unknown_reason: None,
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains("secret"));
        assert!(!json.contains("token"));
        assert!(json.contains("vault:credential-a"));
    }
}
