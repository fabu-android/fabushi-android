use std::collections::BTreeMap;

use fabushi_android_shared::{ExecutionError, ExecutionRequest};
use serde::Deserialize;

use crate::server::{
    RemoteBackendCancelOutcome, RemoteBackendReconcileOutcome, RemoteBackendStartOutcome,
    RemoteExecutionAuthorizer, RemoteExecutionBackend, RemoteExecutionService, RemoteRequestIdentity,
};

pub const CLOUD_AGENT_CAPABILITY: &str = "cloud-agent";
const MAX_TEXT_BYTES: usize = 65_536;
const MAX_ID_BYTES: usize = 512;
const MAX_IMAGE_URL_BYTES: usize = 4_096;
const MAX_MODEL_PARAMS: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloudAgentAction {
    Launch,
    List,
    Models,
    Get,
    Dump,
    Watch,
    Reply,
    Rename,
    Cancel,
    Archive,
    Unarchive,
    Delete,
    ListArtifacts,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CloudAgentImageRef {
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CloudAgentEnvironmentInput {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub team_id: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CloudAgentRequest {
    pub action: CloudAgentAction,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub images: Vec<CloudAgentImageRef>,
    #[serde(default)]
    pub repo_url: Option<String>,
    #[serde(default)]
    pub starting_ref: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub model_params: BTreeMap<String, String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub environment: Option<CloudAgentEnvironmentInput>,
    #[serde(default)]
    pub interrupt: Option<bool>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub include_archived: Option<bool>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub confirm: Option<bool>,
}

impl CloudAgentRequest {
    fn parse(request: &ExecutionRequest) -> Result<Self, ExecutionError> {
        if request.capability_id != CLOUD_AGENT_CAPABILITY {
            return Err(ExecutionError::CapabilityUnavailable(format!(
                "unsupported remote capability {}",
                request.capability_id
            )));
        }
        let parsed: Self = serde_json::from_str(&request.input_json).map_err(|error| {
            ExecutionError::InvalidRequest(format!("invalid CloudAgent input: {error}"))
        })?;
        parsed.validate()?;
        Ok(parsed)
    }

    fn validate(&self) -> Result<(), ExecutionError> {
        for (label, value, max) in [
            ("prompt", self.prompt.as_deref(), MAX_TEXT_BYTES),
            ("repo_url", self.repo_url.as_deref(), MAX_TEXT_BYTES),
            ("starting_ref", self.starting_ref.as_deref(), MAX_TEXT_BYTES),
            ("model", self.model.as_deref(), MAX_TEXT_BYTES),
            ("title", self.title.as_deref(), MAX_TEXT_BYTES),
            ("agent_id", self.agent_id.as_deref(), MAX_ID_BYTES),
        ] {
            if let Some(value) = value {
                if value.len() > max || value.chars().any(|character| character == '\0') {
                    return Err(ExecutionError::InvalidRequest(format!(
                        "CloudAgent {label} is invalid or too large"
                    )));
                }
            }
        }

        if self.model_params.len() > MAX_MODEL_PARAMS {
            return Err(ExecutionError::InvalidRequest(
                "CloudAgent model_params exceed the limit".into(),
            ));
        }
        if !self.model_params.is_empty()
            && self
                .model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
        {
            return Err(ExecutionError::InvalidRequest(
                "CloudAgent model is required when model_params are provided".into(),
            ));
        }
        for (key, value) in &self.model_params {
            if key.trim().is_empty()
                || key.len() > MAX_ID_BYTES
                || key.chars().any(char::is_control)
                || value.len() > MAX_IMAGE_URL_BYTES
                || value.chars().any(char::is_control)
            {
                return Err(ExecutionError::InvalidRequest(
                    "CloudAgent model_params contain an invalid id or value".into(),
                ));
            }
        }

        for image in &self.images {
            if image.url.len() > MAX_IMAGE_URL_BYTES {
                return Err(ExecutionError::InvalidRequest(
                    "CloudAgent image URL is too large".into(),
                ));
            }
            let parsed = url::Url::parse(image.url.trim()).map_err(|_| {
                ExecutionError::InvalidRequest("CloudAgent image URL is invalid".into())
            })?;
            if parsed.scheme() != "file" || parsed.to_file_path().is_err() {
                return Err(ExecutionError::InvalidRequest(
                    "CloudAgent images must use absolute file:// URLs".into(),
                ));
            }
        }

        if let Some(scope) = self.scope.as_deref() {
            if !matches!(scope, "launched" | "all") {
                return Err(ExecutionError::InvalidRequest(
                    "CloudAgent scope is unsupported".into(),
                ));
            }
        }
        if self.limit == Some(0) {
            return Err(ExecutionError::InvalidRequest(
                "CloudAgent limit must be positive".into(),
            ));
        }
        if let Some(environment) = &self.environment {
            if !matches!(
                environment.kind.as_str(),
                "cloud" | "pool" | "machine" | "environment"
            ) {
                return Err(ExecutionError::InvalidRequest(
                    "CloudAgent environment type is unsupported".into(),
                ));
            }
            if environment.kind == "machine"
                && environment
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_none()
            {
                return Err(ExecutionError::InvalidRequest(
                    "CloudAgent machine environment requires name".into(),
                ));
            }
            if environment.kind == "environment"
                && environment
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_none()
                && environment
                    .id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_none()
            {
                return Err(ExecutionError::InvalidRequest(
                    "CloudAgent saved environment requires name or id".into(),
                ));
            }
        }

        match self.action {
            CloudAgentAction::Launch => {
                require_text(self.prompt.as_deref(), "launch prompt", MAX_TEXT_BYTES)?;
            }
            CloudAgentAction::Reply => {
                require_identity(self.agent_id.as_deref(), "reply agent_id")?;
                require_text(self.prompt.as_deref(), "reply prompt", MAX_TEXT_BYTES)?;
            }
            CloudAgentAction::Get
            | CloudAgentAction::Dump
            | CloudAgentAction::Watch
            | CloudAgentAction::Cancel
            | CloudAgentAction::Archive
            | CloudAgentAction::Unarchive
            | CloudAgentAction::Delete
            | CloudAgentAction::ListArtifacts => {
                require_identity(self.agent_id.as_deref(), "agent_id")?;
            }
            CloudAgentAction::Rename => {
                require_identity(self.agent_id.as_deref(), "rename agent_id")?;
                require_identity(self.title.as_deref(), "rename title")?;
            }
            CloudAgentAction::List | CloudAgentAction::Models => {}
        }
        Ok(())
    }
}

fn require_identity(value: Option<&str>, label: &str) -> Result<(), ExecutionError> {
    let value = value.map(str::trim).filter(|value| !value.is_empty()).ok_or_else(|| {
        ExecutionError::InvalidRequest(format!("CloudAgent {label} is required"))
    })?;
    if value.len() > MAX_ID_BYTES || value.chars().any(char::is_control) {
        return Err(ExecutionError::InvalidRequest(format!(
            "CloudAgent {label} is invalid"
        )));
    }
    Ok(())
}

fn require_text(
    value: Option<&str>,
    label: &str,
    max: usize,
) -> Result<(), ExecutionError> {
    let value = value.map(str::trim).filter(|value| !value.is_empty()).ok_or_else(|| {
        ExecutionError::InvalidRequest(format!("CloudAgent {label} is required"))
    })?;
    if value.len() > max || value.chars().any(|character| character == '\0') {
        return Err(ExecutionError::InvalidRequest(format!(
            "CloudAgent {label} is invalid"
        )));
    }
    Ok(())
}

pub trait CloudAgentExecutor: Send {
    fn start(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &CloudAgentRequest,
    ) -> Result<RemoteBackendStartOutcome, ExecutionError>;

    fn reconcile(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &CloudAgentRequest,
    ) -> Result<RemoteBackendReconcileOutcome, ExecutionError>;

    fn cancel(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &CloudAgentRequest,
    ) -> Result<RemoteBackendCancelOutcome, ExecutionError>;
}

/// Dedicated external-Runner Cloud Agent boundary.
///
/// The Android Host can request CloudAgent work only through the authenticated RemoteExecution
/// service. The executor owns the real backend protocol and scoped file reads; no CloudAgent
/// operation can silently fall through to shell/read execution.
pub struct CloudAgentBackend<E: CloudAgentExecutor> {
    executor: E,
}

impl<E: CloudAgentExecutor> CloudAgentBackend<E> {
    pub fn new(executor: E) -> Self {
        Self { executor }
    }

    pub fn into_executor(self) -> E {
        self.executor
    }
}

impl<E: CloudAgentExecutor> RemoteExecutionBackend for CloudAgentBackend<E> {
    fn start(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendStartOutcome, ExecutionError> {
        let parsed = CloudAgentRequest::parse(request)?;
        self.executor.start(identity, &parsed)
    }

    fn reconcile(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
        let parsed = CloudAgentRequest::parse(request)?;
        self.executor.reconcile(identity, &parsed)
    }

    fn cancel(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendCancelOutcome, ExecutionError> {
        let parsed = CloudAgentRequest::parse(request)?;
        self.executor.cancel(identity, &parsed)
    }
}

pub fn open_cloud_agent_service<A, E>(
    journal_path: impl Into<std::path::PathBuf>,
    authorizer: A,
    executor: E,
    now_ms: u64,
) -> Result<RemoteExecutionService<A, CloudAgentBackend<E>>, ExecutionError>
where
    A: RemoteExecutionAuthorizer,
    E: CloudAgentExecutor,
{
    RemoteExecutionService::open(
        journal_path,
        authorizer,
        CloudAgentBackend::new(executor),
        now_ms,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingExecutor {
        starts: Vec<CloudAgentRequest>,
    }

    impl CloudAgentExecutor for RecordingExecutor {
        fn start(
            &mut self,
            _identity: &RemoteRequestIdentity,
            request: &CloudAgentRequest,
        ) -> Result<RemoteBackendStartOutcome, ExecutionError> {
            self.starts.push(request.clone());
            Ok(RemoteBackendStartOutcome::Completed {
                output_json: r#"{"ok":true}"#.into(),
            })
        }

        fn reconcile(
            &mut self,
            _identity: &RemoteRequestIdentity,
            _request: &CloudAgentRequest,
        ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
            Ok(RemoteBackendReconcileOutcome::Pending)
        }

        fn cancel(
            &mut self,
            _identity: &RemoteRequestIdentity,
            _request: &CloudAgentRequest,
        ) -> Result<RemoteBackendCancelOutcome, ExecutionError> {
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

    fn request(value: serde_json::Value) -> ExecutionRequest {
        ExecutionRequest {
            operation_id: "operation-1".into(),
            capability_id: CLOUD_AGENT_CAPABILITY.into(),
            input_json: value.to_string(),
            timeout_ms: 120_000,
        }
    }

    #[test]
    fn typed_backend_accepts_models_launch_reply_and_dump_contracts() {
        let mut backend = CloudAgentBackend::new(RecordingExecutor::default());
        for input in [
            serde_json::json!({"action":"models"}),
            serde_json::json!({"action":"launch","prompt":"fix CI","model":"grok-4.5","model_params":{"fast":"true"},"images":[{"url":"file:///workspace/a.png"}]}),
            serde_json::json!({"action":"reply","agent_id":"bc-1","prompt":"continue"}),
            serde_json::json!({"action":"dump","agent_id":"bc-1"}),
        ] {
            assert!(matches!(
                backend.start(&identity(), &request(input)).unwrap(),
                RemoteBackendStartOutcome::Completed { .. }
            ));
        }
        assert_eq!(backend.executor.starts.len(), 4);
    }

    #[test]
    fn generic_capability_https_image_and_params_without_model_fail_closed() {
        let mut backend = CloudAgentBackend::new(RecordingExecutor::default());
        let mut generic = request(serde_json::json!({"action":"models"}));
        generic.capability_id = "shell".into();
        assert!(matches!(
            backend.start(&identity(), &generic),
            Err(ExecutionError::CapabilityUnavailable(_))
        ));
        assert!(backend
            .start(
                &identity(),
                &request(serde_json::json!({
                    "action":"launch",
                    "prompt":"x",
                    "images":[{"url":"https://example.com/a.png"}]
                }))
            )
            .is_err());
        assert!(backend
            .start(
                &identity(),
                &request(serde_json::json!({
                    "action":"launch",
                    "prompt":"x",
                    "model_params":{"fast":"true"}
                }))
            )
            .is_err());
        assert!(backend.executor.starts.is_empty());
    }

    #[test]
    fn unknown_fields_and_missing_action_identity_fail_closed() {
        let mut backend = CloudAgentBackend::new(RecordingExecutor::default());
        for input in [
            serde_json::json!({"action":"launch","prompt":"x","accessToken":"secret"}),
            serde_json::json!({"action":"reply","prompt":"continue"}),
            serde_json::json!({"action":"rename","agent_id":"bc-1"}),
            serde_json::json!({"action":"destroy"}),
        ] {
            assert!(backend.start(&identity(), &request(input)).is_err());
        }
        assert!(backend.executor.starts.is_empty());
    }
}
