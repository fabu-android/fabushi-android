//! Portable 1Password provisioning safety contract.
//!
//! Android cannot execute the macOS `op` CLI. This module preserves the platform-independent
//! mutation, validation, and outcome-unknown rules while the actual operation stays behind an
//! explicitly authorized Remote Runner. No local process execution or credential reveal exists here.

use serde::{Deserialize, Serialize};
use std::{fmt, fs, path::{Path, PathBuf}};

pub const MIN_EXPIRATION_SECONDS: u64 = 60;
pub const MAX_EXPIRATION_SECONDS: u64 = 365 * 24 * 60 * 60;
pub const MAX_PROVIDER_ID_LENGTH: usize = 256;
pub const MAX_NAME_LENGTH: usize = 256;
pub const MAX_TOKEN_LENGTH: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProvisioningErrorCode {
    InvalidInput,
    Busy,
    SinkUnavailable,
    DeliveryIndeterminate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvisioningError {
    pub code: ProvisioningErrorCode,
    pub message: &'static str,
}

impl fmt::Display for ProvisioningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.message) }
}

impl std::error::Error for ProvisioningError {}

pub fn validate_provider_id(value: &str) -> Result<&str, ProvisioningError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_PROVIDER_ID_LENGTH
        || !value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    {
        return Err(invalid("1Password provider identity is invalid"));
    }
    Ok(value)
}

pub fn validate_name(value: &str) -> Result<&str, ProvisioningError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_NAME_LENGTH
        || value.starts_with('-')
        || value.chars().any(char::is_control)
    {
        return Err(invalid("1Password object name is invalid"));
    }
    Ok(value)
}

pub fn validate_expiration(seconds: u64) -> Result<u64, ProvisioningError> {
    if !(MIN_EXPIRATION_SECONDS..=MAX_EXPIRATION_SECONDS).contains(&seconds) {
        return Err(invalid("1Password credential expiration is invalid"));
    }
    Ok(seconds)
}

/// Validate an `op service-account create --raw` token without retaining or exposing it.
pub fn validate_service_account_token(value: &str) -> Result<(), ProvisioningError> {
    let value = value.trim();
    if value.len() < 5
        || value.len() > MAX_TOKEN_LENGTH
        || !value.starts_with("ops_")
        || !value[4..].bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return Err(invalid("1Password service-account credential is invalid"));
    }
    Ok(())
}

/// Single-owner mutation guard mirroring Desktop provisioning semantics.
///
/// An uncertain delivery is sticky. Callers MUST reconcile it against the authorized credential
/// consumer before another create/mint operation can begin; retrying the side effect is forbidden.
#[derive(Debug, Default)]
pub struct ProvisioningMutationGuard {
    mutation_in_progress: bool,
    delivery_indeterminate: bool,
}

impl ProvisioningMutationGuard {
    pub fn begin(&mut self, sink_available: bool) -> Result<(), ProvisioningError> {
        if !sink_available {
            return Err(ProvisioningError { code: ProvisioningErrorCode::SinkUnavailable, message: "1Password credential consumer is unavailable" });
        }
        if self.delivery_indeterminate {
            return Err(ProvisioningError { code: ProvisioningErrorCode::DeliveryIndeterminate, message: "previous 1Password credential delivery requires reconciliation" });
        }
        if self.mutation_in_progress {
            return Err(ProvisioningError { code: ProvisioningErrorCode::Busy, message: "1Password provisioning is already running" });
        }
        self.mutation_in_progress = true;
        Ok(())
    }

    pub fn finish_settled(&mut self) { self.mutation_in_progress = false; }

    pub fn finish_delivery_indeterminate(&mut self) {
        self.mutation_in_progress = false;
        self.delivery_indeterminate = true;
    }

    /// Only a canonical Remote Runner / credential-consumer reconciliation may clear uncertainty.
    pub fn reconcile_delivery(&mut self) { self.delivery_indeterminate = false; }

    pub fn delivery_indeterminate(&self) -> bool { self.delivery_indeterminate }
}


#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DurableProvisioningPhase {
    Idle,
    Prepared,
    Delivering,
    OutcomeUnknown,
    Delivered,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DurableProvisioningSnapshot {
    pub phase: DurableProvisioningPhase,
    pub account_fence: Option<String>,
    pub account_epoch: u64,
    pub request_id: Option<String>,
    pub operation_id: Option<String>,
    pub consumer_reference: Option<String>,
    pub outcome_unknown_reason: Option<String>,
}

impl DurableProvisioningSnapshot {
    pub fn idle() -> Self {
        Self {
            phase: DurableProvisioningPhase::Idle,
            account_fence: None,
            account_epoch: 0,
            request_id: None,
            operation_id: None,
            consumer_reference: None,
            outcome_unknown_reason: None,
        }
    }
    pub fn requires_reconciliation(&self) -> bool {
        self.phase == DurableProvisioningPhase::OutcomeUnknown
    }
}

/// Durable single owner for the side-effect boundary. Secret material is deliberately absent.
pub struct DurableOnePasswordProvisioningOwner {
    path: PathBuf,
    snapshot: DurableProvisioningSnapshot,
}

impl DurableOnePasswordProvisioningOwner {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut snapshot = if path.exists() {
            serde_json::from_slice::<DurableProvisioningSnapshot>(
                &fs::read(&path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| format!("invalid 1Password provisioning state: {error}"))?
        } else {
            DurableProvisioningSnapshot::idle()
        };
        validate_durable_snapshot(&snapshot)?;

        match snapshot.phase {
            DurableProvisioningPhase::Prepared => {
                snapshot = DurableProvisioningSnapshot::idle();
            }
            DurableProvisioningPhase::Delivering => {
                snapshot.phase = DurableProvisioningPhase::OutcomeUnknown;
                snapshot.outcome_unknown_reason =
                    Some("process_reopen_requires_delivery_reconciliation".into());
            }
            _ => {}
        }

        let owner = Self { path, snapshot };
        owner.persist()?;
        Ok(owner)
    }

    pub fn snapshot(&self) -> &DurableProvisioningSnapshot {
        &self.snapshot
    }

    pub fn begin(
        &mut self,
        account_fence: &str,
        account_epoch: u64,
        request_id: &str,
        operation_id: &str,
    ) -> Result<(), String> {
        validate_durable_identity("account fence", account_fence)?;
        validate_durable_identity("request", request_id)?;
        validate_durable_identity("operation", operation_id)?;
        if account_epoch == 0 {
            return Err("1Password provisioning account epoch must be positive".into());
        }
        if matches!(
            self.snapshot.phase,
            DurableProvisioningPhase::Prepared
                | DurableProvisioningPhase::Delivering
                | DurableProvisioningPhase::OutcomeUnknown
        ) {
            return Err("1Password provisioning already has an unsettled operation".into());
        }
        self.snapshot = DurableProvisioningSnapshot {
            phase: DurableProvisioningPhase::Prepared,
            account_fence: Some(account_fence.into()),
            account_epoch,
            request_id: Some(request_id.into()),
            operation_id: Some(operation_id.into()),
            consumer_reference: None,
            outcome_unknown_reason: None,
        };
        self.persist()
    }

    /// Persist before dispatching the authorized Remote Runner side effect.
    pub fn mark_delivering(
        &mut self,
        account_fence: &str,
        account_epoch: u64,
        operation_id: &str,
    ) -> Result<(), String> {
        self.assert_current(account_fence, account_epoch, operation_id)?;
        if self.snapshot.phase != DurableProvisioningPhase::Prepared {
            return Err("1Password delivery cannot begin from current phase".into());
        }
        self.snapshot.phase = DurableProvisioningPhase::Delivering;
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
            DurableProvisioningPhase::Delivering | DurableProvisioningPhase::OutcomeUnknown
        ) {
            return Err("1Password delivery cannot settle from current phase".into());
        }
        validate_durable_identity("consumer reference", consumer_reference)?;
        self.snapshot.phase = DurableProvisioningPhase::Delivered;
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
        if self.snapshot.phase != DurableProvisioningPhase::Delivering {
            return Err("only in-flight 1Password delivery can become outcome unknown".into());
        }
        validate_durable_text("outcome unknown reason", reason, 320)?;
        self.snapshot.phase = DurableProvisioningPhase::OutcomeUnknown;
        self.snapshot.outcome_unknown_reason = Some(reason.into());
        self.persist()
    }

    pub fn reconcile_not_delivered(
        &mut self,
        account_fence: &str,
        account_epoch: u64,
        operation_id: &str,
    ) -> Result<(), String> {
        self.assert_current(account_fence, account_epoch, operation_id)?;
        if self.snapshot.phase != DurableProvisioningPhase::OutcomeUnknown {
            return Err("1Password delivery does not require reconciliation".into());
        }
        self.snapshot = DurableProvisioningSnapshot::idle();
        self.persist()
    }

    pub fn fence_account_switch(
        &mut self,
        previous_account_fence: &str,
        previous_account_epoch: u64,
    ) -> Result<(), String> {
        if !matches!(
            self.snapshot.phase,
            DurableProvisioningPhase::Prepared
                | DurableProvisioningPhase::Delivering
                | DurableProvisioningPhase::OutcomeUnknown
        ) {
            return Ok(());
        }
        if self.snapshot.account_fence.as_deref() != Some(previous_account_fence)
            || self.snapshot.account_epoch != previous_account_epoch
        {
            return Err("1Password provisioning account fence mismatch".into());
        }
        match self.snapshot.phase {
            DurableProvisioningPhase::Prepared => {
                self.snapshot = DurableProvisioningSnapshot::idle();
            }
            DurableProvisioningPhase::Delivering => {
                self.snapshot.phase = DurableProvisioningPhase::OutcomeUnknown;
                self.snapshot.outcome_unknown_reason =
                    Some("account_switch_requires_delivery_reconciliation".into());
            }
            DurableProvisioningPhase::OutcomeUnknown => {}
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
            return Err("stale 1Password callback from old account epoch".into());
        }
        if self.snapshot.operation_id.as_deref() != Some(operation_id) {
            return Err("stale or mismatched 1Password provisioning operation".into());
        }
        Ok(())
    }

    fn persist(&self) -> Result<(), String> {
        atomic_write_durable_snapshot(&self.path, &self.snapshot)
    }
}

fn validate_durable_snapshot(snapshot: &DurableProvisioningSnapshot) -> Result<(), String> {
    if snapshot.phase == DurableProvisioningPhase::Idle {
        if snapshot.account_fence.is_some()
            || snapshot.account_epoch != 0
            || snapshot.request_id.is_some()
            || snapshot.operation_id.is_some()
            || snapshot.consumer_reference.is_some()
            || snapshot.outcome_unknown_reason.is_some()
        {
            return Err("idle 1Password provisioning contains stale identity".into());
        }
        return Ok(());
    }
    validate_durable_identity(
        "account fence",
        snapshot.account_fence.as_deref().ok_or("1Password account fence missing")?,
    )?;
    if snapshot.account_epoch == 0 {
        return Err("1Password provisioning account epoch must be positive".into());
    }
    validate_durable_identity(
        "request",
        snapshot.request_id.as_deref().ok_or("1Password request missing")?,
    )?;
    validate_durable_identity(
        "operation",
        snapshot.operation_id.as_deref().ok_or("1Password operation missing")?,
    )?;
    if snapshot.phase == DurableProvisioningPhase::Delivered {
        validate_durable_identity(
            "consumer reference",
            snapshot
                .consumer_reference
                .as_deref()
                .ok_or("1Password consumer reference missing")?,
        )?;
    } else if snapshot.consumer_reference.is_some() {
        return Err("unsettled 1Password provisioning contains consumer reference".into());
    }
    if snapshot.phase == DurableProvisioningPhase::OutcomeUnknown {
        validate_durable_text(
            "outcome unknown reason",
            snapshot
                .outcome_unknown_reason
                .as_deref()
                .ok_or("1Password outcome unknown reason missing")?,
            320,
        )?;
    } else if snapshot.outcome_unknown_reason.is_some() {
        return Err("1Password provisioning contains stale outcome unknown reason".into());
    }
    Ok(())
}

fn validate_durable_identity(label: &str, value: &str) -> Result<(), String> {
    validate_durable_text(label, value, 192)?;
    if value.trim() != value {
        return Err(format!("{label} must not contain surrounding whitespace"));
    }
    Ok(())
}

fn validate_durable_text(label: &str, value: &str, max: usize) -> Result<(), String> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

fn atomic_write_durable_snapshot(path: &Path, snapshot: &DurableProvisioningSnapshot) -> Result<(), String> {
    let parent = path.parent().ok_or("1Password provisioning store has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(snapshot).map_err(|error| error.to_string())?;
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).map_err(|error| error.to_string())
}

fn invalid(message: &'static str) -> ProvisioningError {
    ProvisioningError { code: ProvisioningErrorCode::InvalidInput, message }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_desktop_compatible_identifiers_names_expiration_and_tokens() {
        assert_eq!(validate_provider_id("account_1.test-2").unwrap(), "account_1.test-2");
        assert!(validate_provider_id("../bad/account").is_err());
        assert_eq!(validate_name("Shared with Sand").unwrap(), "Shared with Sand");
        assert!(validate_name("-bad").is_err());
        assert_eq!(validate_expiration(3_600).unwrap(), 3_600);
        assert!(validate_expiration(59).is_err());
        assert!(validate_service_account_token("ops_Abc_123-xyz").is_ok());
        assert!(validate_service_account_token("op_bad").is_err());
    }

    #[test]
    fn single_mutation_and_sink_availability_fail_closed() {
        let mut guard = ProvisioningMutationGuard::default();
        assert_eq!(guard.begin(false).unwrap_err().code, ProvisioningErrorCode::SinkUnavailable);
        guard.begin(true).unwrap();
        assert_eq!(guard.begin(true).unwrap_err().code, ProvisioningErrorCode::Busy);
        guard.finish_settled();
        guard.begin(true).unwrap();
    }

    #[test]
    fn indeterminate_delivery_blocks_retry_until_explicit_reconciliation() {
        let mut guard = ProvisioningMutationGuard::default();
        guard.begin(true).unwrap();
        guard.finish_delivery_indeterminate();
        assert!(guard.delivery_indeterminate());
        assert_eq!(guard.begin(true).unwrap_err().code, ProvisioningErrorCode::DeliveryIndeterminate);
        guard.reconcile_delivery();
        guard.begin(true).unwrap();
    }
    #[test]
    fn durable_delivery_reopens_as_outcome_unknown_without_secret_material() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("onepassword-provisioning.json");
        {
            let mut owner = DurableOnePasswordProvisioningOwner::open(&path).unwrap();
            owner.begin("session:a", 7, "req-1", "op-1").unwrap();
            owner.mark_delivering("session:a", 7, "op-1").unwrap();
        }
        let reopened = DurableOnePasswordProvisioningOwner::open(&path).unwrap();
        assert_eq!(reopened.snapshot().phase, DurableProvisioningPhase::OutcomeUnknown);
        assert!(reopened.snapshot().requires_reconciliation());
        let persisted = std::fs::read_to_string(path).unwrap();
        assert!(!persisted.contains("ops_"));
        assert!(!persisted.contains("token"));
        assert!(reopened.begin("session:a", 7, "req-2", "op-2").is_err());
    }

    #[test]
    fn durable_provisioning_fences_account_switch_and_explicit_reconciliation() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("onepassword-provisioning.json");
        let mut owner = DurableOnePasswordProvisioningOwner::open(&path).unwrap();
        owner.begin("session:a", 7, "req-1", "op-1").unwrap();
        owner.mark_delivering("session:a", 7, "op-1").unwrap();
        owner.fence_account_switch("session:a", 7).unwrap();
        assert_eq!(owner.snapshot().phase, DurableProvisioningPhase::OutcomeUnknown);
        assert!(owner
            .reconcile_not_delivered("session:b", 8, "op-1")
            .unwrap_err()
            .contains("old account"));
        owner
            .reconcile_not_delivered("session:a", 7, "op-1")
            .unwrap();
        assert_eq!(owner.snapshot(), &DurableProvisioningSnapshot::idle());
    }

    #[test]
    fn prepared_process_reopen_is_safe_to_discard_before_external_dispatch() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("onepassword-provisioning.json");
        {
            let mut owner = DurableOnePasswordProvisioningOwner::open(&path).unwrap();
            owner.begin("session:a", 7, "req-1", "op-1").unwrap();
        }
        let reopened = DurableOnePasswordProvisioningOwner::open(&path).unwrap();
        assert_eq!(reopened.snapshot(), &DurableProvisioningSnapshot::idle());
    }

}
