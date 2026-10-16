use fabushi_android_shared::{ExecutionError, ExecutionRequest};
use serde::Deserialize;

use crate::server::{
    RemoteBackendCancelOutcome, RemoteBackendReconcileOutcome, RemoteBackendStartOutcome,
    RemoteExecutionAuthorizer, RemoteExecutionBackend, RemoteExecutionService, RemoteRequestIdentity,
};

pub const BOX_STORE_SYNC_CAPABILITY: &str = "box.store.sync";
const MAX_STORE_ID_BYTES: usize = 512;
const MAX_ETAG_BYTES: usize = 2_048;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BoxStoreSyncAction {
    Hydrate,
    Snapshot,
    ChromeSessionStage,
    Vacuum,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BoxStoreSyncRequest {
    pub version: u32,
    pub action: BoxStoreSyncAction,
    pub store_id: String,
    #[serde(default)]
    pub base_manifest_etag: Option<String>,
}

impl BoxStoreSyncRequest {
    fn parse(request: &ExecutionRequest) -> Result<Self, ExecutionError> {
        if request.capability_id != BOX_STORE_SYNC_CAPABILITY {
            return Err(ExecutionError::CapabilityUnavailable(format!(
                "unsupported remote capability {}",
                request.capability_id
            )));
        }
        let parsed: Self = serde_json::from_str(&request.input_json).map_err(|_| {
            ExecutionError::InvalidRequest("box.store.sync input JSON is malformed".into())
        })?;
        if parsed.version != 1 {
            return Err(ExecutionError::InvalidRequest(
                "unsupported box.store.sync wire version".into(),
            ));
        }
        validate_bounded("store id", &parsed.store_id, MAX_STORE_ID_BYTES)?;
        if let Some(etag) = parsed.base_manifest_etag.as_deref() {
            validate_bounded("manifest etag", etag, MAX_ETAG_BYTES)?;
        }
        Ok(parsed)
    }
}

pub trait BoxStoreSyncExecutor: Send {
    fn start(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &BoxStoreSyncRequest,
    ) -> Result<RemoteBackendStartOutcome, ExecutionError>;

    fn reconcile(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &BoxStoreSyncRequest,
    ) -> Result<RemoteBackendReconcileOutcome, ExecutionError>;

    fn cancel(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &BoxStoreSyncRequest,
    ) -> Result<RemoteBackendCancelOutcome, ExecutionError>;
}

/// Remote Runner backend that accepts only the dedicated box.store.sync capability.
///
/// This type deliberately cannot fall through to shell/read/computer execution. The external
/// runner must explicitly provide a BoxStoreSyncExecutor that owns filesystem/object-store work.
pub struct BoxStoreSyncBackend<E: BoxStoreSyncExecutor> {
    executor: E,
}

impl<E: BoxStoreSyncExecutor> BoxStoreSyncBackend<E> {
    pub fn new(executor: E) -> Self {
        Self { executor }
    }

    pub fn into_executor(self) -> E {
        self.executor
    }
}

pub fn open_box_store_sync_service<A, E>(
    store_path: impl Into<std::path::PathBuf>,
    authorizer: A,
    executor: E,
    now_ms: u64,
) -> Result<RemoteExecutionService<A, BoxStoreSyncBackend<E>>, ExecutionError>
where
    A: RemoteExecutionAuthorizer,
    E: BoxStoreSyncExecutor,
{
    RemoteExecutionService::open(
        store_path,
        authorizer,
        BoxStoreSyncBackend::new(executor),
        now_ms,
    )
}

impl<E: BoxStoreSyncExecutor> RemoteExecutionBackend for BoxStoreSyncBackend<E> {
    fn start(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendStartOutcome, ExecutionError> {
        let request = BoxStoreSyncRequest::parse(request)?;
        self.executor.start(identity, &request)
    }

    fn reconcile(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
        _progress_json: Option<&str>,
    ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
        let request = BoxStoreSyncRequest::parse(request)?;
        self.executor.reconcile(identity, &request)
    }

    fn cancel(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
        _progress_json: Option<&str>,
    ) -> Result<RemoteBackendCancelOutcome, ExecutionError> {
        let request = BoxStoreSyncRequest::parse(request)?;
        self.executor.cancel(identity, &request)
    }
}

fn validate_bounded(label: &str, value: &str, max: usize) -> Result<(), ExecutionError> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(ExecutionError::InvalidRequest(format!(
            "box.store.sync {label} is invalid"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingExecutor {
        starts: Vec<BoxStoreSyncRequest>,
        reconciles: Vec<BoxStoreSyncRequest>,
        cancels: Vec<BoxStoreSyncRequest>,
    }

    impl BoxStoreSyncExecutor for RecordingExecutor {
        fn start(
            &mut self,
            _identity: &RemoteRequestIdentity,
            request: &BoxStoreSyncRequest,
        ) -> Result<RemoteBackendStartOutcome, ExecutionError> {
            self.starts.push(request.clone());
            Ok(RemoteBackendStartOutcome::Completed {
                output_json: r#"{"manifestEtag":"etag-2"}"#.into(),
            })
        }

        fn reconcile(
            &mut self,
            _identity: &RemoteRequestIdentity,
            request: &BoxStoreSyncRequest,
        ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
            self.reconciles.push(request.clone());
            Ok(RemoteBackendReconcileOutcome::Pending)
        }

        fn cancel(
            &mut self,
            _identity: &RemoteRequestIdentity,
            request: &BoxStoreSyncRequest,
        ) -> Result<RemoteBackendCancelOutcome, ExecutionError> {
            self.cancels.push(request.clone());
            Ok(RemoteBackendCancelOutcome::Confirmed)
        }
    }

    fn identity() -> RemoteRequestIdentity {
        RemoteRequestIdentity {
            credential_id: "credential-1".into(),
            operation_id: "operation-1".into(),
            request_id: "request-1".into(),
            account_fence: "account-a:epoch-7".into(),
            account_epoch: 7,
            permission_grant_id: "grant-1".into(),
            device_id: "device-1".into(),
        }
    }

    fn request(action: &str) -> ExecutionRequest {
        ExecutionRequest {
            operation_id: "operation-1".into(),
            capability_id: BOX_STORE_SYNC_CAPABILITY.into(),
            input_json: format!(
                r#"{{"version":1,"action":"{action}","storeId":"store-a","baseManifestEtag":"etag-1"}}"#
            ),
            timeout_ms: 120_000,
        }
    }

    #[test]
    fn all_declared_actions_use_the_dedicated_executor() {
        let mut backend = BoxStoreSyncBackend::new(RecordingExecutor::default());
        for action in ["hydrate", "snapshot", "chrome_session_stage", "vacuum"] {
            let outcome = backend.start(&identity(), &request(action)).unwrap();
            assert!(matches!(outcome, RemoteBackendStartOutcome::Completed { .. }));
        }
        assert_eq!(backend.executor.starts.len(), 4);
        assert_eq!(backend.executor.starts[0].action, BoxStoreSyncAction::Hydrate);
        assert_eq!(backend.executor.starts[1].action, BoxStoreSyncAction::Snapshot);
        assert_eq!(
            backend.executor.starts[2].action,
            BoxStoreSyncAction::ChromeSessionStage
        );
        assert_eq!(backend.executor.starts[3].action, BoxStoreSyncAction::Vacuum);
    }

    #[test]
    fn generic_shell_or_unknown_capability_never_reaches_box_store_executor() {
        let mut backend = BoxStoreSyncBackend::new(RecordingExecutor::default());
        let mut shell = request("snapshot");
        shell.capability_id = "shell".into();
        assert!(matches!(
            backend.start(&identity(), &shell),
            Err(ExecutionError::CapabilityUnavailable(_))
        ));
        assert!(backend.executor.starts.is_empty());
    }

    #[test]
    fn malformed_version_store_or_etag_fail_closed() {
        let mut backend = BoxStoreSyncBackend::new(RecordingExecutor::default());

        let mut wrong_version = request("snapshot");
        wrong_version.input_json =
            r#"{"version":2,"action":"snapshot","storeId":"store-a"}"#.into();
        assert!(matches!(
            backend.start(&identity(), &wrong_version),
            Err(ExecutionError::InvalidRequest(_))
        ));

        let mut empty_store = request("snapshot");
        empty_store.input_json =
            r#"{"version":1,"action":"snapshot","storeId":"   "}"#.into();
        assert!(matches!(
            backend.start(&identity(), &empty_store),
            Err(ExecutionError::InvalidRequest(_))
        ));

        let mut control_etag = request("snapshot");
        control_etag.input_json =
            "{\"version\":1,\"action\":\"snapshot\",\"storeId\":\"store-a\",\"baseManifestEtag\":\"bad\\nvalue\"}".into();
        assert!(matches!(
            backend.start(&identity(), &control_etag),
            Err(ExecutionError::InvalidRequest(_))
        ));
        assert!(backend.executor.starts.is_empty());
    }

    #[test]
    fn reconcile_and_cancel_use_the_same_typed_request_contract() {
        let mut backend = BoxStoreSyncBackend::new(RecordingExecutor::default());
        assert!(matches!(
            backend.reconcile(&identity(), &request("hydrate"), None).unwrap(),
            RemoteBackendReconcileOutcome::Pending
        ));
        assert!(matches!(
            backend.cancel(&identity(), &request("hydrate"), None).unwrap(),
            RemoteBackendCancelOutcome::Confirmed
        ));
        assert_eq!(backend.executor.reconciles.len(), 1);
        assert_eq!(backend.executor.cancels.len(), 1);
    }
}
