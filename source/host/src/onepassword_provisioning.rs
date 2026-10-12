//! Portable 1Password provisioning safety contract.
//!
//! Android cannot execute the macOS `op` CLI. This module preserves the platform-independent
//! mutation, validation, and outcome-unknown rules while the actual operation stays behind an
//! explicitly authorized Remote Runner. No local process execution or credential reveal exists here.

use std::fmt;

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
}
