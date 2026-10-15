//! Remote/Box Runner boundary for work that should not execute in the Android app process.

pub mod box_store_sync_backend;
pub mod box_disk_pressure_backend;
pub mod box_store_manifest_format;
pub mod box_store_pack_restore;
pub mod box_store_workspace_ignore;
pub mod box_store_hydration;
pub mod box_store_diagnostics;
pub mod browser_ua_backend;
pub mod cloud_agent_backend;
pub mod cloud_agent_generated_trace;
pub mod cloud_agent_images;
pub mod cloud_agent_production;
pub mod cloud_agent_transcript_dump;
pub mod cloud_agent_wire;
pub mod box_control_backend;
pub mod box_store_backend;
pub mod box_store_backend_policy;
pub mod cli;
#[allow(special_module_name)]
pub mod main;
pub mod deployment_contract;
pub mod file_transfer_backend;
pub mod protected_path_guard;
pub mod server;
pub mod transport;

use fabushi_android_shared::{ExecutionError, ExecutionRequest};
pub use browser_ua_backend::{
    BrowserUaReconcileBackend, BrowserUaReconcileExecutor, BrowserUaReconcileRequest,
    BROWSER_UA_OWNER_STAMP_LENGTH, BROWSER_UA_RECONCILE_CAPABILITY,
};
pub use cloud_agent_backend::{
    open_cloud_agent_service, CloudAgentAction, CloudAgentBackend, CloudAgentExecutor,
    CloudAgentImageRef, CloudAgentRequest, CLOUD_AGENT_CAPABILITY,
};
pub use cloud_agent_images::{
    CloudAgentImageLoadError, LoadedCloudAgentImage, ScopedCloudAgentImageLoader,
    CLOUD_AGENT_IMAGE_BYTE_LIMIT,
};
pub use cloud_agent_production::{
    open_production_cloud_agent_service, ProductionCloudAgentConfig, ProductionCloudAgentExecutor,
};
pub use box_store_sync_backend::{
    open_box_store_sync_service, BoxStoreSyncAction, BoxStoreSyncBackend, BoxStoreSyncExecutor,
    BoxStoreSyncRequest, BOX_STORE_SYNC_CAPABILITY,
};
pub use box_control_backend::{
    open_production_box_control_service, ProductionBoxControlBackend, ProductionBoxControlConfig,
    BOX_ENVIRONMENT_CAPABILITY, BOX_MCP_CAPABILITY,
};
pub use box_store_backend::{RemoteBoxStoreBackend, RemoteBoxStoreBackendConfig};
pub use box_disk_pressure_backend::{
    open_box_disk_pressure_service, BoxDiskPressureSampleRequest, RemoteBoxDiskPressureBackend,
    RemoteBoxDiskPressureConfig, RemoteDiskVolumeRoot, BOX_DISK_PRESSURE_SAMPLE_CAPABILITY,
};

/// Compose the dedicated box.store.sync Remote Runner service with the real bounded
/// filesystem/object-store backend. Android never calls this inside the APK; an explicitly
/// authorized external runner owns this service and its workspace roots.
pub fn open_production_box_store_sync_service<A>(
    journal_path: impl Into<std::path::PathBuf>,
    authorizer: A,
    backend_config: RemoteBoxStoreBackendConfig,
    now_ms: u64,
) -> Result<server::RemoteExecutionService<A, RemoteBoxStoreBackend>, ExecutionError>
where
    A: server::RemoteExecutionAuthorizer,
{
    let backend = RemoteBoxStoreBackend::open(backend_config)?;
    server::RemoteExecutionService::open(journal_path, authorizer, backend, now_ms)
}
pub use file_transfer_backend::{
    open_file_transfer_service, RemoteFileTransferBackend, RemoteFileTransferConfig,
    BOX_FILE_TRANSFER_CAPABILITY, MAX_TRANSFER_BYTES, TRANSFER_CHUNK_BYTES,
};
pub use transport::{
    AuthenticatedRemoteHttpTransport, RemoteBearerCredential, RemoteCancelOutcome,
    RemoteDispatchOutcome, RemoteExecutionContext, RemoteExecutionTransport,
    RemoteReconcileOutcome, RemoteTransportPolicy,
};

pub struct RemoteRunner<T: RemoteExecutionTransport> {
    transport: T,
}

impl<T: RemoteExecutionTransport> RemoteRunner<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn execute(
        &mut self,
        context: &RemoteExecutionContext,
        request: ExecutionRequest,
    ) -> Result<RemoteDispatchOutcome, ExecutionError> {
        request.validate()?;
        context.validate()?;
        self.transport.execute(context, &request)
    }

    pub fn reconcile(
        &mut self,
        context: &RemoteExecutionContext,
        operation_id: &str,
        request_id: &str,
    ) -> Result<RemoteReconcileOutcome, ExecutionError> {
        context.validate()?;
        if operation_id.trim().is_empty() || request_id.trim().is_empty() {
            return Err(ExecutionError::InvalidRequest(
                "remote reconciliation requires operation_id and request_id".into(),
            ));
        }
        self.transport.reconcile(context, operation_id, request_id)
    }

    pub fn cancel(
        &mut self,
        context: &RemoteExecutionContext,
        operation_id: &str,
        request_id: &str,
    ) -> Result<RemoteCancelOutcome, ExecutionError> {
        context.validate()?;
        if operation_id.trim().is_empty() || request_id.trim().is_empty() {
            return Err(ExecutionError::InvalidRequest(
                "remote cancellation requires operation_id and request_id".into(),
            ));
        }
        self.transport.cancel(context, operation_id, request_id)
    }

    pub fn into_transport(self) -> T {
        self.transport
    }
}

pub fn remote_viewer_ports() -> (u16, u16) {
    (
        fabushi_constants::sand_box::SAND_BOX_PRIMARY_NOVNC_PORT,
        fabushi_constants::sand_box::SAND_BOX_FORK_NOVNC_PORT,
    )
}

pub fn remote_viewer_url(
    proxy_base_url: &str,
    network_token: &str,
    session_token: Option<&str>,
    special_treatment: bool,
) -> String {
    fabushi_constants::sand_box::build_sand_box_no_vnc_url(
        proxy_base_url,
        network_token,
        session_token,
        special_treatment,
    )
}

#[cfg(test)]
mod constants_wiring_tests {
    use super::*;

    #[test]
    fn remote_runner_uses_canonical_box_viewer_contract() {
        assert_eq!(remote_viewer_ports(), (6080, 6081));
        let url = remote_viewer_url("https://proxy.example", "network", None, true);
        assert!(fabushi_constants::sand_box::is_sand_special_treatment_no_vnc_url(&url));
    }
}
