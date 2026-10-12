use fabushi_android_shared::{ExecutionError, ExecutionRequest};
use serde::Deserialize;

use crate::server::{
    RemoteBackendCancelOutcome, RemoteBackendReconcileOutcome, RemoteBackendStartOutcome,
    RemoteExecutionBackend, RemoteRequestIdentity,
};

pub const BROWSER_UA_RECONCILE_CAPABILITY: &str = "browser.ua.reconcile";
pub const BROWSER_UA_OWNER_STAMP_LENGTH: usize = 16;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserUaReconcileRequest {
    pub version: u32,
    pub generation: u64,
    #[serde(default)]
    pub owner_stamp: Option<String>,
    pub token_disabled: bool,
}

impl BrowserUaReconcileRequest {
    fn parse(request: &ExecutionRequest) -> Result<Self, ExecutionError> {
        if request.capability_id != BROWSER_UA_RECONCILE_CAPABILITY {
            return Err(ExecutionError::CapabilityUnavailable(format!(
                "unsupported remote capability {}",
                request.capability_id
            )));
        }
        let parsed: Self = serde_json::from_str(&request.input_json).map_err(|_| {
            ExecutionError::InvalidRequest("browser.ua.reconcile input JSON is malformed".into())
        })?;
        if parsed.version != 1 {
            return Err(ExecutionError::InvalidRequest(
                "unsupported browser.ua.reconcile wire version".into(),
            ));
        }
        if parsed.generation == 0 {
            return Err(ExecutionError::InvalidRequest(
                "browser.ua.reconcile generation must be positive".into(),
            ));
        }
        if let Some(stamp) = parsed.owner_stamp.as_deref() {
            if stamp.len() != BROWSER_UA_OWNER_STAMP_LENGTH
                || !stamp.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(ExecutionError::InvalidRequest(
                    "browser.ua.reconcile ownerStamp must be 16 hexadecimal characters".into(),
                ));
            }
        }
        Ok(parsed)
    }
}

pub trait BrowserUaReconcileExecutor: Send {
    fn start(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &BrowserUaReconcileRequest,
    ) -> Result<RemoteBackendStartOutcome, ExecutionError>;

    fn reconcile(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &BrowserUaReconcileRequest,
    ) -> Result<RemoteBackendReconcileOutcome, ExecutionError>;

    fn cancel(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &BrowserUaReconcileRequest,
    ) -> Result<RemoteBackendCancelOutcome, ExecutionError>;
}

/// Dedicated Remote Runner server boundary for Desktop BrowserUa parity.
///
/// The payload contains only the derived owner stamp and token-disable policy. Transport bearer
/// credentials remain authorization-only and can never be projected into the remote browser UA
/// contract. Applying the policy to the real browser remains the executor's responsibility.
pub struct BrowserUaReconcileBackend<E: BrowserUaReconcileExecutor> {
    executor: E,
}

impl<E: BrowserUaReconcileExecutor> BrowserUaReconcileBackend<E> {
    pub fn new(executor: E) -> Self {
        Self { executor }
    }

    pub fn into_executor(self) -> E {
        self.executor
    }
}

impl<E: BrowserUaReconcileExecutor> RemoteExecutionBackend for BrowserUaReconcileBackend<E> {
    fn start(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendStartOutcome, ExecutionError> {
        let request = BrowserUaReconcileRequest::parse(request)?;
        self.executor.start(identity, &request)
    }

    fn reconcile(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
        let request = BrowserUaReconcileRequest::parse(request)?;
        self.executor.reconcile(identity, &request)
    }

    fn cancel(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendCancelOutcome, ExecutionError> {
        let request = BrowserUaReconcileRequest::parse(request)?;
        self.executor.cancel(identity, &request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingExecutor {
        starts: Vec<BrowserUaReconcileRequest>,
        reconciles: Vec<BrowserUaReconcileRequest>,
        cancels: Vec<BrowserUaReconcileRequest>,
    }

    impl BrowserUaReconcileExecutor for RecordingExecutor {
        fn start(
            &mut self,
            _identity: &RemoteRequestIdentity,
            request: &BrowserUaReconcileRequest,
        ) -> Result<RemoteBackendStartOutcome, ExecutionError> {
            self.starts.push(request.clone());
            Ok(RemoteBackendStartOutcome::Completed {
                output_json: r#"{"applied":true}"#.into(),
            })
        }

        fn reconcile(
            &mut self,
            _identity: &RemoteRequestIdentity,
            request: &BrowserUaReconcileRequest,
        ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
            self.reconciles.push(request.clone());
            Ok(RemoteBackendReconcileOutcome::Pending)
        }

        fn cancel(
            &mut self,
            _identity: &RemoteRequestIdentity,
            request: &BrowserUaReconcileRequest,
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

    fn request() -> ExecutionRequest {
        ExecutionRequest {
            operation_id: "operation-1".into(),
            capability_id: BROWSER_UA_RECONCILE_CAPABILITY.into(),
            input_json: r#"{"version":1,"generation":8,"ownerStamp":"0123456789abcdef","tokenDisabled":true}"#.into(),
            timeout_ms: 30_000,
        }
    }

    #[test]
    fn derived_identity_and_kill_switch_use_dedicated_executor() {
        let mut backend = BrowserUaReconcileBackend::new(RecordingExecutor::default());
        let outcome = backend.start(&identity(), &request()).unwrap();
        assert!(matches!(outcome, RemoteBackendStartOutcome::Completed { .. }));
        assert_eq!(backend.executor.starts.len(), 1);
        assert_eq!(backend.executor.starts[0].generation, 8);
        assert_eq!(
            backend.executor.starts[0].owner_stamp.as_deref(),
            Some("0123456789abcdef")
        );
        assert!(backend.executor.starts[0].token_disabled);
    }

    #[test]
    fn bearer_or_generic_capability_never_enters_browser_ua_payload() {
        let mut backend = BrowserUaReconcileBackend::new(RecordingExecutor::default());
        let mut generic = request();
        generic.capability_id = "shell".into();
        assert!(matches!(
            backend.start(&identity(), &generic),
            Err(ExecutionError::CapabilityUnavailable(_))
        ));

        let mut bearer_payload = request();
        bearer_payload.input_json = r#"{"version":1,"generation":8,"ownerStamp":"0123456789abcdef","tokenDisabled":false,"accessToken":"secret"}"#.into();
        assert!(matches!(
            backend.start(&identity(), &bearer_payload),
            Err(ExecutionError::InvalidRequest(_))
        ));
        assert!(backend.executor.starts.is_empty());
    }

    #[test]
    fn malformed_version_generation_or_owner_stamp_fail_closed() {
        let mut backend = BrowserUaReconcileBackend::new(RecordingExecutor::default());
        for input in [
            r#"{"version":2,"generation":8,"ownerStamp":"0123456789abcdef","tokenDisabled":false}"#,
            r#"{"version":1,"generation":0,"ownerStamp":"0123456789abcdef","tokenDisabled":false}"#,
            r#"{"version":1,"generation":8,"ownerStamp":"not-hex","tokenDisabled":false}"#,
        ] {
            let mut invalid = request();
            invalid.input_json = input.into();
            assert!(matches!(
                backend.start(&identity(), &invalid),
                Err(ExecutionError::InvalidRequest(_))
            ));
        }
        assert!(backend.executor.starts.is_empty());
    }

    #[test]
    fn reconcile_and_cancel_reuse_exact_typed_contract() {
        let mut backend = BrowserUaReconcileBackend::new(RecordingExecutor::default());
        assert!(matches!(
            backend.reconcile(&identity(), &request()).unwrap(),
            RemoteBackendReconcileOutcome::Pending
        ));
        assert!(matches!(
            backend.cancel(&identity(), &request()).unwrap(),
            RemoteBackendCancelOutcome::Confirmed
        ));
        assert_eq!(backend.executor.reconciles.len(), 1);
        assert_eq!(backend.executor.cancels.len(), 1);
    }
}
