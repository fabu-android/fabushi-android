//! Shipping-placement contract for Remote execution on Android.
//!
//! Desktop's box-exec daemon is a loopback server that executes inside the user's Desktop box.
//! The Android APK is not that server. Android remains the authenticated client and delegates
//! desktop-OS work to an explicitly authorized external Remote Runner. Keeping this placement
//! explicit prevents a second hidden execution boundary from being introduced into the APK.

use crate::box_store_backend::{RemoteBoxStoreBackend, RemoteBoxStoreBackendConfig};
use crate::server::{RemoteExecutionAuthorizer, RemoteExecutionService};
use fabushi_android_shared::ExecutionError;
use std::path::PathBuf;


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AndroidRemoteExecutionPlacement {
    AuthorizedExternalRunner,
}

/// The Android application must not ship a box-exec HTTP listener.
///
/// The protocol/service implementation in this crate remains reviewable and workspace-tested so
/// the client/runner wire, durable idempotency, cancellation, and reconciliation semantics stay
/// aligned. A deployable Remote Runner may compose it outside the Android application package.
pub const ANDROID_APP_LISTENER_APPLICABLE: bool = false;

pub const ANDROID_REMOTE_EXECUTION_PLACEMENT: AndroidRemoteExecutionPlacement =
    AndroidRemoteExecutionPlacement::AuthorizedExternalRunner;

pub const USER_CAPABILITY_REPLACEMENT: &str =
    "Desktop OS shell/read/computer execution is preserved through an explicitly paired, authorized external Remote Runner; Android never claims arbitrary local OS execution.";

pub const SECURITY_BOUNDARY: &str =
    "Android Host dispatch requires a protected account/device/executor binding and one-time capability approval; the external runner independently authenticates every operation and owns side-effect execution.";

/// Compose the dedicated, durable Box Store execution service for an authorized external Runner.
pub fn open_remote_box_store_execution_service<A: RemoteExecutionAuthorizer>(
    journal_path: impl Into<PathBuf>,
    authorizer: A,
    config: RemoteBoxStoreBackendConfig,
    now_ms: u64,
) -> Result<RemoteExecutionService<A, RemoteBoxStoreBackend>, ExecutionError> {
    let backend = RemoteBoxStoreBackend::open(config)?;
    RemoteExecutionService::open(journal_path, authorizer, backend, now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_never_claims_an_in_apk_remote_execution_listener() {
        assert!(!ANDROID_APP_LISTENER_APPLICABLE);
        assert_eq!(
            ANDROID_REMOTE_EXECUTION_PLACEMENT,
            AndroidRemoteExecutionPlacement::AuthorizedExternalRunner,
        );
        assert!(USER_CAPABILITY_REPLACEMENT.contains("external Remote Runner"));
        assert!(SECURITY_BOUNDARY.contains("independently authenticates"));
    }
}
