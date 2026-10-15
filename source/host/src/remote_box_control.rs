use crate::host_runner_composition::AuthenticatedRemoteHostRunner;
use fabushi_android_box_exec_daemon::{RemoteExecutionContext, RemoteExecutionTransport};
use fabushi_android_shared::{ExecutionError, ExecutionRequest, ExecutionResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

pub const REMOTE_BOX_ENVIRONMENT_CAPABILITY: &str = "box.environment.apply";
pub const REMOTE_BOX_MCP_CAPABILITY: &str = "box.mcp";
pub const REMOTE_BOX_ENVIRONMENT_EXECUTOR: &str = "box-environment";
pub const REMOTE_BOX_MCP_EXECUTOR: &str = "box-mcp";
pub const REMOTE_BOX_CONTROL_TIMEOUT_MS: u64 = 30_000;
pub const MCP_TOOL_EXEC_FIELD_NUMBER: u32 = 11;
pub const MCP_STATE_EXEC_FIELD_NUMBER: u32 = 36;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteBoxMcpBinding {
    context: RemoteExecutionContext,
}

impl RemoteBoxMcpBinding {
    pub fn new(context: &RemoteExecutionContext) -> Self {
        Self {
            context: context.clone(),
        }
    }

    pub fn context(&self) -> &RemoteExecutionContext {
        &self.context
    }

    pub fn execute<T: RemoteExecutionTransport>(
        &self,
        runner: &mut AuthenticatedRemoteHostRunner<T>,
        field_number: u32,
        payload_hex: &str,
    ) -> Result<Vec<u8>, ExecutionError> {
        execute_mcp_resource_via_authorized_runner(
            runner,
            &self.context,
            field_number,
            payload_hex,
        )
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentRequest<'a> {
    version: u32,
    update: &'a Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpLoadRequest<'a> {
    version: u32,
    action: &'static str,
    mcp_config_json: &'a str,
    remove_missing: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpExecRequest<'a> {
    version: u32,
    action: &'static str,
    field_number: u32,
    payload_hex: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpLoadResponse {
    #[serde(default)]
    loaded_server_names: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpExecResponse {
    payload_hex: String,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn request(
    context: &RemoteExecutionContext,
    capability_id: &str,
    input_json: String,
) -> Result<ExecutionRequest, ExecutionError> {
    context.validate()?;
    let request = ExecutionRequest {
        operation_id: context.operation_id.clone(),
        capability_id: capability_id.into(),
        input_json,
        timeout_ms: REMOTE_BOX_CONTROL_TIMEOUT_MS,
    };
    request.validate()?;
    Ok(request)
}

pub fn apply_environment_via_authorized_runner<T: RemoteExecutionTransport>(
    runner: &mut AuthenticatedRemoteHostRunner<T>,
    context: &RemoteExecutionContext,
    update_json: &str,
) -> Result<(), ExecutionError> {
    let update: Value = serde_json::from_str(update_json).map_err(|error| {
        ExecutionError::InvalidRequest(format!("invalid Box environment update: {error}"))
    })?;
    if !update.is_object() {
        return Err(ExecutionError::InvalidRequest(
            "Box environment update must be a JSON object".into(),
        ));
    }
    let input_json = serde_json::to_string(&EnvironmentRequest {
        version: 1,
        update: &update,
    })
    .map_err(|error| ExecutionError::InvalidRequest(error.to_string()))?;
    let request = request(context, REMOTE_BOX_ENVIRONMENT_CAPABILITY, input_json)?;
    runner.execute_authorized(context, &request, now_ms())?;
    Ok(())
}

pub fn load_mcp_servers_via_authorized_runner<T: RemoteExecutionTransport>(
    runner: &mut AuthenticatedRemoteHostRunner<T>,
    context: &RemoteExecutionContext,
    config_json: &str,
) -> Result<Vec<String>, ExecutionError> {
    let parsed: Value = serde_json::from_str(config_json).map_err(|error| {
        ExecutionError::InvalidRequest(format!("invalid Box MCP config: {error}"))
    })?;
    if !parsed.is_object() {
        return Err(ExecutionError::InvalidRequest(
            "Box MCP config must be a JSON object".into(),
        ));
    }
    let input_json = serde_json::to_string(&McpLoadRequest {
        version: 1,
        action: "load_servers",
        mcp_config_json: config_json,
        remove_missing: true,
    })
    .map_err(|error| ExecutionError::InvalidRequest(error.to_string()))?;
    let request = request(context, REMOTE_BOX_MCP_CAPABILITY, input_json)?;
    let result = runner.execute_authorized(context, &request, now_ms())?;
    let response: McpLoadResponse = parse_output(&result, "Box MCP load")?;
    Ok(response.loaded_server_names)
}

pub fn execute_mcp_resource_via_authorized_runner<T: RemoteExecutionTransport>(
    runner: &mut AuthenticatedRemoteHostRunner<T>,
    context: &RemoteExecutionContext,
    field_number: u32,
    payload_hex: &str,
) -> Result<Vec<u8>, ExecutionError> {
    if !matches!(field_number, MCP_TOOL_EXEC_FIELD_NUMBER | MCP_STATE_EXEC_FIELD_NUMBER) {
        return Err(ExecutionError::InvalidRequest(format!(
            "unsupported Box MCP ExecService field {field_number}"
        )));
    }
    validate_hex(payload_hex)?;
    let input_json = serde_json::to_string(&McpExecRequest {
        version: 1,
        action: "execute_raw",
        field_number,
        payload_hex,
    })
    .map_err(|error| ExecutionError::InvalidRequest(error.to_string()))?;
    let request = request(context, REMOTE_BOX_MCP_CAPABILITY, input_json)?;
    let result = runner.execute_authorized(context, &request, now_ms())?;
    let response: McpExecResponse = parse_output(&result, "Box MCP exec")?;
    decode_hex(&response.payload_hex)
}

fn parse_output<T: for<'de> Deserialize<'de>>(
    result: &ExecutionResult,
    label: &str,
) -> Result<T, ExecutionError> {
    serde_json::from_str(&result.output_json)
        .map_err(|error| ExecutionError::Transport(format!("{label} returned invalid JSON: {error}")))
}

fn validate_hex(raw: &str) -> Result<(), ExecutionError> {
    if raw.len() % 2 != 0
        || raw
            .as_bytes()
            .iter()
            .any(|byte| !byte.is_ascii_hexdigit())
    {
        return Err(ExecutionError::InvalidRequest(
            "Box MCP protobuf payload must be even-length hex".into(),
        ));
    }
    Ok(())
}

fn decode_hex(raw: &str) -> Result<Vec<u8>, ExecutionError> {
    validate_hex(raw)?;
    raw.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).unwrap();
            let low = (pair[1] as char).to_digit(16).unwrap();
            Ok(((high << 4) | low) as u8)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_mcp_resource_fields_fail_closed_outside_desktop_contract() {
        assert!(validate_hex("00ff").is_ok());
        assert!(validate_hex("0").is_err());
        assert!(validate_hex("zz").is_err());
        assert!(!matches!(10, MCP_TOOL_EXEC_FIELD_NUMBER | MCP_STATE_EXEC_FIELD_NUMBER));
        assert!(matches!(11, MCP_TOOL_EXEC_FIELD_NUMBER | MCP_STATE_EXEC_FIELD_NUMBER));
        assert!(matches!(36, MCP_TOOL_EXEC_FIELD_NUMBER | MCP_STATE_EXEC_FIELD_NUMBER));
    }
}

#[cfg(test)]
mod forwarding_tests {
    use super::*;
    use crate::remote_execution::RemoteExecutionJournal;
    use fabushi_android_box_exec_daemon::{
        RemoteBearerCredential, RemoteCancelOutcome, RemoteDispatchOutcome,
        RemoteReconcileOutcome,
    };
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct RecordingTransport {
        requests: Arc<Mutex<Vec<ExecutionRequest>>>,
        output_json: String,
    }

    impl RemoteExecutionTransport for RecordingTransport {
        fn execute(
            &mut self,
            _context: &RemoteExecutionContext,
            request: &ExecutionRequest,
        ) -> Result<RemoteDispatchOutcome, ExecutionError> {
            self.requests.lock().unwrap().push(request.clone());
            Ok(RemoteDispatchOutcome::Completed {
                ack_id: format!("ack-{}", request.operation_id),
                result: ExecutionResult {
                    operation_id: request.operation_id.clone(),
                    output_json: self.output_json.clone(),
                },
            })
        }

        fn reconcile(
            &mut self,
            _context: &RemoteExecutionContext,
            operation_id: &str,
            _request_id: &str,
        ) -> Result<RemoteReconcileOutcome, ExecutionError> {
            Ok(RemoteReconcileOutcome::Completed {
                ack_id: "ack-reconcile".into(),
                result: ExecutionResult {
                    operation_id: operation_id.into(),
                    output_json: self.output_json.clone(),
                },
            })
        }

        fn cancel(
            &mut self,
            _context: &RemoteExecutionContext,
            _operation_id: &str,
            _request_id: &str,
        ) -> Result<RemoteCancelOutcome, ExecutionError> {
            Ok(RemoteCancelOutcome::Confirmed {
                ack_id: Some("ack-cancel".into()),
            })
        }
    }

    fn context(operation_id: &str) -> RemoteExecutionContext {
        RemoteExecutionContext {
            credential_id: "credential-1".into(),
            bearer: RemoteBearerCredential::new("remote-test-token-long-enough").unwrap(),
            account_fence: "session:account-a".into(),
            account_epoch: 7,
            operation_id: operation_id.into(),
            request_id: format!("request-{operation_id}"),
            permission_grant_id: format!("grant-{operation_id}"),
            device_id: "device-1".into(),
        }
    }

    fn runner(
        root: &std::path::Path,
        label: &str,
        output_json: &str,
        requests: Arc<Mutex<Vec<ExecutionRequest>>>,
    ) -> AuthenticatedRemoteHostRunner<RecordingTransport> {
        let journal = RemoteExecutionJournal::open(root.join(format!("{label}.json")), 1).unwrap();
        AuthenticatedRemoteHostRunner::new(
            RecordingTransport {
                requests,
                output_json: output_json.into(),
            },
            journal,
        )
    }

    #[test]
    fn environment_forwarding_uses_fenced_remote_execution_not_local_mutation() {
        let root = tempfile::tempdir().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mut remote = runner(root.path(), "env", "{}", Arc::clone(&requests));
        let ctx = context("env-op");
        apply_environment_via_authorized_runner(
            &mut remote,
            &ctx,
            r#"{"FOO":"bar"}"#,
        )
        .unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].capability_id, REMOTE_BOX_ENVIRONMENT_CAPABILITY);
        let env_input: serde_json::Value = serde_json::from_str(&requests[0].input_json).unwrap();
        assert_eq!(env_input["update"]["FOO"], "bar");
        assert!(remote.journal().record("env-op").is_some());
    }

    #[test]
    fn mcp_load_and_raw_exec_preserve_remove_missing_and_desktop_fields() {
        let root = tempfile::tempdir().unwrap();
        let load_requests = Arc::new(Mutex::new(Vec::new()));
        let mut load_runner = runner(
            root.path(),
            "load",
            r#"{"loadedServerNames":["calendar"]}"#,
            Arc::clone(&load_requests),
        );
        let loaded = load_mcp_servers_via_authorized_runner(
            &mut load_runner,
            &context("mcp-load"),
            r#"{"mcpServers":{}}"#,
        )
        .unwrap();
        assert_eq!(loaded, vec!["calendar"]);
        let load_requests = load_requests.lock().unwrap();
        assert_eq!(load_requests[0].capability_id, REMOTE_BOX_MCP_CAPABILITY);
        let load_input: serde_json::Value =
            serde_json::from_str(&load_requests[0].input_json).unwrap();
        assert_eq!(load_input["removeMissing"], true);

        let exec_requests = Arc::new(Mutex::new(Vec::new()));
        let mut exec_runner = runner(
            root.path(),
            "exec",
            r#"{"payloadHex":"0a00"}"#,
            Arc::clone(&exec_requests),
        );
        let bytes = execute_mcp_resource_via_authorized_runner(
            &mut exec_runner,
            &context("mcp-exec"),
            MCP_STATE_EXEC_FIELD_NUMBER,
            "1200",
        )
        .unwrap();
        assert_eq!(bytes, vec![0x0a, 0x00]);
        let exec_requests = exec_requests.lock().unwrap();
        let exec_input: serde_json::Value =
            serde_json::from_str(&exec_requests[0].input_json).unwrap();
        assert_eq!(exec_input["fieldNumber"], MCP_STATE_EXEC_FIELD_NUMBER);
        assert_eq!(exec_input["payloadHex"], "1200");
        assert!(execute_mcp_resource_via_authorized_runner(
            &mut exec_runner,
            &context("mcp-invalid"),
            10,
            "00",
        )
        .is_err());
    }
}
