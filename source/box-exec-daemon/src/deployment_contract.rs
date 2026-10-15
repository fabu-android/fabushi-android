//! Shipping-placement contract for Remote execution on Android.
//!
//! Desktop's box-exec daemon is a loopback server that executes inside the user's Desktop box.
//! The Android APK is not that server. Android remains the authenticated client and delegates
//! desktop-OS work to an explicitly authorized external Remote Runner. Keeping this placement
//! explicit prevents a second hidden execution boundary from being introduced into the APK.

use crate::box_store_backend::{RemoteBoxStoreBackend, RemoteBoxStoreBackendConfig};
use crate::box_store_backend_policy::{
    BoxStoreBackendKind, is_box_store_sync_enabled, resolve_box_store_backend_policy,
};
use crate::box_control_backend::{ProductionBoxControlBackend, ProductionBoxControlConfig};
use crate::cloud_agent_backend::CloudAgentBackend;
use crate::cloud_agent_production::{ProductionCloudAgentConfig, ProductionCloudAgentExecutor};
use crate::file_transfer_backend::{RemoteFileTransferBackend, RemoteFileTransferConfig};
use crate::server::{RemoteExecutionAuthorizer, RemoteExecutionService};
use fabushi_android_shared::ExecutionError;
use std::{collections::BTreeMap, path::PathBuf};


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
/// Compose the bounded CopyToBox/CopyFromBox service for an authorized external Runner.
///
/// The Android APK remains only the authenticated client. The external Runner owns both configured
/// endpoint roots and executes the staged 100 MiB transfer contract after Host authorization.
/// Compose the Desktop-compatible Box control/MCP provider inside the Authorized External Runner.
/// MCP tool execution (ExecService field 11) is never blindly replayed during reconciliation;
/// only environment/load/state operations with idempotent semantics may be retried.
/// Compose the authenticated production CloudAgent executor inside the external Runner.
/// The service journal owns process-death/outcome-unknown state; mutation reconciliation never
/// blindly repeats launch/reply/rename/archive/delete side effects.
pub fn open_remote_cloud_agent_execution_service<A: RemoteExecutionAuthorizer>(
    journal_path: impl Into<PathBuf>,
    authorizer: A,
    config: ProductionCloudAgentConfig,
    now_ms: u64,
) -> Result<RemoteExecutionService<A, CloudAgentBackend<ProductionCloudAgentExecutor>>, ExecutionError> {
    crate::open_production_cloud_agent_service(journal_path, authorizer, config, now_ms)
}

pub fn open_remote_box_control_execution_service<A: RemoteExecutionAuthorizer>(
    journal_path: impl Into<PathBuf>,
    authorizer: A,
    config: ProductionBoxControlConfig,
    now_ms: u64,
) -> Result<RemoteExecutionService<A, ProductionBoxControlBackend>, ExecutionError> {
    crate::open_production_box_control_service(journal_path, authorizer, config, now_ms)
}

pub fn open_remote_file_transfer_execution_service<A: RemoteExecutionAuthorizer>(
    journal_path: impl Into<PathBuf>,
    authorizer: A,
    config: RemoteFileTransferConfig,
    now_ms: u64,
) -> Result<RemoteExecutionService<A, RemoteFileTransferBackend>, ExecutionError> {
    crate::file_transfer_backend::open_file_transfer_service(
        journal_path,
        authorizer,
        config,
        now_ms,
    )
}

/// Resolve the Desktop Box-store backend contract at the Authorized External Runner boundary.
///
/// The Android APK never reads these environment variables. The external Runner owns them.
/// The frozen V2 backend is the current production replacement; an explicit local root is also
/// honored for managed/test deployments. Desktop AgentStore remains fail-closed until a real
/// external Runner adapter exists, rather than silently changing persistence ownership.
pub fn open_remote_box_store_execution_service_from_environment<A: RemoteExecutionAuthorizer>(
    journal_path: impl Into<PathBuf>,
    authorizer: A,
    mut config: RemoteBoxStoreBackendConfig,
    environment: &BTreeMap<String, String>,
    now_ms: u64,
) -> Result<RemoteExecutionService<A, RemoteBoxStoreBackend>, ExecutionError> {
    if !is_box_store_sync_enabled(environment) {
        return Err(ExecutionError::CapabilityUnavailable(
            "box.store.sync is disabled by SAND_BOX_STORE_SYNC".into(),
        ));
    }
    let policy = resolve_box_store_backend_policy(environment);
    match policy.kind {
        BoxStoreBackendKind::LocalFs => {
            config.store_root = policy.local_dir.ok_or_else(|| {
                ExecutionError::InvalidRequest(
                    "local Box-store backend requires an absolute store root".into(),
                )
            })?;
        }
        BoxStoreBackendKind::SandBoxStoreV2 => {}
        BoxStoreBackendKind::AgentStore => {
            return Err(ExecutionError::CapabilityUnavailable(
                "Desktop AgentStore backend has no Authorized External Runner adapter yet".into(),
            ));
        }
    }
    open_remote_box_store_execution_service(journal_path, authorizer, config, now_ms)
}

pub fn open_remote_box_store_execution_service<A: RemoteExecutionAuthorizer>(
    journal_path: impl Into<PathBuf>,
    authorizer: A,
    config: RemoteBoxStoreBackendConfig,
    now_ms: u64,
) -> Result<RemoteExecutionService<A, RemoteBoxStoreBackend>, ExecutionError> {
    crate::open_production_box_store_sync_service(journal_path, authorizer, config, now_ms)
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
