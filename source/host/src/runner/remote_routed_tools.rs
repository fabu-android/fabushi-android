use super::AndroidRoutedToolBridge;
use crate::capability_broker::SharedCapabilityBroker;
use crate::host_runner_composition::AuthenticatedRemoteHostRunner;
use crate::remote_execution::RemoteExecutionState;
use crate::sha256::sha256_hex;
use fabushi_android_box_exec_daemon::{
    AuthenticatedRemoteHttpTransport, RemoteBearerCredential, RemoteExecutionContext,
    RemoteTransportPolicy,
};
use fabushi_android_shared::{ExecutionError, ExecutionRequest};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const APPROVAL_WAIT_TIMEOUT: Duration = Duration::from_secs(120);
const APPROVAL_POLL: Duration = Duration::from_millis(250);
const MAX_SHELL_COMMAND: usize = 32 * 1024;
const MAX_READ_PATH: usize = 4096;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RemoteDispatchBinding {
    pub(crate) endpoint: String,
    pub(crate) bearer_credential: String,
    pub(crate) device_id: String,
    pub(crate) account_fence: String,
    pub(crate) account_epoch: u64,
    #[serde(default)]
    pub(crate) has_desktop: bool,
}

impl RemoteDispatchBinding {
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        let binding: Self = serde_json::from_str(raw)
            .map_err(|error| format!("invalid protected remote binding: {error}"))?;
        binding.validate()?;
        Ok(binding)
    }

    fn validate(&self) -> Result<(), String> {
        if self.account_epoch == 0 {
            return Err("remote binding account epoch must be positive".into());
        }
        for (label, value) in [
            ("remote device", self.device_id.as_str()),
            ("remote account fence", self.account_fence.as_str()),
        ] {
            if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
                return Err(format!("{label} identity is invalid"));
            }
        }
        RemoteBearerCredential::new(self.bearer_credential.clone())
            .map_err(|error| format!("remote credential rejected: {error:?}"))?;
        AuthenticatedRemoteHttpTransport::new(&self.endpoint, RemoteTransportPolicy::default())
            .map_err(|error| format!("remote endpoint rejected: {error:?}"))?;
        Ok(())
    }

    fn context(
        &self,
        operation_id: &str,
        request_id: &str,
        approval_id: &str,
    ) -> Result<RemoteExecutionContext, String> {
        Ok(RemoteExecutionContext {
            bearer: RemoteBearerCredential::new(self.bearer_credential.clone())
                .map_err(|error| format!("remote credential rejected: {error:?}"))?,
            account_fence: self.account_fence.clone(),
            account_epoch: self.account_epoch,
            operation_id: operation_id.to_string(),
            request_id: request_id.to_string(),
            permission_grant_id: approval_id.to_string(),
            device_id: self.device_id.clone(),
        })
    }
}

#[derive(Clone)]
struct RemoteApprovalWait {
    parent_operation_id: String,
    remote_operation_id: String,
    request_id: String,
    capability: String,
    account_fence: String,
    account_epoch: u64,
    device_id: String,
    signal: Arc<(Mutex<Option<bool>>, Condvar)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemoteApprovalResolution {
    pub(crate) parent_operation_id: String,
    pub(crate) remote_operation_id: String,
    pub(crate) capability: String,
    pub(crate) approved: bool,
}

#[derive(Clone, Default)]
pub(crate) struct RemoteApprovalRegistry {
    waits: Arc<Mutex<BTreeMap<String, RemoteApprovalWait>>>,
}

impl RemoteApprovalRegistry {
    pub(crate) fn resolve_from_ui(
        &self,
        broker: &SharedCapabilityBroker,
        approval_id: &str,
        approved: bool,
        current_account_fence: &str,
        now_ms: u64,
    ) -> Result<Option<RemoteApprovalResolution>, String> {
        let wait = self
            .waits
            .lock()
            .map_err(|_| "remote approval registry lock poisoned".to_string())?
            .get(approval_id)
            .cloned();
        let Some(wait) = wait else {
            return Ok(None);
        };
        if wait.account_fence != current_account_fence {
            return Err("remote approval callback is fenced by account identity".into());
        }
        let resolved = broker.resolve_approval(
            approval_id,
            approved,
            current_account_fence,
            now_ms,
        )?;
        if resolved.operation_id != wait.remote_operation_id
            || resolved.request_id != wait.request_id
            || resolved.capability != wait.capability
            || resolved.account_epoch != Some(wait.account_epoch)
            || resolved.device_id.as_deref() != Some(wait.device_id.as_str())
        {
            return Err("remote approval callback identity disagrees with durable broker state".into());
        }
        let (state, condvar) = &*wait.signal;
        let mut state = state
            .lock()
            .map_err(|_| "remote approval signal lock poisoned".to_string())?;
        if state.is_some() {
            return Err("remote approval callback is duplicate or stale".into());
        }
        *state = Some(approved);
        condvar.notify_all();
        Ok(Some(RemoteApprovalResolution {
            parent_operation_id: wait.parent_operation_id,
            remote_operation_id: wait.remote_operation_id,
            capability: wait.capability,
            approved,
        }))
    }

    fn register(&self, approval_id: &str, wait: RemoteApprovalWait) -> Result<(), String> {
        let mut waits = self
            .waits
            .lock()
            .map_err(|_| "remote approval registry lock poisoned".to_string())?;
        if waits.contains_key(approval_id) {
            return Err("remote approval waiter identity collision".into());
        }
        waits.insert(approval_id.to_string(), wait);
        Ok(())
    }

    fn remove(&self, approval_id: &str) {
        if let Ok(mut waits) = self.waits.lock() {
            waits.remove(approval_id);
        }
    }
}

pub(crate) struct RemoteRoutedTools {
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    broker: SharedCapabilityBroker,
    binding: Arc<Mutex<Option<RemoteDispatchBinding>>>,
    runner: Arc<Mutex<Option<AuthenticatedRemoteHostRunner<AuthenticatedRemoteHttpTransport>>>>,
    approvals: RemoteApprovalRegistry,
    live_account_fence: Arc<Mutex<Option<String>>>,
    events: Arc<Mutex<VecDeque<Value>>>,
    parent_operation_id: String,
    parent_request_id: String,
    cancelled: Arc<AtomicBool>,
}

impl AndroidRoutedToolBridge for RemoteRoutedTools {
    fn list_tools(&self) -> Result<Vec<Value>, String> {
        let mut tools = self.delegate.list_tools()?;
        if self.current_binding().is_err() {
            return Ok(tools);
        }
        tools.retain(|tool| {
            !matches!(
                tool.get("name").and_then(Value::as_str),
                Some("Shell") | Some("Read")
            )
        });
        tools.push(json!({
            "type":"function",
            "name":"Shell",
            "description":"Run a bounded shell command on the currently paired trusted Remote desktop. Requires one-time user approval before dispatch.",
            "parameters":{
                "type":"object",
                "additionalProperties":false,
                "required":["command"],
                "properties":{
                    "command":{"type":"string"},
                    "timeoutMs":{"type":"integer","minimum":1000,"maximum":120000}
                }
            }
        }));
        tools.push(json!({
            "type":"function",
            "name":"Read",
            "description":"Read a bounded file path on the currently paired trusted Remote desktop. Requires one-time user approval before dispatch.",
            "parameters":{
                "type":"object",
                "additionalProperties":false,
                "required":["path"],
                "properties":{
                    "path":{"type":"string"},
                    "offset":{"type":"integer","minimum":0},
                    "limit":{"type":"integer","minimum":1,"maximum":1048576}
                }
            }
        }));
        Ok(tools)
    }

    fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String> {
        if name != "Shell" && name != "Read" {
            return self.delegate.call_tool(name, args, tool_call_id);
        }
        if self.cancelled.load(Ordering::Acquire) {
            return Err("remote tool dispatch cancelled before authorization".into());
        }
        let timeout_ms = validate_remote_tool_input(name, &args)?;
        let args_json = serde_json::to_string(&args).map_err(|error| error.to_string())?;
        let identity_hash = sha256_hex(
            format!(
                "{}\n{}\n{}\n{}",
                self.parent_operation_id, tool_call_id, name, args_json
            )
            .as_bytes(),
        );
        let request_hash = sha256_hex(
            format!("{}\n{}\n{}", self.parent_request_id, tool_call_id, identity_hash).as_bytes(),
        );
        let operation_id = format!("remote-{identity_hash}");
        let request_id = format!("remote-request-{request_hash}");
        let approval_id = format!("remote-approval-{identity_hash}");
        let capability = if name == "Shell" { "remote.shell" } else { "remote.read" };
        let binding = self.current_binding()?;
        let context = binding.context(&operation_id, &request_id, &approval_id)?;
        let request = ExecutionRequest {
            operation_id: operation_id.clone(),
            capability_id: capability.to_string(),
            input_json: json!({"tool":name,"arguments":args}).to_string(),
            timeout_ms,
        };

        // Stable tool-call identity resumes terminal/unknown durable Remote state without
        // requesting or consuming a second approval grant.
        {
            let mut guard = self
                .runner
                .lock()
                .map_err(|_| "remote runner lock poisoned".to_string())?;
            let runner = guard.as_mut().ok_or("trusted Remote runner is unavailable")?;
            if let Some(record) = runner.journal().record(&operation_id).cloned() {
                let result = match record.state {
                    RemoteExecutionState::Completed => runner
                        .execute_authorized(&context, &request, now_ms())
                        .map_err(remote_execution_error)?,
                    RemoteExecutionState::OutcomeUnknown
                    | RemoteExecutionState::Sent
                    | RemoteExecutionState::Acked => runner
                        .reconcile_authorized(&context, now_ms())
                        .map_err(remote_execution_error)?,
                    RemoteExecutionState::Pending => {
                        return Err("remote_pending_dispatch_requires_original_owner".into())
                    }
                    RemoteExecutionState::Rejected => return Err("remote_execution_rejected".into()),
                    RemoteExecutionState::Cancelled => return Err("remote_execution_cancelled".into()),
                };
                return Ok(parse_remote_output(&result.output_json));
            }
        }

        self.broker.request_remote_approval(
            &approval_id,
            &request_id,
            &operation_id,
            capability,
            json!({
                "deviceId":binding.device_id,
                "tool":name,
                "arguments":args,
                "inputIdentity":identity_hash,
            }),
            &binding.account_fence,
            binding.account_epoch,
            &binding.device_id,
            now_ms(),
        )?;

        let signal = Arc::new((Mutex::new(None), Condvar::new()));
        self.approvals.register(
            &approval_id,
            RemoteApprovalWait {
                parent_operation_id: self.parent_operation_id.clone(),
                remote_operation_id: operation_id.clone(),
                request_id: request_id.clone(),
                capability: capability.to_string(),
                account_fence: binding.account_fence.clone(),
                account_epoch: binding.account_epoch,
                device_id: binding.device_id.clone(),
                signal: Arc::clone(&signal),
            },
        )?;
        push_event(
            &self.events,
            json!({
                "type":"approval.requested",
                "approvalId":approval_id,
                "operationId":operation_id,
                "parentOperationId":self.parent_operation_id,
                "requestId":request_id,
                "capability":capability,
                "target":{
                    "deviceId":binding.device_id,
                    "tool":name,
                    "arguments":args,
                },
                "accountFence":binding.account_fence,
                "accountEpoch":binding.account_epoch,
                "deviceId":binding.device_id,
                "remoteDispatch":true,
            }),
        )?;

        let started = Instant::now();
        let approved = loop {
            if self.cancelled.load(Ordering::Acquire) {
                let _ = self.broker.cancel_approval_operation(
                    &operation_id,
                    "parent turn cancelled before Remote dispatch",
                    now_ms(),
                );
                self.approvals.remove(&approval_id);
                return Err("remote tool dispatch cancelled before authorization".into());
            }
            let (state, condvar) = &*signal;
            let state = state
                .lock()
                .map_err(|_| "remote approval signal lock poisoned".to_string())?;
            if let Some(approved) = *state {
                break approved;
            }
            if started.elapsed() >= APPROVAL_WAIT_TIMEOUT {
                drop(state);
                let _ = self.broker.cancel_approval_operation(
                    &operation_id,
                    "remote approval timed out before dispatch",
                    now_ms(),
                );
                self.approvals.remove(&approval_id);
                return Err("remote approval timed out before dispatch".into());
            }
            let _ = condvar
                .wait_timeout(state, APPROVAL_POLL)
                .map_err(|_| "remote approval signal lock poisoned".to_string())?;
        };
        self.approvals.remove(&approval_id);
        if !approved {
            return Err("remote tool dispatch denied by user".into());
        }

        // Freeze exact protected binding across approval. Endpoint/credential/device/account
        // rotation after the card was shown invalidates this dispatch instead of silently
        // changing its execution target.
        let current_binding = self.current_binding()?;
        if current_binding != binding {
            return Err("trusted Remote binding changed after approval".into());
        }
        // Persist a never-sent Remote operation before consuming the one-time grant. A process
        // death before mark_sent can then be proven not to have crossed the Remote boundary.
        {
            let mut guard = self
                .runner
                .lock()
                .map_err(|_| "remote runner lock poisoned".to_string())?;
            let runner = guard.as_mut().ok_or("trusted Remote runner is unavailable")?;
            runner
                .prepare_authorized(&context, &request, now_ms())
                .map_err(remote_execution_error)?;
        }

        if let Err(error) = self.broker.consume_remote_approval_for_dispatch(
            &approval_id,
            &operation_id,
            &request_id,
            capability,
            &binding.account_fence,
            binding.account_epoch,
            &binding.device_id,
            now_ms(),
        ) {
            if let Ok(mut guard) = self.runner.lock() {
                if let Some(runner) = guard.as_mut() {
                    let _ = runner.cancel_prepared_authorized(&context, now_ms());
                }
            }
            return Err(error);
        }
        if self.cancelled.load(Ordering::Acquire) {
            if let Ok(mut guard) = self.runner.lock() {
                if let Some(runner) = guard.as_mut() {
                    let _ = runner.cancel_prepared_authorized(&context, now_ms());
                }
            }
            return Err("remote tool dispatch cancelled before side effect".into());
        }

        let mut guard = self
            .runner
            .lock()
            .map_err(|_| "remote runner lock poisoned".to_string())?;
        let runner = guard.as_mut().ok_or("trusted Remote runner is unavailable")?;
        match runner.dispatch_prepared_authorized(&context, &request, now_ms()) {
            Ok(result) => Ok(parse_remote_output(&result.output_json)),
            Err(ExecutionError::Transport(message))
                if message.contains("remote_outcome_unknown") =>
            {
                Err(format!("{message}; explicit reconciliation required"))
            }
            Err(error) => Err(remote_execution_error(error)),
        }
    }
}

impl RemoteRoutedTools {
    fn current_binding(&self) -> Result<RemoteDispatchBinding, String> {
        let binding = self
            .binding
            .lock()
            .map_err(|_| "remote binding lock poisoned".to_string())?
            .clone()
            .ok_or("trusted Remote binding is unavailable")?;
        let current_fence = self
            .live_account_fence
            .lock()
            .map_err(|_| "live account fence lock poisoned".to_string())?
            .clone();
        if current_fence.as_deref() != Some(binding.account_fence.as_str()) {
            return Err("trusted Remote binding is fenced by current account identity".into());
        }
        Ok(binding)
    }
}

pub(crate) fn with_remote_routed_tools(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    broker: SharedCapabilityBroker,
    binding: Arc<Mutex<Option<RemoteDispatchBinding>>>,
    runner: Arc<Mutex<Option<AuthenticatedRemoteHostRunner<AuthenticatedRemoteHttpTransport>>>>,
    approvals: RemoteApprovalRegistry,
    live_account_fence: Arc<Mutex<Option<String>>>,
    events: Arc<Mutex<VecDeque<Value>>>,
    parent_operation_id: &str,
    parent_request_id: &str,
    cancelled: Arc<AtomicBool>,
) -> Arc<dyn AndroidRoutedToolBridge> {
    Arc::new(RemoteRoutedTools {
        delegate,
        broker,
        binding,
        runner,
        approvals,
        live_account_fence,
        events,
        parent_operation_id: parent_operation_id.to_string(),
        parent_request_id: parent_request_id.to_string(),
        cancelled,
    })
}

fn validate_remote_tool_input(name: &str, args: &Value) -> Result<u64, String> {
    let object = args.as_object().ok_or("remote tool arguments must be an object")?;
    match name {
        "Shell" => {
            let command = object
                .get("command")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or("Remote Shell command is required")?;
            if command.len() > MAX_SHELL_COMMAND || command.chars().any(|c| c == '\0') {
                return Err("Remote Shell command is invalid or too large".into());
            }
            let timeout = object.get("timeoutMs").and_then(Value::as_u64).unwrap_or(30_000);
            if !(1_000..=120_000).contains(&timeout) {
                return Err("Remote Shell timeoutMs is outside the bounded range".into());
            }
            Ok(timeout)
        }
        "Read" => {
            let path = object
                .get("path")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or("Remote Read path is required")?;
            if path.len() > MAX_READ_PATH || path.chars().any(|c| c == '\0') {
                return Err("Remote Read path is invalid or too large".into());
            }
            if object.get("offset").is_some_and(|value| value.as_u64().is_none()) {
                return Err("Remote Read offset must be a non-negative integer".into());
            }
            if let Some(limit) = object.get("limit") {
                let limit = limit.as_u64().ok_or("Remote Read limit must be a positive integer")?;
                if !(1..=1_048_576).contains(&limit) {
                    return Err("Remote Read limit is outside the bounded range".into());
                }
            }
            Ok(30_000)
        }
        _ => Err("unsupported Remote routed tool".into()),
    }
}

fn parse_remote_output(output: &str) -> Value {
    serde_json::from_str(output).unwrap_or_else(|_| Value::String(output.to_string()))
}

fn remote_execution_error(error: ExecutionError) -> String {
    format!("{error:?}")
}

fn push_event(events: &Arc<Mutex<VecDeque<Value>>>, event: Value) -> Result<(), String> {
    events
        .lock()
        .map_err(|_| "turn event queue lock poisoned".to_string())?
        .push_back(event);
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_rejects_wrong_epoch_and_untrusted_plaintext_endpoint() {
        let invalid_epoch = json!({
            "endpoint":"https://remote.example.com",
            "bearerCredential":"long-enough-credential",
            "deviceId":"device-1",
            "accountFence":"session:a",
            "accountEpoch":0,
            "hasDesktop":true
        }).to_string();
        assert!(RemoteDispatchBinding::parse(&invalid_epoch).is_err());

        let plaintext = json!({
            "endpoint":"http://remote.example.com",
            "bearerCredential":"long-enough-credential",
            "deviceId":"device-1",
            "accountFence":"session:a",
            "accountEpoch":7,
            "hasDesktop":true
        }).to_string();
        assert!(RemoteDispatchBinding::parse(&plaintext).is_err());
    }

    #[test]
    fn remote_approval_callback_rejects_wrong_account_and_duplicate_resolution() {
        let root = tempfile::tempdir().unwrap();
        let broker = SharedCapabilityBroker::open(root.path().join("broker.json"), 1).unwrap();
        broker
            .request_remote_approval(
                "approval-1",
                "request-1",
                "remote-op-1",
                "remote.shell",
                json!({"deviceId":"device-1"}),
                "session:account-a",
                7,
                "device-1",
                1,
            )
            .unwrap();
        let registry = RemoteApprovalRegistry::default();
        registry
            .register(
                "approval-1",
                RemoteApprovalWait {
                    parent_operation_id: "parent-op".into(),
                    remote_operation_id: "remote-op-1".into(),
                    request_id: "request-1".into(),
                    capability: "remote.shell".into(),
                    account_fence: "session:account-a".into(),
                    account_epoch: 7,
                    device_id: "device-1".into(),
                    signal: Arc::new((Mutex::new(None), Condvar::new())),
                },
            )
            .unwrap();

        assert!(registry
            .resolve_from_ui(&broker, "approval-1", true, "session:account-b", 2)
            .is_err());
        let resolved = registry
            .resolve_from_ui(&broker, "approval-1", true, "session:account-a", 3)
            .unwrap()
            .unwrap();
        assert!(resolved.approved);
        assert!(registry
            .resolve_from_ui(&broker, "approval-1", true, "session:account-a", 4)
            .is_err());
    }

    #[test]
    fn remote_tool_identity_changes_when_arguments_change() {
        let first = sha256_hex(b"parent\ncall\nShell\n{\"command\":\"one\"}");
        let second = sha256_hex(b"parent\ncall\nShell\n{\"command\":\"two\"}");
        assert_ne!(first, second);
    }
}
