use super::AndroidRoutedToolBridge;
use crate::android_user_settings::{AndroidUserSettings, LocalToolPermission};
use crate::capability_broker::{PendingCapabilityCall, SharedCapabilityBroker};
use crate::extensions::telemetry::local_exec_telemetry::{
    local_exec_failed_telemetry, LocalExecFailedReport,
};
use fabushi_android_local_exec_daemon::failure_classifier::classify_local_exec_failure;
use crate::sha256::sha256_hex;
use fabushi_android_local_exec_daemon::{
    bounded_file_handlers::{
        LOCAL_FILE_READ_CAPABILITY, LOCAL_FILE_WRITE_CAPABILITY, MAX_LOCAL_TOOL_READ_BYTES,
    },
    AndroidLocalRunner,
};
use fabushi_android_shared::{ExecutionError, ExecutionRequest};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const APPROVAL_WAIT_TIMEOUT: Duration = Duration::from_secs(120);
const APPROVAL_POLL: Duration = Duration::from_millis(250);
const LOCAL_EXEC_TIMEOUT_MS: u64 = 10_000;
const MAX_LOCAL_WRITE_CONTENT: usize = 48 * 1024;
const MAX_LOCAL_PATH: usize = 4096;

#[derive(Clone)]
struct LocalApprovalWait {
    local_operation_id: String,
    request_id: String,
    capability: String,
    account_fence: String,
    signal: Arc<(Mutex<Option<bool>>, Condvar)>,
}

#[derive(Clone, Default)]
pub(crate) struct LocalApprovalRegistry {
    waits: Arc<Mutex<BTreeMap<String, LocalApprovalWait>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LocalApprovalResolution {
    pub(crate) local_operation_id: String,
    pub(crate) capability: String,
    pub(crate) approved: bool,
}

impl LocalApprovalRegistry {
    pub(crate) fn is_pending_for_account(
        &self,
        approval_id: &str,
        current_account_fence: &str,
    ) -> Result<bool, String> {
        let waits = self
            .waits
            .lock()
            .map_err(|_| "local approval registry lock poisoned".to_string())?;
        Ok(waits
            .get(approval_id)
            .is_some_and(|wait| wait.account_fence == current_account_fence))
    }

    pub(crate) fn pending_ids_for_account(
        &self,
        current_account_fence: &str,
    ) -> Result<Vec<String>, String> {
        let waits = self
            .waits
            .lock()
            .map_err(|_| "local approval registry lock poisoned".to_string())?;
        Ok(waits
            .iter()
            .filter_map(|(approval_id, wait)| {
                (wait.account_fence == current_account_fence).then(|| approval_id.clone())
            })
            .collect())
    }

    pub(crate) fn resolve_from_ui(
        &self,
        broker: &SharedCapabilityBroker,
        approval_id: &str,
        approved: bool,
        current_account_fence: &str,
        now_ms: u64,
    ) -> Result<Option<LocalApprovalResolution>, String> {
        let wait = self
            .waits
            .lock()
            .map_err(|_| "local approval registry lock poisoned".to_string())?
            .get(approval_id)
            .cloned();
        let Some(wait) = wait else {
            return Ok(None);
        };
        if wait.account_fence != current_account_fence {
            return Err("local approval callback is fenced by account identity".into());
        }
        let resolved = broker.resolve_approval(
            approval_id,
            approved,
            current_account_fence,
            now_ms,
        )?;
        if resolved.operation_id != wait.local_operation_id
            || resolved.request_id != wait.request_id
            || resolved.capability != wait.capability
        {
            return Err("local approval callback identity disagrees with durable broker state".into());
        }
        let (state, condvar) = &*wait.signal;
        let mut state = state
            .lock()
            .map_err(|_| "local approval signal lock poisoned".to_string())?;
        if state.is_some() {
            return Err("local approval callback is duplicate or stale".into());
        }
        *state = Some(approved);
        condvar.notify_all();
        Ok(Some(LocalApprovalResolution {
            local_operation_id: wait.local_operation_id,
            capability: wait.capability,
            approved,
        }))
    }

    pub(crate) fn clear_from_ui(
        &self,
        approval_id: &str,
        expected_operation_id: &str,
        current_account_fence: &str,
    ) -> Result<Option<LocalApprovalResolution>, String> {
        let wait = self
            .waits
            .lock()
            .map_err(|_| "local approval registry lock poisoned".to_string())?
            .get(approval_id)
            .cloned();
        let Some(wait) = wait else {
            return Ok(None);
        };
        if wait.account_fence != current_account_fence {
            return Err("local approval cleanup is fenced by account identity".into());
        }
        if wait.local_operation_id != expected_operation_id {
            return Err("local approval cleanup operation identity disagrees with waiter".into());
        }
        let (state, condvar) = &*wait.signal;
        let mut state = state
            .lock()
            .map_err(|_| "local approval signal lock poisoned".to_string())?;
        if state.is_some() {
            return Err("local approval cleanup raced a terminal resolution".into());
        }
        *state = Some(false);
        condvar.notify_all();
        Ok(Some(LocalApprovalResolution {
            local_operation_id: wait.local_operation_id,
            capability: wait.capability,
            approved: false,
        }))
    }

    fn register(&self, approval_id: &str, wait: LocalApprovalWait) -> Result<(), String> {
        let mut waits = self
            .waits
            .lock()
            .map_err(|_| "local approval registry lock poisoned".to_string())?;
        if waits.contains_key(approval_id) {
            return Err("local approval waiter identity collision".into());
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

pub(crate) struct LocalRoutedTools {
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    broker: SharedCapabilityBroker,
    runner: Arc<Mutex<AndroidLocalRunner>>,
    approvals: LocalApprovalRegistry,
    user_settings: Arc<Mutex<AndroidUserSettings>>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    events: Arc<Mutex<VecDeque<Value>>>,
    parent_operation_id: String,
    parent_request_id: String,
    conversation_id: String,
    runtime_generation: u64,
    cancelled: Arc<AtomicBool>,
}

impl LocalRoutedTools {
    fn report_execution_failure(&self, message: &str) {
        let classified = classify_local_exec_failure(message);
        let projection = local_exec_failed_telemetry(&LocalExecFailedReport {
            error_class: classified.error_class.as_str().to_string(),
            errno: classified.errno,
            site: "android-local".into(),
            conversation_id: self.conversation_id.clone(),
        });
        if let Some(event) = projection.event {
            if let Ok(account_fence) = self.current_account_fence() {
                if let Ok(mut events) = self.events.lock() {
                    events.push_back(json!({
                        "type":"internal.local-exec.telemetry",
                        "accountFence":account_fence,
                        "level":projection.level.unwrap_or("warn"),
                        "event":event,
                        "metadata":projection.metadata,
                    }));
                }
            }
        }
    }
}

impl AndroidRoutedToolBridge for LocalRoutedTools {
    fn list_tools(&self) -> Result<Vec<Value>, String> {
        let mut tools = self.delegate.list_tools()?;
        if self.local_tool_permission()? == LocalToolPermission::Never {
            return Ok(tools);
        }
        tools.retain(|tool| {
            !matches!(
                tool.get("name").and_then(Value::as_str),
                Some("Read" | "Write")
            )
        });
        tools.push(json!({
            "type":"function",
            "name":"Read",
            "description":"Read bounded UTF-8 text inside Fabushi Android app-owned local storage. The Host Capability Broker enforces the current local-tool permission and one-time approval policy.",
            "parameters":{
                "type":"object",
                "additionalProperties":false,
                "required":["path"],
                "properties":{
                    "path":{"type":"string","minLength":1,"maxLength":MAX_LOCAL_PATH},
                    "offset":{"type":"integer","minimum":0},
                    "limit":{"type":"integer","minimum":1,"maximum":MAX_LOCAL_TOOL_READ_BYTES}
                }
            }
        }));
        tools.push(json!({
            "type":"function",
            "name":"Write",
            "description":"Atomically write bounded UTF-8 text inside Fabushi Android app-owned local storage. The Host Capability Broker enforces the current local-tool permission and one-time approval policy.",
            "parameters":{
                "type":"object",
                "additionalProperties":false,
                "required":["path","content"],
                "properties":{
                    "path":{"type":"string","minLength":1,"maxLength":MAX_LOCAL_PATH},
                    "content":{"type":"string","maxLength":MAX_LOCAL_WRITE_CONTENT}
                }
            }
        }));
        Ok(tools)
    }

    fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String> {
        let capability = match name {
            "Read" => LOCAL_FILE_READ_CAPABILITY,
            "Write" => LOCAL_FILE_WRITE_CAPABILITY,
            _ => return self.delegate.call_tool(name, args, tool_call_id),
        };
        validate_local_tool_input(name, &args)?;
        if self.cancelled.load(Ordering::Acquire) {
            return Err("local file tool dispatch cancelled before authorization".into());
        }
        let account_fence = self.current_account_fence()?;
        let permission = self.local_tool_permission()?;
        if permission == LocalToolPermission::Never {
            return Err("local file tools are disabled by the current Host permission policy".into());
        }

        let identity = sha256_hex(
            format!(
                "{}\n{}\n{}\n{}",
                self.parent_operation_id, self.parent_request_id, tool_call_id, capability
            )
            .as_bytes(),
        );
        let operation_id = format!("local-{}", &identity[..32]);
        let request_id = format!("local-request-{}", &identity[..32]);
        let approval_id = format!("approval-local-{}", &identity[..32]);

        if permission == LocalToolPermission::Always {
            let decision = self.broker.authorize(
                &request_id,
                "host.local-runner",
                capability,
                name,
                &account_fence,
                self.runtime_generation.max(1),
                true,
                true,
                now_ms(),
            )?;
            if decision != crate::capability_broker::CapabilityDecision::Allow {
                return Err("persisted local file permission was not authorized".into());
            }
        } else {
            let signal = Arc::new((Mutex::new(None), Condvar::new()));
            self.broker.request_approval(
                &approval_id,
                &request_id,
                &operation_id,
                capability,
                json!({"tool":name,"arguments":args}),
                &account_fence,
                now_ms(),
            )?;
            self.approvals.register(
                &approval_id,
                LocalApprovalWait {
                    local_operation_id: operation_id.clone(),
                    request_id: request_id.clone(),
                    capability: capability.into(),
                    account_fence: account_fence.clone(),
                    signal: Arc::clone(&signal),
                },
            )?;
            if let Ok(mut events) = self.events.lock() {
                events.push_back(json!({
                    "type":"approval.requested",
                    "approvalId":approval_id,
                    "operationId":operation_id,
                    "parentOperationId":self.parent_operation_id,
                    "capability":capability,
                    "target":{"executor":"android-local","tool":name},
                    "accountFence":account_fence,
                    "reason":"Agent requested bounded app-local file access"
                }));
            }

            let started = Instant::now();
            let approved = loop {
                if self.cancelled.load(Ordering::Acquire) {
                    let _ = self.broker.cancel_approval_operation(
                        &operation_id,
                        "parent turn cancelled before local dispatch",
                        now_ms(),
                    );
                    self.approvals.remove(&approval_id);
                    return Err("local file tool dispatch cancelled before authorization".into());
                }
                let (state, condvar) = &*signal;
                let state = state
                    .lock()
                    .map_err(|_| "local approval signal lock poisoned".to_string())?;
                if let Some(approved) = *state {
                    break approved;
                }
                if started.elapsed() >= APPROVAL_WAIT_TIMEOUT {
                    drop(state);
                    let _ = self.broker.cancel_approval_operation(
                        &operation_id,
                        "local approval timed out before dispatch",
                        now_ms(),
                    );
                    self.approvals.remove(&approval_id);
                    return Err("local file approval timed out before dispatch".into());
                }
                let _ = condvar
                    .wait_timeout(state, APPROVAL_POLL)
                    .map_err(|_| "local approval signal lock poisoned".to_string())?;
            };
            self.approvals.remove(&approval_id);
            if !approved {
                return Err("local file tool dispatch denied by user".into());
            }
            let current_fence = self.current_account_fence()?;
            if current_fence != account_fence {
                return Err("local file approval fenced by account switch".into());
            }
            if self.local_tool_permission()? == LocalToolPermission::Never {
                return Err("local file permission was revoked before dispatch".into());
            }
            self.broker.consume_approval_for_dispatch(
                &approval_id,
                &operation_id,
                &request_id,
                capability,
                &account_fence,
                now_ms(),
            )?;
        }

        if self.cancelled.load(Ordering::Acquire) {
            return Err("local file tool dispatch cancelled before side effect".into());
        }
        let started_at = now_ms();
        self.broker.begin(PendingCapabilityCall {
            request_id: request_id.clone(),
            plugin_id: "host.local-runner".into(),
            capability: capability.into(),
            tool: name.into(),
            arguments: args.clone(),
            required_permissions: BTreeSet::new(),
            account_fence: account_fence.clone(),
            runtime_generation: self.runtime_generation.max(1),
            started_at_ms: started_at,
            deadline_at_ms: started_at.saturating_add(LOCAL_EXEC_TIMEOUT_MS),
            state: "pending".into(),
        })?;
        let request = ExecutionRequest {
            operation_id,
            capability_id: capability.into(),
            input_json: serde_json::to_string(&args)
                .map_err(|error| format!("failed to encode local file arguments: {error}"))?,
            timeout_ms: LOCAL_EXEC_TIMEOUT_MS,
        };
        let result = self
            .runner
            .lock()
            .map_err(|_| "Android Local Runner lock poisoned".to_string())?
            .execute(request);
        match result {
            Ok(result) => {
                result
                    .validate()
                    .map_err(local_execution_error)?;
                self.broker
                    .settle(&request_id, "completed", None, now_ms())?;
                serde_json::from_str(&result.output_json)
                    .map_err(|error| format!("invalid Local Runner result JSON: {error}"))
            }
            Err(error) => {
                let message = local_execution_error(error);
                let _ = self
                    .broker
                    .settle(&request_id, "failed", Some(message.clone()), now_ms());
                self.report_execution_failure(&message);
                Err(message)
            }
        }
    }
}

pub(crate) fn with_local_file_tools(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    broker: SharedCapabilityBroker,
    runner: Arc<Mutex<AndroidLocalRunner>>,
    approvals: LocalApprovalRegistry,
    user_settings: Arc<Mutex<AndroidUserSettings>>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    events: Arc<Mutex<VecDeque<Value>>>,
    parent_operation_id: &str,
    parent_request_id: &str,
    conversation_id: &str,
    runtime_generation: u64,
    cancelled: Arc<AtomicBool>,
) -> Arc<dyn AndroidRoutedToolBridge> {
    Arc::new(LocalRoutedTools {
        delegate,
        broker,
        runner,
        approvals,
        user_settings,
        live_account_fence,
        events,
        parent_operation_id: parent_operation_id.to_string(),
        parent_request_id: parent_request_id.to_string(),
        conversation_id: conversation_id.to_string(),
        runtime_generation,
        cancelled,
    })
}

impl LocalRoutedTools {
    fn local_tool_permission(&self) -> Result<LocalToolPermission, String> {
        let account_fence = self.current_account_fence()?;
        self.user_settings
            .lock()
            .map_err(|_| "Android user settings lock poisoned".to_string())?
            .local_tool_permission_for(&account_fence)
            .map_err(|error| format!("failed to read Android local tool permission: {error}"))
    }

    fn current_account_fence(&self) -> Result<String, String> {
        self.live_account_fence
            .lock()
            .map_err(|_| "live account fence lock poisoned".to_string())?
            .clone()
            .ok_or("local file tool requires an active account fence".into())
    }
}

fn validate_local_tool_input(name: &str, args: &Value) -> Result<(), String> {
    let object = args
        .as_object()
        .ok_or("local file tool arguments must be an object")?;
    let path = object
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("local file path is required")?;
    if path.len() > MAX_LOCAL_PATH || path.contains('\0') {
        return Err("local file path is invalid or too large".into());
    }
    match name {
        "Read" => {
            const KEYS: &[&str] = &["path", "offset", "limit"];
            if let Some(key) = object.keys().find(|key| !KEYS.contains(&key.as_str())) {
                return Err(format!("local Read argument is unsupported: {key}"));
            }
            if object.get("offset").is_some_and(|value| value.as_u64().is_none()) {
                return Err("local Read offset must be a non-negative integer".into());
            }
            if let Some(limit) = object.get("limit") {
                let limit = limit
                    .as_u64()
                    .ok_or("local Read limit must be a positive integer")?;
                if limit == 0 || limit > MAX_LOCAL_TOOL_READ_BYTES as u64 {
                    return Err("local Read limit is outside the bounded range".into());
                }
            }
        }
        "Write" => {
            const KEYS: &[&str] = &["path", "content"];
            if let Some(key) = object.keys().find(|key| !KEYS.contains(&key.as_str())) {
                return Err(format!("local Write argument is unsupported: {key}"));
            }
            let content = object
                .get("content")
                .and_then(Value::as_str)
                .ok_or("local Write content must be a string")?;
            if content.len() > MAX_LOCAL_WRITE_CONTENT {
                return Err("local Write content exceeds the bounded tool payload".into());
            }
        }
        _ => return Err("unsupported local file tool".into()),
    }
    Ok(())
}

fn local_execution_error(error: ExecutionError) -> String {
    match error {
        ExecutionError::InvalidRequest(message) => format!("local_invalid_request:{message}"),
        ExecutionError::CapabilityUnavailable(message) => {
            format!("local_capability_unavailable:{message}")
        }
        ExecutionError::Cancelled => "local_cancelled".into(),
        ExecutionError::TimedOut => "local_timed_out".into(),
        ExecutionError::Transport(message) => format!("local_transport:{message}"),
    }
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
    use fabushi_android_local_exec_daemon::bounded_file_handlers::bounded_file_runner;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct EmptyBridge;
    impl AndroidRoutedToolBridge for EmptyBridge {
        fn list_tools(&self) -> Result<Vec<Value>, String> {
            Ok(Vec::new())
        }
        fn call_tool(&self, _name: &str, _args: Value, _tool_call_id: &str) -> Result<Value, String> {
            Err("delegate unavailable".into())
        }
    }

    fn root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-local-routed-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn never_permission_hides_local_file_tools() {
        let root = root("never");
        std::fs::create_dir_all(&root).unwrap();
        let settings_path = root.join("settings.json");
        let mut settings = AndroidUserSettings::open(&settings_path).unwrap();
        settings.set_local_tool_permission(LocalToolPermission::Never).unwrap();
        let broker = SharedCapabilityBroker::open(root.join("broker.json"), now_ms()).unwrap();
        let bridge = with_local_file_tools(
            Arc::new(EmptyBridge),
            broker,
            Arc::new(Mutex::new(bounded_file_runner(root.join("files")).unwrap())),
            LocalApprovalRegistry::default(),
            Arc::new(Mutex::new(settings)),
            Arc::new(Mutex::new(Some("acct".into()))),
            Arc::new(Mutex::new(VecDeque::new())),
            "parent-op",
            "parent-request",
            "conversation-1",
            1,
            Arc::new(AtomicBool::new(false)),
        );
        assert!(bridge.list_tools().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn runner_failure_emits_bounded_account_fenced_telemetry() {
        let root = root("telemetry-failure");
        std::fs::create_dir_all(&root).unwrap();
        let settings_path = root.join("settings.json");
        let mut settings = AndroidUserSettings::open(&settings_path).unwrap();
        settings.set_local_tool_permission(LocalToolPermission::Always).unwrap();
        let broker = SharedCapabilityBroker::open(root.join("broker.json"), now_ms()).unwrap();
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let bridge = with_local_file_tools(
            Arc::new(EmptyBridge),
            broker,
            Arc::new(Mutex::new(bounded_file_runner(root.join("files")).unwrap())),
            LocalApprovalRegistry::default(),
            Arc::new(Mutex::new(settings)),
            Arc::new(Mutex::new(Some("acct".into()))),
            Arc::clone(&events),
            "parent-op",
            "parent-request",
            "conversation-telemetry",
            3,
            Arc::new(AtomicBool::new(false)),
        );

        let error = bridge
            .call_tool("Read", json!({"path":"missing-secret.txt"}), "read-missing")
            .unwrap_err();
        assert!(error.starts_with("local_invalid_request:"));

        let event = events.lock().unwrap().pop_front().expect("local exec telemetry");
        assert_eq!(event["type"], "internal.local-exec.telemetry");
        assert_eq!(event["accountFence"], "acct");
        assert_eq!(event["event"], "sand.local_exec.exec_failed");
        assert_eq!(event["metadata"]["conversation_id"], "conversation-telemetry");
        assert_eq!(event["metadata"]["site"], "android-local");
        assert_eq!(event["metadata"]["error_class"], "other");
        assert!(
            !event.to_string().contains("missing-secret.txt"),
            "telemetry must not include local path or tool arguments"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn always_permission_executes_only_through_broker_and_runner() {
        let root = root("always");
        std::fs::create_dir_all(&root).unwrap();
        let settings_path = root.join("settings.json");
        let mut settings = AndroidUserSettings::open(&settings_path).unwrap();
        settings.set_local_tool_permission(LocalToolPermission::Always).unwrap();
        let broker = SharedCapabilityBroker::open(root.join("broker.json"), now_ms()).unwrap();
        let bridge = with_local_file_tools(
            Arc::new(EmptyBridge),
            broker,
            Arc::new(Mutex::new(bounded_file_runner(root.join("files")).unwrap())),
            LocalApprovalRegistry::default(),
            Arc::new(Mutex::new(settings)),
            Arc::new(Mutex::new(Some("acct".into()))),
            Arc::new(Mutex::new(VecDeque::new())),
            "parent-op",
            "parent-request",
            "conversation-1",
            7,
            Arc::new(AtomicBool::new(false)),
        );
        bridge
            .call_tool(
                "Write",
                json!({"path":"notes.txt","content":"hello"}),
                "write-call",
            )
            .unwrap();
        let read = bridge
            .call_tool("Read", json!({"path":"notes.txt"}), "read-call")
            .unwrap();
        assert_eq!(read["content"], "hello");
        let _ = std::fs::remove_dir_all(root);
    }
}
