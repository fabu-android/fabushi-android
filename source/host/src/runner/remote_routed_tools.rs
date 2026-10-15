use super::AndroidRoutedToolBridge;
use crate::android_user_settings::{AndroidUserSettings, LocalToolPermission};
use crate::box_readiness::remote_box_not_ready_message;
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
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// Desktop SAND_LOCAL_TOOL_ASK_TTL_MS is 10 minutes. Keep Android's authorized Remote
// replacement at the same user-visible ask lifetime; cancellation/account fencing still aborts sooner.
const APPROVAL_WAIT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const APPROVAL_POLL: Duration = Duration::from_millis(250);
const MAX_SHELL_COMMAND: usize = 32 * 1024;
const MAX_READ_PATH: usize = 4096;
const MAX_BROWSER_UPLOAD_PATH: usize = 4096;
const MAX_WRITE_CONTENT: usize = 1024 * 1024;
const COPY_TO_BOX_TOOL_NAME: &str = "CopyToBox";
const COPY_FROM_BOX_TOOL_NAME: &str = "CopyFromBox";
const REMOTE_BROWSER_EXECUTION_TIMEOUT_MS: u64 = 90_000;
const REMOTE_CLOUD_AGENT_EXECUTION_TIMEOUT_MS: u64 = 120_000;
const REMOTE_REFUSED_ACTION_MEMORY: usize = 512;
const CLOUD_AGENT_TOOL_NAME: &str = "CloudAgent";
const CLOUD_AGENT_ACTIONS: &[&str] = &[
    "launch", "list", "models", "get", "dump", "watch", "reply", "rename",
    "cancel", "archive", "unarchive", "delete", "list_artifacts",
];
const CLOUD_AGENT_DESTRUCTIVE_ACTIONS: &[&str] = &["cancel", "archive", "unarchive", "delete"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemoteBrowserNavigationObservation {
    pub operation_id: String,
    pub url: String,
    pub page_title: String,
}

pub(crate) type RemoteBrowserNavigationObserver =
    Arc<dyn Fn(RemoteBrowserNavigationObservation) + Send + Sync + 'static>;

const REMOTE_BROWSER_TOOL_NAMES: &[&str] = &[
    "browser_navigate",
    "browser_refresh",
    "browser_back",
    "browser_forward",
    "browser_snapshot",
    "browser_click",
    "browser_mouse_click_xy",
    "browser_download",
    "browser_upload_file",
    "browser_type",
    "browser_fill",
    "browser_select_option",
    "browser_press_key",
    "browser_scroll",
    "browser_drag",
    "browser_get_bounding_box",
    "browser_highlight",
    "browser_cdp",
    "browser_tabs",
    "browser_take_screenshot",
];

#[derive(Clone, Copy)]
struct RemoteBrowserToolSpec {
    name: &'static str,
    description: &'static str,
    required: &'static [&'static str],
}

fn remote_browser_specs() -> Vec<RemoteBrowserToolSpec> {
    vec![
        RemoteBrowserToolSpec { name: "browser_navigate", description: "Navigate the box browser to a URL. By default reuses your tab; set newTab: true to open in a new tab. Returns the resulting page state with a screenshot.", required: &["url"] },
        RemoteBrowserToolSpec { name: "browser_refresh", description: "Refresh the exact claimed browser tab. Requires viewId plus the current expectedGeneration and returns a new generation.", required: &["viewId", "expectedGeneration"] },
        RemoteBrowserToolSpec { name: "browser_back", description: "Navigate the exact claimed browser tab backward in history. Requires viewId plus the current expectedGeneration and returns a new generation.", required: &["viewId", "expectedGeneration"] },
        RemoteBrowserToolSpec { name: "browser_forward", description: "Navigate the exact claimed browser tab forward in history. Requires viewId plus the current expectedGeneration and returns a new generation.", required: &["viewId", "expectedGeneration"] },
        RemoteBrowserToolSpec { name: "browser_snapshot", description: "Capture a structured snapshot of the current page with ref handles for interactive elements.", required: &[] },
        RemoteBrowserToolSpec { name: "browser_click", description: "Click an element by ref from browser_snapshot.", required: &["ref"] },
        RemoteBrowserToolSpec { name: "browser_mouse_click_xy", description: "Click at viewport coordinates.", required: &["x", "y"] },
        RemoteBrowserToolSpec { name: "browser_download", description: "Click an element and track the resulting download into the authorized Remote Runner workspace download area with bounded size and SHA-256 evidence.", required: &["ref"] },
        RemoteBrowserToolSpec { name: "browser_upload_file", description: "Upload one explicitly authorized regular file from the Remote Runner /workspace root into a file input.", required: &["ref", "filePath"] },
        RemoteBrowserToolSpec { name: "browser_type", description: "Type text into an editable element by ref.", required: &["ref", "text"] },
        RemoteBrowserToolSpec { name: "browser_fill", description: "Set the value of an editable element by ref.", required: &["ref", "value"] },
        RemoteBrowserToolSpec { name: "browser_select_option", description: "Select one or more options in a select element by ref.", required: &["ref", "values"] },
        RemoteBrowserToolSpec { name: "browser_press_key", description: "Press a key in the browser page.", required: &["key"] },
        RemoteBrowserToolSpec { name: "browser_scroll", description: "Scroll the page or scroll an element into view.", required: &[] },
        RemoteBrowserToolSpec { name: "browser_drag", description: "Drag an element by ref to another ref or viewport coordinates.", required: &["sourceRef"] },
        RemoteBrowserToolSpec { name: "browser_get_bounding_box", description: "Get the viewport bounding box for an element ref.", required: &["ref"] },
        RemoteBrowserToolSpec { name: "browser_highlight", description: "Highlight an element by ref for visual grounding.", required: &["ref"] },
        RemoteBrowserToolSpec { name: "browser_cdp", description: "Send a Chrome DevTools Protocol command to the target browser tab.", required: &["method"] },
        RemoteBrowserToolSpec { name: "browser_tabs", description: "List, create, close, or select a browser tab.", required: &["action"] },
        RemoteBrowserToolSpec { name: "browser_take_screenshot", description: "Take a screenshot of the current page.", required: &[] },
    ]
}

fn is_remote_browser_tool(name: &str) -> bool {
    REMOTE_BROWSER_TOOL_NAMES.contains(&name)
}

fn remote_execution_input_json(name: &str, args: &Value) -> String {
    // CloudAgent is a first-class typed Remote capability whose external Runner backend
    // deserializes CloudAgentRequest directly. Other legacy Remote tools retain the shared
    // tool-wrapper envelope expected by their generic execution backend.
    if name == CLOUD_AGENT_TOOL_NAME {
        args.to_string()
    } else {
        json!({"tool":name,"arguments":args}).to_string()
    }
}

fn remote_cloud_agent_tool_definition() -> Value {
    json!({
        "type":"function",
        "name":CLOUD_AGENT_TOOL_NAME,
        "description":"Manage Desktop-compatible cloud agents through the currently paired trusted Remote Runner. Every dispatch is account/device fenced and approval-gated; destructive lifecycle actions also require confirm: true.",
        "parameters":{
            "type":"object",
            "additionalProperties":false,
            "required":["action"],
            "properties":{
                "action":{"type":"string","enum":CLOUD_AGENT_ACTIONS},
                "prompt":{"type":"string"},
                "images":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["url"],"properties":{"url":{"type":"string","minLength":1}}}},
                "repo_url":{"type":"string"},
                "starting_ref":{"type":"string"},
                "model":{"type":"string"},
                "model_params":{"type":"object","additionalProperties":{"type":"string"}},
                "title":{"type":"string"},
                "environment":{
                    "type":"object",
                    "additionalProperties":false,
                    "required":["type"],
                    "properties":{
                        "type":{"type":"string","enum":["cloud","pool","machine","environment"]},
                        "name":{"type":"string"},
                        "id":{"type":"string"},
                        "team_id":{"type":"integer"}
                    }
                },
                "interrupt":{"type":"boolean"},
                "agent_id":{"type":"string"},
                "scope":{"type":"string","enum":["launched","all"]},
                "include_archived":{"type":"boolean"},
                "limit":{"type":"integer","minimum":1},
                "confirm":{"type":"boolean"}
            }
        }
    })
}

fn remote_browser_tool_definitions() -> Vec<Value> {
    remote_browser_specs()
        .into_iter()
        .map(|spec| {
            let mut properties = serde_json::Map::new();
            for key in spec.required {
                properties.insert((*key).to_string(), json!({}));
            }
            properties
                .entry("viewId".to_string())
                .or_insert_with(|| json!({"type":"string","minLength":1}));
            properties
                .entry("expectedGeneration".to_string())
                .or_insert_with(|| json!({"type":"string","minLength":1,"maxLength":240}));
            if spec.name == "browser_tabs" {
                properties.insert(
                    "action".to_string(),
                    json!({"type":"string","enum":["list","new","close","select"]}),
                );
            }
            if spec.name == "browser_upload_file" {
                properties.insert(
                    "filePath".to_string(),
                    json!({"type":"string","minLength":1,"maxLength":MAX_BROWSER_UPLOAD_PATH}),
                );
            }
            let mut required = spec.required.to_vec();
            if matches!(
                spec.name,
                "browser_refresh" | "browser_back" | "browser_forward"
                    | "browser_click" | "browser_mouse_click_xy" | "browser_download"
                    | "browser_upload_file" | "browser_type" | "browser_fill" | "browser_select_option" | "browser_press_key"
                    | "browser_drag" | "browser_cdp"
            ) {
                if !required.contains(&"viewId") { required.push("viewId"); }
                if !required.contains(&"expectedGeneration") { required.push("expectedGeneration"); }
            }
            json!({
                "type":"function",
                "name":spec.name,
                "description":spec.description,
                "parameters":{
                    "type":"object",
                    "required":required,
                    "additionalProperties":true,
                    "properties":properties
                }
            })
        })
        .collect()
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RemoteDispatchBinding {
    pub(crate) credential_plane: String,
    pub(crate) credential_id: String,
    pub(crate) issued_at_ms: u64,
    pub(crate) expires_at_ms: u64,
    pub(crate) endpoint: String,
    pub(crate) bearer_credential: String,
    pub(crate) device_id: String,
    pub(crate) account_fence: String,
    pub(crate) account_epoch: u64,
    pub(crate) executors: BTreeSet<String>,
}

impl RemoteDispatchBinding {
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        let binding: Self = serde_json::from_str(raw)
            .map_err(|error| format!("invalid protected remote binding: {error}"))?;
        binding.validate()?;
        Ok(binding)
    }

    fn validate(&self) -> Result<(), String> {
        if self.credential_plane != "authorized-remote-runner-v1" {
            return Err(
                "remote binding credential plane is not an authorized Remote Runner enrollment"
                    .into(),
            );
        }
        if self.account_epoch == 0 {
            return Err("remote binding account epoch must be positive".into());
        }
        if self.issued_at_ms == 0 || self.expires_at_ms <= self.issued_at_ms {
            return Err("remote binding credential lifetime is invalid".into());
        }
        for (label, value) in [
            ("remote credential", self.credential_id.as_str()),
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
        if self.executors.is_empty() {
            return Err("remote binding must declare at least one executor".into());
        }
        const ALLOWED_EXECUTORS: &[&str] = &[
            "shell",
            "read",
            "write",
            "computer",
            "screenshot",
            "browser",
            "external-shell",
            "external-read",
            "cloud-agent",
            "box-store-sync",
            "file-transfer",
        ];
        if let Some(executor) = self.executors.iter().find(|value| !ALLOWED_EXECUTORS.contains(&value.as_str())) {
            return Err(format!("remote binding executor is unsupported: {executor}"));
        }
        Ok(())
    }

    pub(crate) fn active_at(&self, now_ms: u64) -> Result<(), String> {
        if now_ms < self.issued_at_ms {
            return Err("trusted Remote credential is not active yet".into());
        }
        if now_ms >= self.expires_at_ms {
            return Err("trusted Remote credential is expired".into());
        }
        Ok(())
    }

    pub(crate) fn supports(&self, executor: &str) -> bool {
        self.executors.contains(executor)
    }

    pub(crate) fn context(
        &self,
        operation_id: &str,
        request_id: &str,
        approval_id: &str,
    ) -> Result<RemoteExecutionContext, String> {
        Ok(RemoteExecutionContext {
            credential_id: self.credential_id.clone(),
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
    refusal_id: String,
    account_fence: String,
    account_epoch: u64,
    device_id: String,
    signal: Arc<(Mutex<Option<bool>>, Condvar)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RemoteAutoReviewDecision {
    Allow,
    RequireApproval { reason: String },
    Reject { reason: String },
}

pub(crate) type RemoteAutoReviewPreflight =
    Arc<dyn Fn(&str, &Value) -> Result<RemoteAutoReviewDecision, String> + Send + Sync>;

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
    refused_actions: Arc<Mutex<VecDeque<String>>>,
}

impl RemoteApprovalRegistry {
    pub(crate) fn is_pending_for_account(
        &self,
        approval_id: &str,
        current_account_fence: &str,
    ) -> Result<bool, String> {
        let wait = self
            .waits
            .lock()
            .map_err(|_| "remote approval registry lock poisoned".to_string())?
            .get(approval_id)
            .cloned();
        let Some(wait) = wait else {
            return Ok(false);
        };
        if wait.account_fence != current_account_fence {
            return Err("remote approval callback is fenced by account identity".into());
        }
        Ok(true)
    }

    pub(crate) fn pending_ids_for_account(
        &self,
        current_account_fence: &str,
    ) -> Result<Vec<String>, String> {
        let waits = self
            .waits
            .lock()
            .map_err(|_| "remote approval registry lock poisoned".to_string())?;
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
        if !approved {
            self.remember_refused(wait.refusal_id.clone())?;
            broker.remember_local_tool_refusal(
                &wait.parent_operation_id,
                1,
                "remote-tool",
                &json!({"refusalId": wait.refusal_id}),
            )?;
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


    pub(crate) fn clear_from_ui(
        &self,
        approval_id: &str,
        expected_operation_id: &str,
        current_account_fence: &str,
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
            return Err("remote approval cleanup is fenced by account identity".into());
        }
        if wait.remote_operation_id != expected_operation_id {
            return Err("remote approval cleanup operation identity disagrees with waiter".into());
        }
        let (state, condvar) = &*wait.signal;
        let mut state = state
            .lock()
            .map_err(|_| "remote approval signal lock poisoned".to_string())?;
        if state.is_some() {
            return Err("remote approval cleanup raced a terminal resolution".into());
        }
        *state = Some(false);
        condvar.notify_all();
        Ok(Some(RemoteApprovalResolution {
            parent_operation_id: wait.parent_operation_id,
            remote_operation_id: wait.remote_operation_id,
            capability: wait.capability,
            approved: false,
        }))
    }

    fn was_refused(&self, refusal_id: &str) -> Result<bool, String> {
        self.refused_actions
            .lock()
            .map_err(|_| "remote refusal memory lock poisoned".to_string())
            .map(|refused| refused.iter().any(|value| value == refusal_id))
    }

    fn remember_refused(&self, refusal_id: String) -> Result<(), String> {
        let mut refused = self
            .refused_actions
            .lock()
            .map_err(|_| "remote refusal memory lock poisoned".to_string())?;
        if refused.iter().any(|value| value == &refusal_id) {
            return Ok(());
        }
        refused.push_back(refusal_id);
        while refused.len() > REMOTE_REFUSED_ACTION_MEMORY {
            refused.pop_front();
        }
        Ok(())
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
    user_settings: Arc<Mutex<AndroidUserSettings>>,
    local_tool_permission_ceiling: Option<LocalToolPermission>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    events: Arc<Mutex<VecDeque<Value>>>,
    parent_operation_id: String,
    parent_request_id: String,
    cancelled: Arc<AtomicBool>,
    client_paused: Arc<AtomicBool>,
    auto_review_preflight: Option<RemoteAutoReviewPreflight>,
    browser_navigation_observer: Option<RemoteBrowserNavigationObserver>,
}

impl AndroidRoutedToolBridge for RemoteRoutedTools {
    fn list_tools(&self) -> Result<Vec<Value>, String> {
        let mut tools = self.delegate.list_tools()?;
        if self.client_paused.load(Ordering::Acquire) {
            return Ok(tools);
        }
        let binding = match self.current_binding() {
            Ok(binding) => binding,
            Err(_) => return Ok(tools),
        };
        tools.retain(|tool| {
            let Some(name) = tool.get("name").and_then(Value::as_str) else {
                return true;
            };
            !matches!(
                name,
                "Shell" | "Read" | "Write" | "Computer" | "Screenshot" | "ExternalShell" | "ExternalRead"
                    | COPY_TO_BOX_TOOL_NAME | COPY_FROM_BOX_TOOL_NAME
            ) && !is_remote_browser_tool(name)
        });
        if binding.supports("shell") {
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
        }
        if binding.supports("read") {
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
        }
        if binding.supports("write") {
            tools.push(json!({
                "type":"function",
                "name":"Write",
                "description":"Write bounded UTF-8 content to a file on the currently paired trusted Remote desktop. Requires one-time user approval before dispatch.",
                "parameters":{
                    "type":"object",
                    "additionalProperties":false,
                    "required":["path","content"],
                    "properties":{
                        "path":{"type":"string","minLength":1,"maxLength":4096},
                        "content":{"type":"string","maxLength":1048576}
                    }
                }
            }));
        }
        if binding.supports("file-transfer") {
            tools.push(json!({
                "type":"function",
                "name":COPY_TO_BOX_TOOL_NAME,
                "description":"Copy a file from the user's paired trusted Remote computer into the Agent box, verbatim. The transfer is bounded to 100 MiB, SHA-256 verified, account/device fenced, and approval-gated.",
                "parameters":{
                    "type":"object",
                    "additionalProperties":false,
                    "required":["computer_path"],
                    "properties":{
                        "computer_path":{"type":"string","minLength":1,"maxLength":4096},
                        "box_path":{"type":"string","minLength":1,"maxLength":4096},
                        "computer":{"type":"string","minLength":1,"maxLength":512},
                        "expected_destination_version":{"type":"string","pattern":"^[0-9a-fA-F]{64}$"},
                        "overwrite":{"type":"boolean"}
                    }
                }
            }));
            tools.push(json!({
                "type":"function",
                "name":COPY_FROM_BOX_TOOL_NAME,
                "description":"Copy a file from the Agent box out to the user's paired trusted Remote computer, verbatim. The transfer is bounded to 100 MiB, SHA-256 verified, account/device fenced, and approval-gated.",
                "parameters":{
                    "type":"object",
                    "additionalProperties":false,
                    "required":["box_path"],
                    "properties":{
                        "box_path":{"type":"string","minLength":1,"maxLength":4096},
                        "computer_path":{"type":"string","minLength":1,"maxLength":4096},
                        "computer":{"type":"string","minLength":1,"maxLength":512},
                        "expected_destination_version":{"type":"string","pattern":"^[0-9a-fA-F]{64}$"},
                        "overwrite":{"type":"boolean"}
                    }
                }
            }));
        }
        if binding.supports("computer") {
            tools.push(json!({
                "type":"function",
                "name":"Computer",
                "description":"Interact with the paired trusted Remote desktop using the Desktop Fabushi Computer contract. Every dispatch requires one-time user approval.",
                "parameters":{
                    "type":"object",
                    "additionalProperties":false,
                    "required":["action"],
                    "properties":{
                        "action":{"type":"string","enum":["screenshot","click","move","drag","type","key","scroll","wait"]},
                        "x":{"type":"integer"},
                        "y":{"type":"integer"},
                        "x2":{"type":"integer"},
                        "y2":{"type":"integer"},
                        "path":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["x","y"],"properties":{"x":{"type":"integer"},"y":{"type":"integer"}}}},
                        "text":{"type":"string"},
                        "key":{"type":"string"},
                        "button":{"type":"string","enum":["left","right","middle"]},
                        "count":{"type":"integer","minimum":1,"maximum":3},
                        "direction":{"type":"string","enum":["up","down","left","right"]},
                        "amount":{"type":"integer"},
                        "durationMs":{"type":"integer","minimum":0,"maximum":30000},
                        "description":{"type":"string"},
                        "then":{"type":"array","minItems":1,"maxItems":9,"items":{"type":"object"}}
                    }
                }
            }));
        }
        if binding.supports("screenshot") {
            tools.push(json!({
                "type":"function",
                "name":"Screenshot",
                "description":"Capture the paired trusted Remote desktop using the Desktop Fabushi screenshot contract. Every dispatch requires one-time user approval.",
                "parameters":{
                    "type":"object",
                    "additionalProperties":false,
                    "properties":{}
                }
            }));
        }
        if binding.supports("browser") {
            tools.extend(remote_browser_tool_definitions());
        }
        if binding.supports("external-shell") {
            tools.push(json!({
                "type":"function",
                "name":"ExternalShell",
                "description":"Run a shell command on the user's paired trusted Remote computer. Requires one-time user approval before dispatch.",
                "parameters":{
                    "type":"object",
                    "additionalProperties":false,
                    "required":["command"],
                    "properties":{
                        "command":{"type":"string","minLength":1},
                        "workingDirectory":{"type":"string"}
                    }
                }
            }));
        }
        if binding.supports("cloud-agent") {
            tools.push(remote_cloud_agent_tool_definition());
        }
        if binding.supports("external-read") {
            tools.push(json!({
                "type":"function",
                "name":"ExternalRead",
                "description":"Read a file from the user's paired trusted Remote computer. Requires one-time user approval before dispatch.",
                "parameters":{
                    "type":"object",
                    "additionalProperties":false,
                    "required":["path"],
                    "properties":{
                        "path":{"type":"string","minLength":1},
                        "offset":{"type":"integer"},
                        "limit":{"type":"integer","minimum":0},
                        "encodingHint":{"type":"string"}
                    }
                }
            }));
        }
        Ok(tools)
    }

    fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String> {
        let browser_tool = is_remote_browser_tool(name);
        if !matches!(
            name,
            "Shell" | "Read" | "Write" | "Computer" | "Screenshot" | "ExternalShell" | "ExternalRead"
                | COPY_TO_BOX_TOOL_NAME | COPY_FROM_BOX_TOOL_NAME | CLOUD_AGENT_TOOL_NAME
        ) && !browser_tool {
            return self.delegate.call_tool(name, args, tool_call_id);
        }
        if name == CLOUD_AGENT_TOOL_NAME {
            if let Some(action) = args.get("action").and_then(Value::as_str) {
                if CLOUD_AGENT_DESTRUCTIVE_ACTIONS.contains(&action)
                    && args.get("confirm").and_then(Value::as_bool) != Some(true)
                {
                    return Ok(Value::String(format!(
                        "'{action}' is destructive. Confirm with the user first, then call CloudAgent again with confirm: true."
                    )));
                }
            }
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
        // Desktop local-tool permission remembers an exact rejected action for the lifetime of
        // the current turn even if the model retries it under a new toolCallId. Preserve that
        // semantic on Android by excluding toolCallId from the refusal identity while fencing it
        // to the canonical parent operation.
        let refusal_id = sha256_hex(
            format!("{}\n{}\n{}", self.parent_operation_id, name, args_json).as_bytes(),
        );
        let capability = match name {
            "Shell" | "ExternalShell" => "remote.shell",
            "Read" | "ExternalRead" => "remote.read",
            "Write" => "remote.write",
            COPY_TO_BOX_TOOL_NAME | COPY_FROM_BOX_TOOL_NAME => "box.file.transfer",
            "Computer" | "Screenshot" => "computer.use",
            CLOUD_AGENT_TOOL_NAME => "cloud-agent",
            _ if browser_tool => "browser.use",
            _ => unreachable!("unsupported Remote routed tool was delegated"),
        };
        let permission_at_authorization = self.local_tool_permission()?;
        if permission_at_authorization == LocalToolPermission::Never {
            return Err("local computer tools are disabled by the current Host permission policy".into());
        }
        if self.approvals.was_refused(&refusal_id)?
            || self.broker.local_tool_refusal_reason(
                &self.parent_operation_id,
                1,
                "remote-tool",
                &json!({"refusalId": refusal_id.clone()}),
            )?.is_some()
        {
            return Err(
                "remote tool action was already denied in this turn; do not retry it".into(),
            );
        }
        let binding = self.current_binding()?;
        let required_executor = match name {
            "Shell" => "shell",
            "Read" => "read",
            "Write" => "write",
            COPY_TO_BOX_TOOL_NAME | COPY_FROM_BOX_TOOL_NAME => "file-transfer",
            "Computer" => "computer",
            "Screenshot" => "screenshot",
            CLOUD_AGENT_TOOL_NAME => "cloud-agent",
            _ if browser_tool => "browser",
            "ExternalShell" => "external-shell",
            "ExternalRead" => "external-read",
            _ => unreachable!("unsupported Remote routed tool was delegated"),
        };
        if !binding.supports(required_executor) {
            return Err(format!("trusted Remote binding does not declare executor {required_executor}"));
        }
        if matches!(name, COPY_TO_BOX_TOOL_NAME | COPY_FROM_BOX_TOOL_NAME) {
            if let Some(computer) = args.get("computer").and_then(Value::as_str) {
                if computer != binding.device_id {
                    return Err("file-transfer computer selector does not match the paired Remote device".into());
                }
            }
        }
        let auto_review = match self.auto_review_preflight.as_ref() {
            Some(preflight) => preflight(name, &args)?,
            None => RemoteAutoReviewDecision::Allow,
        };
        let (force_manual_approval, auto_review_reason) = match auto_review {
            RemoteAutoReviewDecision::Allow => (false, None),
            RemoteAutoReviewDecision::RequireApproval { reason } => (true, Some(reason)),
            RemoteAutoReviewDecision::Reject { reason } => {
                return Err(format!("remote tool action rejected by Auto-review: {reason}"));
            }
        };
        let context = binding.context(&operation_id, &request_id, &approval_id)?;
        let request = ExecutionRequest {
            operation_id: operation_id.clone(),
            capability_id: capability.to_string(),
            input_json: remote_execution_input_json(name, &args),
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
                let parsed = parse_remote_tool_output(name, &args, &result.output_json)?;
                self.observe_browser_navigation(name, &parsed, &operation_id);
                return Ok(parsed);
            }
        }

        self.refuse_new_dispatch_if_paused()?;

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
                "autoReviewReason":auto_review_reason,
            }),
            &binding.account_fence,
            binding.account_epoch,
            &binding.device_id,
            now_ms(),
        )?;

        let approved = if permission_at_authorization == LocalToolPermission::Always
            && !force_manual_approval
        {
            let resolved = self.broker.resolve_approval(
                &approval_id,
                true,
                &binding.account_fence,
                now_ms(),
            )?;
            if resolved.operation_id != operation_id
                || resolved.request_id != request_id
                || resolved.capability != capability
                || resolved.account_epoch != Some(binding.account_epoch)
                || resolved.device_id.as_deref() != Some(binding.device_id.as_str())
            {
                let _ = self.broker.cancel_approval_operation(
                    &operation_id,
                    "Host policy authorization identity mismatch",
                    now_ms(),
                );
                return Err("Host policy authorization disagrees with durable broker state".into());
            }
            true
        } else {
            let signal = Arc::new((Mutex::new(None), Condvar::new()));
            self.approvals.register(
                &approval_id,
                RemoteApprovalWait {
                    parent_operation_id: self.parent_operation_id.clone(),
                    remote_operation_id: operation_id.clone(),
                    request_id: request_id.clone(),
                    capability: capability.to_string(),
                    refusal_id: refusal_id.clone(),
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
                    "autoReview":force_manual_approval,
                    "reason":auto_review_reason,
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
            approved
        };
        if !approved {
            return Err("remote tool dispatch denied by user".into());
        }

        // Freeze exact protected binding across approval. Endpoint/credential/device/account
        // rotation after the card was shown invalidates this dispatch instead of silently
        // changing its execution target. The already-approved one-time grant is explicitly
        // cancelled before returning so it cannot survive as an unused privilege in this Host.
        let current_binding = match self.current_binding() {
            Ok(binding) => binding,
            Err(error) => {
                return Err(abandon_unconsumed_remote_approval(
                    &self.broker,
                    &operation_id,
                    &error,
                ));
            }
        };
        if current_binding != binding {
            return Err(abandon_unconsumed_remote_approval(
                &self.broker,
                &operation_id,
                "trusted Remote binding changed after approval",
            ));
        }
        if let Err(error) = self.refuse_new_dispatch_if_paused() {
            return Err(abandon_unconsumed_remote_approval(
                &self.broker,
                &operation_id,
                &error,
            ));
        }
        let permission_before_dispatch = self.local_tool_permission()?;
        if permission_before_dispatch == LocalToolPermission::Never
            || (permission_at_authorization == LocalToolPermission::Always
                && permission_before_dispatch != LocalToolPermission::Always)
        {
            return Err(abandon_unconsumed_remote_approval(
                &self.broker,
                &operation_id,
                "local computer permission policy changed before Remote dispatch",
            ));
        }
        // Persist a never-sent Remote operation before consuming the one-time grant. A process
        // death before mark_sent can then be proven not to have crossed the Remote boundary.
        let prepare_result = (|| -> Result<(), String> {
            let mut guard = self
                .runner
                .lock()
                .map_err(|_| "remote runner lock poisoned".to_string())?;
            let runner = guard.as_mut().ok_or("trusted Remote runner is unavailable")?;
            runner
                .prepare_authorized(&context, &request, now_ms())
                .map_err(remote_execution_error)
        })();
        if let Err(error) = prepare_result {
            return Err(abandon_unconsumed_remote_approval(
                &self.broker,
                &operation_id,
                &format!("trusted Remote dispatch could not be prepared after approval: {error}"),
            ));
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
        if self.cancelled.load(Ordering::Acquire) || self.client_paused.load(Ordering::Acquire) {
            if let Ok(mut guard) = self.runner.lock() {
                if let Some(runner) = guard.as_mut() {
                    let _ = runner.cancel_prepared_authorized(&context, now_ms());
                }
            }
            return Err(if self.client_paused.load(Ordering::Acquire) {
                "trusted Remote client is paused before side effect".into()
            } else {
                "remote tool dispatch cancelled before side effect".into()
            });
        }

        let mut guard = self
            .runner
            .lock()
            .map_err(|_| "remote runner lock poisoned".to_string())?;
        let runner = guard.as_mut().ok_or("trusted Remote runner is unavailable")?;
        match runner.dispatch_prepared_authorized(&context, &request, now_ms()) {
            Ok(result) => {
                let parsed = parse_remote_tool_output(name, &args, &result.output_json)?;
                self.observe_browser_navigation(name, &parsed, &operation_id);
                Ok(parsed)
            }
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
    fn observe_browser_navigation(&self, name: &str, parsed: &Value, operation_id: &str) {
        let Some(observer) = self.browser_navigation_observer.as_ref() else {
            return;
        };
        if !is_remote_browser_tool(name)
            || parsed.get("isError").and_then(Value::as_bool) != Some(false)
        {
            return;
        }
        let Some(url) = parsed
            .get("url")
            .or_else(|| parsed.get("currentUrl"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return;
        };
        let page_title = parsed
            .get("pageTitle")
            .or_else(|| parsed.get("title"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        observer(RemoteBrowserNavigationObservation {
            operation_id: operation_id.to_string(),
            url: url.to_string(),
            page_title,
        });
    }

    fn refuse_new_dispatch_if_paused(&self) -> Result<(), String> {
        if self.client_paused.load(Ordering::Acquire) {
            Err("trusted Remote client is paused by sand_client_pause".into())
        } else {
            Ok(())
        }
    }

    fn local_tool_permission(&self) -> Result<LocalToolPermission, String> {
        let account_fence = self
            .live_account_fence
            .lock()
            .map_err(|_| "live account fence lock poisoned".to_string())?
            .clone()
            .ok_or_else(|| "trusted Remote tool requires an active account fence".to_string())?;
        self.user_settings
            .lock()
            .map_err(|_| "Android user settings lock poisoned".to_string())?
            .local_tool_permission_for(&account_fence)
            .map(|permission| permission.clamp_to_ceiling(self.local_tool_permission_ceiling))
            .map_err(|error| format!("failed to read Android Remote tool permission: {error}"))
    }

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
        binding.active_at(now_ms())?;
        Ok(binding)
    }
}

pub(crate) fn with_remote_routed_tools(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    broker: SharedCapabilityBroker,
    binding: Arc<Mutex<Option<RemoteDispatchBinding>>>,
    runner: Arc<Mutex<Option<AuthenticatedRemoteHostRunner<AuthenticatedRemoteHttpTransport>>>>,
    approvals: RemoteApprovalRegistry,
    user_settings: Arc<Mutex<AndroidUserSettings>>,
    local_tool_permission_ceiling: Option<LocalToolPermission>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    events: Arc<Mutex<VecDeque<Value>>>,
    parent_operation_id: &str,
    parent_request_id: &str,
    cancelled: Arc<AtomicBool>,
    client_paused: Arc<AtomicBool>,
    auto_review_preflight: Option<RemoteAutoReviewPreflight>,
    browser_navigation_observer: Option<RemoteBrowserNavigationObserver>,
) -> Arc<dyn AndroidRoutedToolBridge> {
    Arc::new(RemoteRoutedTools {
        delegate,
        broker,
        binding,
        runner,
        approvals,
        user_settings,
        local_tool_permission_ceiling,
        live_account_fence,
        events,
        parent_operation_id: parent_operation_id.to_string(),
        parent_request_id: parent_request_id.to_string(),
        cancelled,
        client_paused,
        auto_review_preflight,
        browser_navigation_observer,
    })
}

fn abandon_unconsumed_remote_approval(
    broker: &SharedCapabilityBroker,
    operation_id: &str,
    reason: &str,
) -> String {
    match broker.cancel_approval_operation(operation_id, reason, now_ms()) {
        Ok(true) => reason.to_string(),
        Ok(false) => format!(
            "{reason}; Remote one-time approval was no longer cancellable before dispatch"
        ),
        Err(error) => format!(
            "{reason}; failed to cancel unconsumed Remote one-time approval: {error}"
        ),
    }
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
        "Write" => {
            const KEYS: &[&str] = &["path", "content"];
            if let Some(key) = object.keys().find(|key| !KEYS.contains(&key.as_str())) {
                return Err(format!("Remote Write argument is unsupported: {key}"));
            }
            let path = object
                .get("path")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or("Remote Write path is required")?;
            if path.len() > MAX_READ_PATH || path.chars().any(|c| c == '\0') {
                return Err("Remote Write path is invalid or too large".into());
            }
            let content = object
                .get("content")
                .and_then(Value::as_str)
                .ok_or("Remote Write content must be a string")?;
            if content.len() > MAX_WRITE_CONTENT {
                return Err("Remote Write content exceeds the bounded 1 MiB limit".into());
            }
            Ok(30_000)
        }
        COPY_TO_BOX_TOOL_NAME | COPY_FROM_BOX_TOOL_NAME => {
            let allowed = if name == COPY_TO_BOX_TOOL_NAME {
                ["computer_path", "box_path", "computer", "expected_destination_version", "overwrite"]
            } else {
                ["box_path", "computer_path", "computer", "expected_destination_version", "overwrite"]
            };
            if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
                return Err(format!("Remote {name} argument is unsupported: {key}"));
            }
            let required = if name == COPY_TO_BOX_TOOL_NAME { "computer_path" } else { "box_path" };
            let path = object
                .get(required)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("Remote {name} requires {required}"))?;
            if path.len() > MAX_READ_PATH || path.contains('\0') {
                return Err(format!("Remote {name} {required} is invalid or too large"));
            }
            for optional in ["computer_path", "box_path"] {
                if optional == required {
                    continue;
                }
                if let Some(value) = object.get(optional) {
                    let value = value.as_str().ok_or_else(|| format!("Remote {name} {optional} must be a string"))?;
                    if value.trim().is_empty() || value.len() > MAX_READ_PATH || value.contains('\0') {
                        return Err(format!("Remote {name} {optional} is invalid or too large"));
                    }
                }
            }
            if let Some(computer) = object.get("computer") {
                let computer = computer.as_str().ok_or_else(|| format!("Remote {name} computer must be a string"))?;
                if computer.trim().is_empty() || computer.len() > 512 || computer.chars().any(char::is_control) {
                    return Err(format!("Remote {name} computer selector is invalid"));
                }
            }
            if let Some(version) = object.get("expected_destination_version") {
                let version = version.as_str().ok_or_else(|| format!("Remote {name} expected_destination_version must be a SHA-256 string"))?;
                if version.len() != 64 || !version.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err(format!("Remote {name} expected_destination_version is invalid"));
                }
            }
            if object.get("overwrite").is_some_and(|value| !value.is_boolean()) {
                return Err(format!("Remote {name} overwrite must be boolean"));
            }
            Ok(120_000)
        }
        "Computer" => {
            validate_remote_computer_action(args, true)?;
            Ok(30_000)
        }
        "Screenshot" => {
            if !object.is_empty() {
                return Err("Remote Screenshot does not accept arguments".into());
            }
            Ok(30_000)
        }
        CLOUD_AGENT_TOOL_NAME => validate_remote_cloud_agent_input(object),
        "ExternalShell" => {
            const KEYS: &[&str] = &["command", "workingDirectory"];
            if let Some(key) = object.keys().find(|key| !KEYS.contains(&key.as_str())) {
                return Err(format!("Remote ExternalShell argument is unsupported: {key}"));
            }
            let command = object
                .get("command")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or("Remote ExternalShell command is required")?;
            if command.len() > MAX_SHELL_COMMAND || command.chars().any(|c| c == '\0') {
                return Err("Remote ExternalShell command is invalid or too large".into());
            }
            if object
                .get("workingDirectory")
                .is_some_and(|value| !value.is_string())
            {
                return Err("Remote ExternalShell workingDirectory must be a string".into());
            }
            Ok(30_000)
        }
        name if is_remote_browser_tool(name) => validate_remote_browser_tool_input(name, args),
        "ExternalRead" => {
            const KEYS: &[&str] = &["path", "offset", "limit", "encodingHint"];
            if let Some(key) = object.keys().find(|key| !KEYS.contains(&key.as_str())) {
                return Err(format!("Remote ExternalRead argument is unsupported: {key}"));
            }
            let path = object
                .get("path")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or("Remote ExternalRead path is required")?;
            if path.len() > MAX_READ_PATH || path.chars().any(|c| c == '\0') {
                return Err("Remote ExternalRead path is invalid or too large".into());
            }
            if let Some(offset) = object.get("offset") {
                let offset = offset
                    .as_i64()
                    .ok_or("Remote ExternalRead offset must be an integer")?;
                if i32::try_from(offset).is_err() {
                    return Err("Remote ExternalRead offset is outside signed int32 range".into());
                }
            }
            if let Some(limit) = object.get("limit") {
                let limit = limit
                    .as_u64()
                    .ok_or("Remote ExternalRead limit must be a non-negative integer")?;
                if u32::try_from(limit).is_err() {
                    return Err("Remote ExternalRead limit is outside unsigned int32 range".into());
                }
            }
            if object
                .get("encodingHint")
                .is_some_and(|value| !value.is_string())
            {
                return Err("Remote ExternalRead encodingHint must be a string".into());
            }
            Ok(30_000)
        }
        _ => Err("unsupported Remote routed tool".into()),
    }
}

fn validate_browser_required_string(
    object: &serde_json::Map<String, Value>,
    tool: &str,
    key: &str,
) -> Result<(), String> {
    match object.get(key).and_then(Value::as_str) {
        Some(value) if !value.is_empty() => Ok(()),
        _ => Err(format!("Remote Browser {tool} requires string {key}")),
    }
}

fn validate_browser_optional_string(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<(), String> {
    if object.get(key).is_some_and(|value| !value.is_string()) {
        return Err(format!("Remote Browser {key} must be a string"));
    }
    Ok(())
}

fn validate_browser_optional_bool(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<(), String> {
    if object.get(key).is_some_and(|value| !value.is_boolean()) {
        return Err(format!("Remote Browser {key} must be a boolean"));
    }
    Ok(())
}

fn validate_browser_optional_number(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<(), String> {
    if object.get(key).is_some_and(|value| value.as_f64().is_none()) {
        return Err(format!("Remote Browser {key} must be a number"));
    }
    Ok(())
}

fn validate_browser_string_array(
    object: &serde_json::Map<String, Value>,
    key: &str,
    required: bool,
) -> Result<(), String> {
    let Some(value) = object.get(key) else {
        return if required {
            Err(format!("Remote Browser requires {key}"))
        } else {
            Ok(())
        };
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("Remote Browser {key} must be an array"))?;
    if values.iter().any(|value| !value.is_string()) {
        return Err(format!("Remote Browser {key} must contain only strings"));
    }
    Ok(())
}

fn validate_browser_button(object: &serde_json::Map<String, Value>) -> Result<(), String> {
    if let Some(button) = object.get("button") {
        if !matches!(button.as_str(), Some("left" | "right" | "middle")) {
            return Err("Remote Browser button is unsupported".into());
        }
    }
    Ok(())
}

fn validate_browser_cdp_method(method: &str) -> Result<(), String> {
    const DENIED_PREFIXES: &[&str] = &[
        "Browser.",
        "Target.",
        "Storage.",
        "SystemInfo.",
        "Security.",
        "Input.",
        "Tethering.",
        "Cast.",
        // Fetch interception can rewrite Authorization/cookie headers after the
        // Remote Browser transport has applied its account/device fence.
        "Fetch.",
    ];
    const DENIED_METHODS: &[&str] = &[
        "Network.setCookie",
        "Network.setCookies",
        "Network.getCookies",
        "Network.getAllCookies",
        "Network.deleteCookies",
        "Network.clearBrowserCookies",
        "Network.clearBrowserCache",
        // Desktop's BrowserUa extension has an explicit UA-token kill switch.
        // Android's Remote Browser replacement never exposes bearer material to
        // the browser tool payload, so fail closed on CDP methods that could
        // re-introduce credential or UA injection inside the remote page.
        "Network.setExtraHTTPHeaders",
        "Network.setUserAgentOverride",
        "Emulation.setUserAgentOverride",
    ];
    if DENIED_PREFIXES.iter().any(|prefix| method.starts_with(prefix))
        || DENIED_METHODS.contains(&method)
    {
        return Err(format!("Remote Browser CDP method is denied: {method}"));
    }
    Ok(())
}

fn validate_remote_browser_allowed_keys(
    name: &str,
    object: &serde_json::Map<String, Value>,
) -> Result<(), String> {
    const COMMON: &[&str] = &["viewId", "element", "expectedGeneration"];
    let tool_keys: &[&str] = match name {
        "browser_navigate" => &["url", "newTab"],
        "browser_refresh" | "browser_back" | "browser_forward" => &[],
        "browser_snapshot" => &["interactive", "maxDepth", "selector"],
        "browser_click" => &["ref", "offsetX", "offsetY", "holdDurationMs", "doubleClick", "button", "modifiers"],
        "browser_mouse_click_xy" => &["x", "y", "button"],
        "browser_download" => &["ref"],
        "browser_upload_file" => &["ref", "filePath"],
        "browser_type" => &["ref", "text", "clear", "slowly", "submit"],
        "browser_fill" => &["ref", "value"],
        "browser_select_option" => &["ref", "values"],
        "browser_press_key" => &["key"],
        "browser_scroll" => &["ref", "amount", "deltaX", "deltaY", "direction"],
        "browser_drag" => &["sourceRef", "targetRef", "targetX", "targetY"],
        "browser_get_bounding_box" => &["ref"],
        "browser_highlight" => &["ref", "durationMs"],
        "browser_cdp" => &["method", "params"],
        "browser_tabs" => &["action", "index"],
        "browser_take_screenshot" => &["fullPage"],
        _ => return Err("unsupported Remote Browser tool".into()),
    };
    if let Some(key) = object
        .keys()
        .find(|key| !COMMON.contains(&key.as_str()) && !tool_keys.contains(&key.as_str()))
    {
        return Err(format!("Remote Browser {name} argument is unsupported: {key}"));
    }
    Ok(())
}

fn remote_browser_requires_generation_fence(name: &str, args: &Value) -> bool {
    match name {
        "browser_refresh" | "browser_back" | "browser_forward"
        | "browser_click" | "browser_mouse_click_xy" | "browser_download"
        | "browser_upload_file" | "browser_type" | "browser_fill" | "browser_select_option" | "browser_press_key"
        | "browser_drag" | "browser_cdp" => true,
        "browser_tabs" => matches!(
            args.get("action").and_then(Value::as_str),
            Some("close")
        ),
        _ => false,
    }
}

fn validate_remote_browser_tool_input(name: &str, args: &Value) -> Result<u64, String> {
    let object = args
        .as_object()
        .ok_or("Remote Browser arguments must be an object")?;
    validate_remote_browser_allowed_keys(name, object)?;
    let spec = remote_browser_specs()
        .into_iter()
        .find(|spec| spec.name == name)
        .ok_or("unsupported Remote Browser tool")?;
    for key in spec.required {
        let value = object
            .get(*key)
            .ok_or_else(|| format!("Remote Browser {name} requires {key}"))?;
        if value.is_null() || value.as_str() == Some("") {
            return Err(format!("Remote Browser {name} requires {key}"));
        }
    }

    validate_browser_optional_string(object, "viewId")?;
    validate_browser_optional_string(object, "element")?;
    validate_browser_optional_string(object, "expectedGeneration")?;
    if let Some(generation) = object.get("expectedGeneration").and_then(Value::as_str) {
        if generation.is_empty()
            || generation.len() > 240
            || generation.chars().any(char::is_control)
        {
            return Err("Remote Browser expectedGeneration must be a non-empty string of at most 240 non-control characters".into());
        }
    }

    // Desktop Auto-review re-checks the reviewed display state before a mutating
    // browser action. Android binds review, approval identity, and Remote dispatch
    // to the exact view generation so stale-page execution fails closed remotely.
    if remote_browser_requires_generation_fence(name, args) {
        validate_browser_required_string(object, name, "viewId")?;
        validate_browser_required_string(object, name, "expectedGeneration")?;
    }

    match name {
        "browser_navigate" => {
            validate_browser_required_string(object, name, "url")?;
            validate_browser_optional_bool(object, "newTab")?;
        }
        "browser_refresh" | "browser_back" | "browser_forward" => {
            validate_browser_required_string(object, name, "viewId")?;
            validate_browser_required_string(object, name, "expectedGeneration")?;
        }
        "browser_snapshot" => {
            validate_browser_optional_bool(object, "interactive")?;
            validate_browser_optional_number(object, "maxDepth")?;
            validate_browser_optional_string(object, "selector")?;
        }
        "browser_click" => {
            validate_browser_required_string(object, name, "ref")?;
            validate_browser_optional_number(object, "offsetX")?;
            validate_browser_optional_number(object, "offsetY")?;
            validate_browser_optional_number(object, "holdDurationMs")?;
            validate_browser_optional_bool(object, "doubleClick")?;
            validate_browser_button(object)?;
            validate_browser_string_array(object, "modifiers", false)?;
        }
        "browser_mouse_click_xy" => {
            for key in ["x", "y"] {
                if object.get(key).and_then(Value::as_f64).is_none() {
                    return Err(format!("Remote Browser {name} requires numeric {key}"));
                }
            }
            validate_browser_button(object)?;
        }
        "browser_download" => {
            validate_browser_required_string(object, name, "ref")?;
        }
        "browser_upload_file" => {
            validate_browser_required_string(object, name, "ref")?;
            let path = object
                .get("filePath")
                .and_then(Value::as_str)
                .ok_or("Remote Browser browser_upload_file requires filePath")?;
            if path.is_empty()
                || path.len() > MAX_BROWSER_UPLOAD_PATH
                || path.chars().any(char::is_control)
                || !path.starts_with("/workspace/")
                || path.split('/').any(|part| matches!(part, "." | ".."))
            {
                return Err("Remote Browser upload filePath must be a confined /workspace path of at most 4096 non-control characters".into());
            }
        }
        "browser_type" => {
            validate_browser_required_string(object, name, "ref")?;
            validate_browser_required_string(object, name, "text")?;
            for key in ["clear", "slowly", "submit"] {
                validate_browser_optional_bool(object, key)?;
            }
        }
        "browser_fill" => {
            validate_browser_required_string(object, name, "ref")?;
            validate_browser_required_string(object, name, "value")?;
        }
        "browser_select_option" => {
            validate_browser_required_string(object, name, "ref")?;
            validate_browser_string_array(object, "values", true)?;
        }
        "browser_press_key" => {
            validate_browser_required_string(object, name, "key")?;
        }
        "browser_scroll" => {
            validate_browser_optional_string(object, "ref")?;
            for key in ["amount", "deltaX", "deltaY"] {
                validate_browser_optional_number(object, key)?;
            }
            if let Some(direction) = object.get("direction") {
                if !matches!(direction.as_str(), Some("up" | "down" | "left" | "right")) {
                    return Err("Remote Browser scroll direction is unsupported".into());
                }
            }
        }
        "browser_drag" => {
            validate_browser_required_string(object, name, "sourceRef")?;
            validate_browser_optional_string(object, "targetRef")?;
            validate_browser_optional_number(object, "targetX")?;
            validate_browser_optional_number(object, "targetY")?;
            let has_target_ref = object
                .get("targetRef")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty());
            let has_target_xy = object.get("targetX").and_then(Value::as_f64).is_some()
                && object.get("targetY").and_then(Value::as_f64).is_some();
            if !has_target_ref && !has_target_xy {
                return Err("Remote Browser drag requires targetRef or targetX/targetY".into());
            }
        }
        "browser_get_bounding_box" => {
            validate_browser_required_string(object, name, "ref")?;
        }
        "browser_highlight" => {
            validate_browser_required_string(object, name, "ref")?;
            validate_browser_optional_number(object, "durationMs")?;
        }
        "browser_cdp" => {
            validate_browser_required_string(object, name, "method")?;
            let method = object
                .get("method")
                .and_then(Value::as_str)
                .ok_or("Remote Browser CDP method is required")?;
            validate_browser_cdp_method(method)?;
            if object.get("params").is_some_and(|value| !value.is_object()) {
                return Err("Remote Browser CDP params must be an object".into());
            }
        }
        "browser_tabs" => {
            let action = object
                .get("action")
                .and_then(Value::as_str)
                .ok_or("Remote Browser tabs action is required")?;
            if !matches!(action, "list" | "new" | "close" | "select") {
                return Err("Remote Browser tabs action is unsupported".into());
            }
            if let Some(index) = object.get("index") {
                if index.as_u64().is_none() {
                    return Err("Remote Browser tabs index must be a non-negative integer".into());
                }
            }
            if action == "select" && object.get("index").and_then(Value::as_u64).is_none() {
                return Err("Remote Browser tabs select requires index".into());
            }
        }
        "browser_take_screenshot" => {
            validate_browser_optional_bool(object, "fullPage")?;
        }
        _ => return Err("unsupported Remote Browser tool".into()),
    }

    // Desktop's frozen browser driver owns a 90s watchdog. Preserve that execution
    // budget in the authorized Remote Runner request. Transport uncertainty remains
    // outcome-unknown and is reconciled by stable operation/request identity.
    Ok(REMOTE_BROWSER_EXECUTION_TIMEOUT_MS)
}

fn validate_remote_cloud_agent_input(object: &serde_json::Map<String, Value>) -> Result<u64, String> {
    const KEYS: &[&str] = &[
        "action", "prompt", "images", "repo_url", "starting_ref", "model", "model_params",
        "title", "environment", "interrupt", "agent_id", "scope", "include_archived", "limit",
        "confirm",
    ];
    if let Some(key) = object.keys().find(|key| !KEYS.contains(&key.as_str())) {
        return Err(format!("Remote CloudAgent argument is unsupported: {key}"));
    }
    let action = object.get("action").and_then(Value::as_str).map(str::trim)
        .filter(|value| !value.is_empty()).ok_or("Remote CloudAgent action is required")?;
    if !CLOUD_AGENT_ACTIONS.contains(&action) {
        return Err(format!("Remote CloudAgent action is unsupported: {action}"));
    }
    if object.get("confirm").is_some_and(|value| !value.is_boolean()) {
        return Err("Remote CloudAgent confirm must be boolean".into());
    }
    if object.get("interrupt").is_some_and(|value| !value.is_boolean()) {
        return Err("Remote CloudAgent interrupt must be boolean".into());
    }
    if object.get("include_archived").is_some_and(|value| !value.is_boolean()) {
        return Err("Remote CloudAgent include_archived must be boolean".into());
    }
    if let Some(limit) = object.get("limit") {
        let limit = limit.as_u64().ok_or("Remote CloudAgent limit must be a positive integer")?;
        if limit == 0 {
            return Err("Remote CloudAgent limit must be positive".into());
        }
    }
    if let Some(scope) = object.get("scope") {
        let scope = scope.as_str().ok_or("Remote CloudAgent scope must be a string")?;
        if !matches!(scope, "launched" | "all") {
            return Err("Remote CloudAgent scope is unsupported".into());
        }
    }
    for key in ["prompt", "repo_url", "starting_ref", "model", "title", "agent_id"] {
        if let Some(value) = object.get(key) {
            let value = value.as_str().ok_or_else(|| format!("Remote CloudAgent {key} must be a string"))?;
            if value.len() > 65_536 || value.chars().any(|c| c == '\0') {
                return Err(format!("Remote CloudAgent {key} is invalid or too large"));
            }
        }
    }
    if let Some(environment) = object.get("environment") {
        let environment = environment
            .as_object()
            .ok_or("Remote CloudAgent environment must be an object")?;
        const ENV_KEYS: &[&str] = &["type", "name", "id", "team_id"];
        if let Some(key) = environment.keys().find(|key| !ENV_KEYS.contains(&key.as_str())) {
            return Err(format!("Remote CloudAgent environment argument is unsupported: {key}"));
        }
        let kind = environment
            .get("type")
            .and_then(Value::as_str)
            .ok_or("Remote CloudAgent environment.type is required")?;
        if !matches!(kind, "cloud" | "pool" | "machine" | "environment") {
            return Err("Remote CloudAgent environment.type is unsupported".into());
        }
        if environment.get("name").is_some_and(|value| !value.is_string())
            || environment.get("id").is_some_and(|value| !value.is_string())
            || environment.get("team_id").is_some_and(|value| value.as_i64().is_none())
        {
            return Err("Remote CloudAgent environment fields are malformed".into());
        }
        if kind == "machine"
            && environment.get("name").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).is_none()
        {
            return Err("Remote CloudAgent machine environment requires name".into());
        }
        if kind == "environment"
            && environment.get("name").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).is_none()
            && environment.get("id").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).is_none()
        {
            return Err("Remote CloudAgent saved environment requires name or id".into());
        }
    }
    if let Some(params) = object.get("model_params") {
        let params = params.as_object().ok_or("Remote CloudAgent model_params must be an object")?;
        if params.len() > 64 || params.values().any(|value| !value.is_string()) {
            return Err("Remote CloudAgent model_params must contain bounded string values".into());
        }
        if !params.is_empty()
            && object
                .get("model")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
        {
            return Err("Remote CloudAgent model is required when model_params are provided".into());
        }
        if params.iter().any(|(key, value)| {
            key.trim().is_empty()
                || key.len() > 512
                || key.chars().any(char::is_control)
                || value
                    .as_str()
                    .is_some_and(|value| value.len() > 4096 || value.chars().any(char::is_control))
        }) {
            return Err("Remote CloudAgent model_params contain an invalid id or value".into());
        }
    }
    if let Some(images) = object.get("images") {
        let images = images.as_array().ok_or("Remote CloudAgent images must be an array")?;
        for (index, image) in images.iter().enumerate() {
            let image = image.as_object().ok_or_else(|| format!("Remote CloudAgent images[{index}] must be an object"))?;
            if image.len() != 1 || !image.contains_key("url") {
                return Err(format!("Remote CloudAgent images[{index}] only supports url"));
            }
            let url = image.get("url").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty())
                .ok_or_else(|| format!("Remote CloudAgent images[{index}].url is required"))?;
            if url.len() > 4096 || url.chars().any(|c| c == '\0') {
                return Err(format!("Remote CloudAgent images[{index}].url is invalid"));
            }
            let parsed = url::Url::parse(url)
                .map_err(|_| format!("Remote CloudAgent images[{index}].url is not a valid URL"))?;
            if parsed.scheme() != "file" || parsed.to_file_path().is_err() {
                return Err(format!(
                    "Remote CloudAgent images[{index}].url must be an absolute file:// URL"
                ));
            }
        }
    }
    match action {
        "launch" => {
            let prompt = object.get("prompt").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty())
                .ok_or("Remote CloudAgent launch prompt is required")?;
            if prompt.len() > 65_536 { return Err("Remote CloudAgent launch prompt is too large".into()); }
        }
        "reply" => {
            required_cloud_agent_identity(object, "agent_id", action)?;
            let prompt = object.get("prompt").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty())
                .ok_or("Remote CloudAgent reply prompt is required")?;
            if prompt.len() > 65_536 { return Err("Remote CloudAgent reply prompt is too large".into()); }
        }
        "get" | "dump" | "watch" | "rename" | "cancel" | "archive" | "unarchive" | "delete" | "list_artifacts" => {
            required_cloud_agent_identity(object, "agent_id", action)?;
            if action == "rename" { required_cloud_agent_identity(object, "title", action)?; }
        }
        "list" | "models" => {}
        _ => unreachable!("validated CloudAgent action"),
    }
    Ok(REMOTE_CLOUD_AGENT_EXECUTION_TIMEOUT_MS)
}

fn required_cloud_agent_identity(
    object: &serde_json::Map<String, Value>, key: &str, action: &str,
) -> Result<(), String> {
    let value = object.get(key).and_then(Value::as_str).map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Remote CloudAgent {action} requires {key}"))?;
    if value.len() > 512 || value.chars().any(char::is_control) {
        return Err(format!("Remote CloudAgent {key} identity is invalid"));
    }
    Ok(())
}

fn validate_remote_computer_action(args: &Value, allow_followups: bool) -> Result<(), String> {
    const ACTIONS: &[&str] = &["screenshot", "click", "move", "drag", "type", "key", "scroll", "wait"];
    const KEYS: &[&str] = &[
        "action", "x", "y", "x2", "y2", "path", "text", "key", "button", "count",
        "direction", "amount", "durationMs", "description", "then",
    ];
    let object = args.as_object().ok_or("Remote Computer arguments must be an object")?;
    if let Some(key) = object.keys().find(|key| !KEYS.contains(&key.as_str())) {
        return Err(format!("Remote Computer argument is unsupported: {key}"));
    }
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .ok_or("Remote Computer action is required")?;
    if !ACTIONS.contains(&action) {
        return Err(format!("Remote Computer action is unsupported: {action}"));
    }
    for key in ["x", "y", "x2", "y2", "amount"] {
        if object.get(key).is_some_and(|value| value.as_i64().is_none()) {
            return Err(format!("Remote Computer {key} must be an integer"));
        }
    }
    if object.get("text").is_some_and(|value| !value.is_string())
        || object.get("key").is_some_and(|value| !value.is_string())
        || object.get("description").is_some_and(|value| !value.is_string())
    {
        return Err("Remote Computer text/key/description must be strings".into());
    }
    if let Some(button) = object.get("button") {
        if !matches!(button.as_str(), Some("left" | "right" | "middle")) {
            return Err("Remote Computer button is unsupported".into());
        }
    }
    if let Some(direction) = object.get("direction") {
        if !matches!(direction.as_str(), Some("up" | "down" | "left" | "right")) {
            return Err("Remote Computer direction is unsupported".into());
        }
    }
    if let Some(count) = object.get("count") {
        if !matches!(count.as_u64(), Some(1..=3)) {
            return Err("Remote Computer count must be between 1 and 3".into());
        }
    }
    if let Some(duration) = object.get("durationMs") {
        if !matches!(duration.as_u64(), Some(0..=30_000)) {
            return Err("Remote Computer durationMs must be between 0 and 30000".into());
        }
    }
    let path_valid = object.get("path").map(|value| {
        value.as_array().is_some_and(|points| {
            points.len() >= 2 && points.iter().all(|point| {
                point.as_object().is_some_and(|point| {
                    point.len() == 2
                        && point.get("x").and_then(Value::as_i64).is_some()
                        && point.get("y").and_then(Value::as_i64).is_some()
                })
            })
        })
    });
    if object.get("path").is_some() && path_valid != Some(true) {
        return Err("Remote Computer drag path must contain at least two integer x/y points".into());
    }
    if action == "drag"
        && path_valid != Some(true)
        && !["x", "y", "x2", "y2"]
            .iter()
            .all(|key| object.get(*key).and_then(Value::as_i64).is_some())
    {
        return Err("Remote Computer drag requires x/y/x2/y2 or a path".into());
    }
    match object.get("then") {
        None => {}
        Some(_) if !allow_followups => {
            return Err("Remote Computer nested then actions are not supported".into());
        }
        Some(value) => {
            let followups = value.as_array().ok_or("Remote Computer then must be an array")?;
            if followups.is_empty() || followups.len() > 9 {
                return Err("Remote Computer then must contain between 1 and 9 actions".into());
            }
            for followup in followups {
                if followup.get("action").and_then(Value::as_str) == Some("screenshot") {
                    return Err("Remote Computer screenshot is not allowed inside then".into());
                }
                validate_remote_computer_action(followup, false)?;
            }
        }
    }
    Ok(())
}

fn parse_remote_output(output: &str) -> Value {
    serde_json::from_str(output).unwrap_or_else(|_| Value::String(output.to_string()))
}

fn parse_remote_tool_output(name: &str, args: &Value, output: &str) -> Result<Value, String> {
    let parsed = parse_remote_output(output);
    if !is_remote_browser_tool(name) {
        return Ok(parsed);
    }
    let object = parsed
        .as_object()
        .ok_or("Remote Browser result must use the Desktop BrowserToolExecutor object contract")?;
    if object.get("text").and_then(Value::as_str).is_none() {
        return Err("Remote Browser result omitted string text".into());
    }
    let is_error = object
        .get("isError")
        .and_then(Value::as_bool)
        .ok_or("Remote Browser result omitted boolean isError")?;
    match object.get("imageB64") {
        Some(Value::Null) | Some(Value::String(_)) => {}
        Some(_) => return Err("Remote Browser imageB64 must be a string or null".into()),
        None => return Err("Remote Browser result omitted imageB64".into()),
    }
    if matches!(name, "browser_refresh" | "browser_back" | "browser_forward") && !is_error {
        let requested = args
            .as_object()
            .ok_or("Remote Browser fenced navigation arguments must be an object")?;
        let expected_view_id = requested
            .get("viewId")
            .and_then(Value::as_str)
            .ok_or("Remote Browser fenced navigation requires viewId")?;
        let expected_generation = requested
            .get("expectedGeneration")
            .and_then(Value::as_str)
            .ok_or("Remote Browser fenced navigation requires expectedGeneration")?;
        let returned_view_id = object
            .get("viewId")
            .and_then(Value::as_str)
            .ok_or("Remote Browser fenced navigation result omitted viewId")?;
        let returned_generation = object
            .get("generation")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or("Remote Browser fenced navigation result omitted generation")?;
        if returned_view_id != expected_view_id {
            return Err("Remote Browser fenced navigation returned a stale/different viewId".into());
        }
        if returned_generation == expected_generation {
            return Err("Remote Browser fenced navigation did not advance generation".into());
        }
    }
    Ok(parsed)
}

fn remote_execution_error(error: ExecutionError) -> String {
    match &error {
        ExecutionError::TimedOut
        | ExecutionError::CapabilityUnavailable(_)
        | ExecutionError::Transport(_) => {
            format!("{} ({error:?})", remote_box_not_ready_message(&error))
        }
        _ => format!("{error:?}"),
    }
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

    struct NeverCalledDelegate;

    impl AndroidRoutedToolBridge for NeverCalledDelegate {
        fn list_tools(&self) -> Result<Vec<Value>, String> {
            Ok(Vec::new())
        }

        fn call_tool(&self, _name: &str, _args: Value, _tool_call_id: &str) -> Result<Value, String> {
            panic!("policy-gated Remote tools must not fall through to the delegate")
        }
    }

    #[test]
    fn pending_ids_are_account_fenced() {
        let registry = RemoteApprovalRegistry::default();
        let make_wait = |account_fence: &str, suffix: &str| RemoteApprovalWait {
            parent_operation_id: format!("parent-{suffix}"),
            remote_operation_id: format!("remote-{suffix}"),
            request_id: format!("request-{suffix}"),
            capability: "remote.shell".into(),
            refusal_id: format!("refusal-{suffix}"),
            account_fence: account_fence.into(),
            account_epoch: 7,
            device_id: "device-a".into(),
            signal: Arc::new((Mutex::new(None), Condvar::new())),
        };
        registry.register("approval-a1", make_wait("account-a", "a1")).unwrap();
        registry.register("approval-b1", make_wait("account-b", "b1")).unwrap();
        registry.register("approval-a2", make_wait("account-a", "a2")).unwrap();

        assert_eq!(
            registry.pending_ids_for_account("account-a").unwrap(),
            vec!["approval-a1".to_string(), "approval-a2".to_string()],
        );
        assert_eq!(
            registry.pending_ids_for_account("account-b").unwrap(),
            vec!["approval-b1".to_string()],
        );
        assert!(registry.pending_ids_for_account("account-c").unwrap().is_empty());
    }

    #[test]
    fn remote_box_errors_preserve_readiness_and_no_monitor_taxonomy() {
        assert!(remote_execution_error(ExecutionError::TimedOut)
            .starts_with(crate::box_readiness::SAND_BOX_NOT_RESPONDING_MESSAGE));
        assert!(remote_execution_error(ExecutionError::Transport(
            "connection refused".into(),
        ))
        .starts_with(crate::box_readiness::SAND_BOX_NOT_READY_MESSAGE));
        assert!(remote_execution_error(ExecutionError::CapabilityUnavailable(
            "no private desktop monitor is available".into(),
        ))
        .starts_with(crate::box_readiness::SAND_BOX_NO_MONITOR_AVAILABLE_MESSAGE));
    }

    #[test]
    fn approval_wait_ttl_matches_desktop_local_tool_contract() {
        assert_eq!(APPROVAL_WAIT_TIMEOUT, Duration::from_secs(10 * 60));
    }

    #[test]
    fn never_permission_fails_closed_before_remote_binding() {
        let root = tempfile::tempdir().unwrap();
        let mut settings =
            AndroidUserSettings::open(root.path().join("user-settings.json")).unwrap();
        settings.set_local_tool_permission_for("acct", "never").unwrap();
        let tools = RemoteRoutedTools {
            delegate: Arc::new(NeverCalledDelegate),
            broker: SharedCapabilityBroker::open(root.path().join("broker.json"), 1).unwrap(),
            binding: Arc::new(Mutex::new(None)),
            runner: Arc::new(Mutex::new(None)),
            approvals: RemoteApprovalRegistry::default(),
            user_settings: Arc::new(Mutex::new(settings)),
            local_tool_permission_ceiling: None,
            live_account_fence: Arc::new(Mutex::new(Some("acct".into()))),
            events: Arc::new(Mutex::new(VecDeque::new())),
            parent_operation_id: "parent-operation".into(),
            parent_request_id: "parent-request".into(),
            cancelled: Arc::new(AtomicBool::new(false)),
            client_paused: Arc::new(AtomicBool::new(false)),
            auto_review_preflight: None,
            browser_navigation_observer: None,
        };

        let error = tools
            .call_tool("Shell", json!({"command":"echo denied"}), "tool-call-1")
            .unwrap_err();
        assert!(error.contains("disabled by the current Host permission policy"));
    }

    #[test]
    fn client_pause_hides_remote_tools_and_refuses_only_new_dispatch() {
        let root = tempfile::tempdir().unwrap();
        let settings = AndroidUserSettings::open(root.path().join("user-settings.json")).unwrap();
        let paused = Arc::new(AtomicBool::new(true));
        let tools = RemoteRoutedTools {
            delegate: Arc::new(NeverCalledDelegate),
            broker: SharedCapabilityBroker::open(root.path().join("broker.json"), 1).unwrap(),
            binding: Arc::new(Mutex::new(None)),
            runner: Arc::new(Mutex::new(None)),
            approvals: RemoteApprovalRegistry::default(),
            user_settings: Arc::new(Mutex::new(settings)),
            local_tool_permission_ceiling: None,
            live_account_fence: Arc::new(Mutex::new(None)),
            events: Arc::new(Mutex::new(VecDeque::new())),
            parent_operation_id: "parent-operation".into(),
            parent_request_id: "parent-request".into(),
            cancelled: Arc::new(AtomicBool::new(false)),
            client_paused: Arc::clone(&paused),
            auto_review_preflight: None,
            browser_navigation_observer: None,
        };

        assert!(tools.list_tools().unwrap().is_empty());
        assert!(tools
            .refuse_new_dispatch_if_paused()
            .unwrap_err()
            .contains("sand_client_pause"));
        paused.store(false, Ordering::Release);
        assert!(tools.refuse_new_dispatch_if_paused().is_ok());
    }

    #[test]
    fn binding_rejects_wrong_epoch_and_untrusted_plaintext_endpoint() {
        let invalid_epoch = json!({
            "credentialPlane":"authorized-remote-runner-v1",
            "credentialId":"runner-credential-1",
            "issuedAtMs":1,
            "expiresAtMs":4102444800000_u64,
            "endpoint":"https://remote.example.com",
            "bearerCredential":"long-enough-credential",
            "deviceId":"device-1",
            "accountFence":"session:a",
            "accountEpoch":0,
            "executors":["shell","read","computer","screenshot","external-shell","external-read"]
        }).to_string();
        assert!(RemoteDispatchBinding::parse(&invalid_epoch).is_err());

        let plaintext = json!({
            "credentialPlane":"authorized-remote-runner-v1",
            "credentialId":"runner-credential-1",
            "issuedAtMs":1,
            "expiresAtMs":4102444800000_u64,
            "endpoint":"http://remote.example.com",
            "bearerCredential":"long-enough-credential",
            "deviceId":"device-1",
            "accountFence":"session:a",
            "accountEpoch":7,
            "executors":["shell","read","computer","screenshot","external-shell","external-read"]
        }).to_string();
        assert!(RemoteDispatchBinding::parse(&plaintext).is_err());
    }

    #[test]
    fn write_executor_is_bounded_and_explicitly_scoped() {
        let binding = RemoteDispatchBinding::parse(&json!({
            "credentialPlane":"authorized-remote-runner-v1",
            "credentialId":"runner-credential-write",
            "issuedAtMs":1,
            "expiresAtMs":4102444800000_u64,
            "endpoint":"https://remote.example.com",
            "bearerCredential":"long-enough-credential",
            "deviceId":"device-write",
            "accountFence":"session:a",
            "accountEpoch":7,
            "executors":["write"]
        }).to_string()).unwrap();
        assert!(binding.supports("write"));
        assert_eq!(
            validate_remote_tool_input("Write", &json!({"path":"workspace/note.txt","content":"hello"})),
            Ok(30_000),
        );
        assert!(validate_remote_tool_input("Write", &json!({"path":"","content":"hello"})).is_err());
        assert!(validate_remote_tool_input(
            "Write",
            &json!({"path":"workspace/note.txt","content":"x".repeat(MAX_WRITE_CONTENT + 1)}),
        ).is_err());
        assert!(validate_remote_tool_input(
            "Write",
            &json!({"path":"workspace/note.txt","content":"hello","mode":"append"}),
        ).is_err());
    }

    #[test]
    fn file_transfer_executor_is_explicit_bounded_and_device_fenced() {
        let binding = RemoteDispatchBinding::parse(&json!({
            "credentialPlane":"authorized-remote-runner-v1",
            "credentialId":"runner-credential-transfer",
            "issuedAtMs":1,
            "expiresAtMs":4102444800000_u64,
            "endpoint":"https://remote.example.com",
            "bearerCredential":"long-enough-credential",
            "deviceId":"device-transfer",
            "accountFence":"session:a",
            "accountEpoch":7,
            "executors":["file-transfer"]
        }).to_string()).unwrap();
        assert!(binding.supports("file-transfer"));
        assert_eq!(
            validate_remote_tool_input(
                COPY_TO_BOX_TOOL_NAME,
                &json!({"computer_path":"docs/input.bin","box_path":"uploads/input.bin"})
            ).unwrap(),
            120_000,
        );
        assert_eq!(
            validate_remote_tool_input(
                COPY_FROM_BOX_TOOL_NAME,
                &json!({"box_path":"out/report.pdf","computer_path":"report.pdf","overwrite":false})
            ).unwrap(),
            120_000,
        );
        assert!(validate_remote_tool_input(
            COPY_TO_BOX_TOOL_NAME,
            &json!({"computer_path":"x","expected_destination_version":"not-a-digest"})
        ).is_err());
        assert!(validate_remote_tool_input(
            COPY_FROM_BOX_TOOL_NAME,
            &json!({"box_path":"x","unexpected":true})
        ).is_err());
    }

    #[test]
    fn binding_rejects_expired_or_not_yet_active_credential() {
        let binding = RemoteDispatchBinding::parse(&json!({
            "credentialPlane":"authorized-remote-runner-v1",
            "credentialId":"runner-credential-1",
            "issuedAtMs":100,
            "expiresAtMs":200,
            "endpoint":"https://remote.example.com",
            "bearerCredential":"long-enough-credential",
            "deviceId":"device-1",
            "accountFence":"session:a",
            "accountEpoch":7,
            "executors":["computer"]
        }).to_string()).unwrap();
        assert!(binding.active_at(99).is_err());
        assert!(binding.active_at(100).is_ok());
        assert!(binding.active_at(199).is_ok());
        assert!(binding.active_at(200).is_err());
    }

    #[test]
    fn binding_rejects_credentials_from_unrelated_remote_control_planes() {
        for credential_plane in [
            "computer-client-token-v1",
            "computer-mobile-token-v1",
            "computer-device-secret-v1",
            "codex-remote-control-token-v1",
        ] {
            let raw = json!({
                "credentialPlane":credential_plane,
                "credentialId":"runner-credential-1",
                "issuedAtMs":1,
                "expiresAtMs":4102444800000_u64,
                "endpoint":"https://remote.example.com",
                "bearerCredential":"long-enough-credential",
                "deviceId":"device-1",
                "accountFence":"session:a",
                "accountEpoch":7,
                "executors":["computer"]
            }).to_string();
            assert!(
                RemoteDispatchBinding::parse(&raw).is_err(),
                "{credential_plane} must never authorize Remote Runner dispatch",
            );
        }
    }

    #[test]
    fn binding_requires_explicit_known_executor_capabilities() {
        let missing = json!({
            "credentialPlane":"authorized-remote-runner-v1",
            "credentialId":"runner-credential-1",
            "issuedAtMs":1,
            "expiresAtMs":4102444800000_u64,
            "endpoint":"https://remote.example.com",
            "bearerCredential":"long-enough-credential",
            "deviceId":"device-1",
            "accountFence":"session:a",
            "accountEpoch":7,
            "executors":[]
        }).to_string();
        assert!(RemoteDispatchBinding::parse(&missing).is_err());

        let unknown = json!({
            "credentialPlane":"authorized-remote-runner-v1",
            "credentialId":"runner-credential-1",
            "issuedAtMs":1,
            "expiresAtMs":4102444800000_u64,
            "endpoint":"https://remote.example.com",
            "bearerCredential":"long-enough-credential",
            "deviceId":"device-1",
            "accountFence":"session:a",
            "accountEpoch":7,
            "executors":["computer","clipboard"]
        }).to_string();
        assert!(RemoteDispatchBinding::parse(&unknown).is_err());

        let scoped = RemoteDispatchBinding::parse(&json!({
            "credentialPlane":"authorized-remote-runner-v1",
            "credentialId":"runner-credential-1",
            "issuedAtMs":1,
            "expiresAtMs":4102444800000_u64,
            "endpoint":"https://remote.example.com",
            "bearerCredential":"long-enough-credential",
            "deviceId":"device-1",
            "accountFence":"session:a",
            "accountEpoch":7,
            "executors":["computer","screenshot","cloud-agent"]
        }).to_string()).unwrap();
        assert!(scoped.supports("computer"));
        assert!(scoped.supports("screenshot"));
        assert!(scoped.supports("cloud-agent"));
        assert!(!scoped.supports("shell"));
        assert!(!scoped.supports("external-shell"));
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
                    refusal_id: "refusal-1".into(),
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
    fn denied_remote_action_is_remembered_for_the_turn_but_not_other_turns() {
        let root = tempfile::tempdir().unwrap();
        let broker = SharedCapabilityBroker::open(root.path().join("broker.json"), 1).unwrap();
        broker
            .request_remote_approval(
                "approval-refused",
                "request-refused",
                "remote-op-refused",
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
                "approval-refused",
                RemoteApprovalWait {
                    parent_operation_id: "turn-a".into(),
                    remote_operation_id: "remote-op-refused".into(),
                    request_id: "request-refused".into(),
                    capability: "remote.shell".into(),
                    refusal_id: "turn-a:shell:echo-no".into(),
                    account_fence: "session:account-a".into(),
                    account_epoch: 7,
                    device_id: "device-1".into(),
                    signal: Arc::new((Mutex::new(None), Condvar::new())),
                },
            )
            .unwrap();

        let resolved = registry
            .resolve_from_ui(
                &broker,
                "approval-refused",
                false,
                "session:account-a",
                2,
            )
            .unwrap()
            .unwrap();
        assert!(!resolved.approved);
        assert!(registry.was_refused("turn-a:shell:echo-no").unwrap());
        assert!(!registry.was_refused("turn-b:shell:echo-no").unwrap());
        assert!(broker
            .local_tool_refusal_reason(
                "turn-a",
                1,
                "remote-tool",
                &json!({"refusalId":"turn-a:shell:echo-no"}),
            )
            .unwrap()
            .is_some());
        drop(broker);
        let reopened = SharedCapabilityBroker::open(root.path().join("broker.json"), 3).unwrap();
        assert!(reopened
            .local_tool_refusal_reason(
                "turn-a",
                1,
                "remote-tool",
                &json!({"refusalId":"turn-a:shell:echo-no"}),
            )
            .unwrap()
            .is_some());

        // Repeated inserts are idempotent and bounded so Host lifetime cannot grow this memory
        // without limit.
        registry
            .remember_refused("turn-a:shell:echo-no".into())
            .unwrap();
        for index in 0..(REMOTE_REFUSED_ACTION_MEMORY + 5) {
            registry
                .remember_refused(format!("other-refusal-{index}"))
                .unwrap();
        }
        assert_eq!(
            registry.refused_actions.lock().unwrap().len(),
            REMOTE_REFUSED_ACTION_MEMORY
        );
    }

    #[test]
    fn pre_dispatch_failure_cancels_allowed_once_remote_grant() {
        let root = tempfile::tempdir().unwrap();
        let broker = SharedCapabilityBroker::open(root.path().join("broker.json"), 1).unwrap();
        broker
            .request_remote_approval(
                "approval-cancel",
                "request-cancel",
                "remote-op-cancel",
                "remote.shell",
                json!({"deviceId":"device-1"}),
                "session:account-a",
                7,
                "device-1",
                1,
            )
            .unwrap();
        broker
            .resolve_approval("approval-cancel", true, "session:account-a", 2)
            .unwrap();

        assert_eq!(
            abandon_unconsumed_remote_approval(
                &broker,
                "remote-op-cancel",
                "binding changed after approval",
            ),
            "binding changed after approval"
        );
        assert!(broker
            .consume_remote_approval_for_dispatch(
                "approval-cancel",
                "remote-op-cancel",
                "request-cancel",
                "remote.shell",
                "session:account-a",
                7,
                "device-1",
                4,
            )
            .is_err());
    }

    #[test]
    fn cloud_agent_remote_wire_is_typed_and_matches_runner_capability() {
        let args = json!({
            "action":"watch",
            "agent_id":"bc-123",
            "include_archived":false
        });
        let encoded = remote_execution_input_json(CLOUD_AGENT_TOOL_NAME, &args);
        let parsed: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(parsed, args);
        assert!(parsed.get("tool").is_none());
        assert!(parsed.get("arguments").is_none());

        let shell = remote_execution_input_json("Shell", &json!({"command":"pwd"}));
        let shell: Value = serde_json::from_str(&shell).unwrap();
        assert_eq!(shell["tool"], "Shell");
        assert_eq!(shell["arguments"]["command"], "pwd");

        let capability = match CLOUD_AGENT_TOOL_NAME {
            CLOUD_AGENT_TOOL_NAME => "cloud-agent",
            _ => unreachable!(),
        };
        assert_eq!(capability, fabushi_android_box_exec_daemon::CLOUD_AGENT_CAPABILITY);
    }

    #[test]
    fn cloud_agent_executor_requires_explicit_enrollment_and_destructive_confirmation() {
        assert_eq!(
            validate_remote_tool_input(
                CLOUD_AGENT_TOOL_NAME,
                &json!({"action":"launch","prompt":"fix CI","repo_url":"https://example/repo"}),
            ).unwrap(),
            REMOTE_CLOUD_AGENT_EXECUTION_TIMEOUT_MS,
        );
        assert!(validate_remote_tool_input(CLOUD_AGENT_TOOL_NAME, &json!({"action":"launch"})).is_err());
        assert_eq!(
            validate_remote_tool_input(CLOUD_AGENT_TOOL_NAME, &json!({"action":"delete","agent_id":"bc-1"})).unwrap(),
            REMOTE_CLOUD_AGENT_EXECUTION_TIMEOUT_MS,
        );
        assert_eq!(
            validate_remote_tool_input(CLOUD_AGENT_TOOL_NAME, &json!({"action":"delete","agent_id":"bc-1","confirm":true})).unwrap(),
            REMOTE_CLOUD_AGENT_EXECUTION_TIMEOUT_MS,
        );
        assert!(validate_remote_tool_input(CLOUD_AGENT_TOOL_NAME, &json!({"action":"reply","agent_id":"bc-1"})).is_err());
        assert!(validate_remote_tool_input(
            CLOUD_AGENT_TOOL_NAME,
            &json!({"action":"launch","prompt":"x","model_params":{"fast":"true"}}),
        ).is_err());
        assert_eq!(
            validate_remote_tool_input(
                CLOUD_AGENT_TOOL_NAME,
                &json!({"action":"launch","prompt":"x","model":"grok-4.5","model_params":{"fast":"true"}}),
            ).unwrap(),
            REMOTE_CLOUD_AGENT_EXECUTION_TIMEOUT_MS,
        );
        assert!(validate_remote_tool_input(
            CLOUD_AGENT_TOOL_NAME,
            &json!({"action":"launch","prompt":"x","images":[{"url":"https://example.com/a.png"}]}),
        ).is_err());
        assert_eq!(
            validate_remote_tool_input(
                CLOUD_AGENT_TOOL_NAME,
                &json!({"action":"launch","prompt":"x","images":[{"url":"file:///workspace/a.png"}]}),
            ).unwrap(),
            REMOTE_CLOUD_AGENT_EXECUTION_TIMEOUT_MS,
        );
        assert!(validate_remote_tool_input(CLOUD_AGENT_TOOL_NAME, &json!({"action":"unknown"})).is_err());
    }

    #[test]
    fn browser_executor_matches_desktop_tool_names_and_required_contracts() {
        let definitions = remote_browser_tool_definitions();
        let names = definitions
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<BTreeSet<_>>();
        assert_eq!(names.len(), REMOTE_BROWSER_TOOL_NAMES.len());
        assert!(REMOTE_BROWSER_TOOL_NAMES.iter().all(|name| names.contains(name)));

        assert_eq!(
            validate_remote_tool_input(
                "browser_navigate",
                &json!({"url":"https://example.com","newTab":true}),
            )
            .unwrap(),
            REMOTE_BROWSER_EXECUTION_TIMEOUT_MS,
        );
        assert!(validate_remote_tool_input("browser_navigate", &json!({})).is_err());
        assert!(validate_remote_tool_input("browser_navigate", &json!({"url":42})).is_err());

        assert!(validate_remote_tool_input(
            "browser_mouse_click_xy",
            &json!({"x":"12","y":34}),
        )
        .is_err());
        assert!(validate_remote_tool_input(
            "browser_select_option",
            &json!({"ref":"ref-1","values":["one",2]}),
        )
        .is_err());
        assert!(validate_remote_tool_input(
            "browser_drag",
            &json!({"sourceRef":"ref-1"}),
        )
        .is_err());
        assert_eq!(
            validate_remote_tool_input(
                "browser_drag",
                &json!({"sourceRef":"ref-1","targetX":12.5,"targetY":9,"viewId":"view-a","expectedGeneration":"generation-7"}),
            )
            .unwrap(),
            REMOTE_BROWSER_EXECUTION_TIMEOUT_MS,
        );

        for name in ["browser_download", "browser_upload_file"] {
            assert!(validate_remote_tool_input(name, &json!({"ref":"ref-1"})).is_err());
        }
        assert_eq!(
            validate_remote_tool_input(
                "browser_download",
                &json!({"ref":"ref-1","viewId":"view-a","expectedGeneration":"generation-7"}),
            )
            .unwrap(),
            REMOTE_BROWSER_EXECUTION_TIMEOUT_MS,
        );
        assert_eq!(
            validate_remote_tool_input(
                "browser_upload_file",
                &json!({"ref":"ref-1","filePath":"/workspace/upload.txt","viewId":"view-a","expectedGeneration":"generation-7"}),
            )
            .unwrap(),
            REMOTE_BROWSER_EXECUTION_TIMEOUT_MS,
        );
        for path in ["/tmp/outside.txt", "/workspace/../escape.txt", "/workspace/bad\nname"] {
            assert!(validate_remote_tool_input(
                "browser_upload_file",
                &json!({"ref":"ref-1","filePath":path,"viewId":"view-a","expectedGeneration":"generation-7"}),
            )
            .is_err(), "unsafe upload path must fail closed: {path}");
        }

        assert!(validate_remote_tool_input(
            "browser_cdp",
            &json!({"method":"Storage.clearDataForOrigin","params":{}}),
        )
        .is_err());
        assert!(validate_remote_tool_input(
            "browser_cdp",
            &json!({"method":"Network.setCookie","params":{}}),
        )
        .is_err());
        assert_eq!(
            validate_remote_tool_input(
                "browser_cdp",
                &json!({"method":"Runtime.evaluate","params":{"expression":"location.href"},"viewId":"view-a","expectedGeneration":"generation-7"}),
            )
            .unwrap(),
            REMOTE_BROWSER_EXECUTION_TIMEOUT_MS,
        );
        assert!(validate_remote_tool_input(
            "browser_cdp",
            &json!({"method":"Runtime.evaluate","params":"not-an-object"}),
        )
        .is_err());

        assert_eq!(
            validate_remote_tool_input("browser_tabs", &json!({"action":"list"})).unwrap(),
            REMOTE_BROWSER_EXECUTION_TIMEOUT_MS,
        );
        assert!(validate_remote_tool_input("browser_tabs", &json!({"action":"destroy"})).is_err());
        assert!(validate_remote_tool_input("browser_tabs", &json!({"action":"select"})).is_err());
        assert!(validate_remote_tool_input(
            "browser_click",
            &json!({"ref":"ref-1","viewId":42}),
        )
        .is_err());
        assert!(validate_remote_tool_input(
            "browser_click",
            &json!({"ref":"ref-1"}),
        )
        .is_err());
        assert_eq!(
            validate_remote_tool_input(
                "browser_click",
                &json!({"ref":"ref-1","viewId":"view-a","expectedGeneration":"generation-7"}),
            )
            .unwrap(),
            REMOTE_BROWSER_EXECUTION_TIMEOUT_MS,
        );
        assert!(validate_remote_tool_input(
            "browser_tabs",
            &json!({"action":"close","index":0}),
        )
        .is_err());
        assert_eq!(
            validate_remote_tool_input(
                "browser_tabs",
                &json!({"action":"close","index":0,"viewId":"view-a","expectedGeneration":"generation-7"}),
            )
            .unwrap(),
            REMOTE_BROWSER_EXECUTION_TIMEOUT_MS,
        );
        assert!(validate_remote_tool_input(
            "browser_take_screenshot",
            &json!({"fullPage":"yes"}),
        )
        .is_err());
        for name in ["browser_refresh", "browser_back", "browser_forward"] {
            assert!(validate_remote_tool_input(name, &json!({})).is_err());
            assert!(validate_remote_tool_input(
                name,
                &json!({"viewId":"view-a","expectedGeneration":"generation-7"}),
            )
            .is_ok());
            assert!(validate_remote_tool_input(
                name,
                &json!({"viewId":"view-a","expectedGeneration":""}),
            )
            .is_err());
            assert!(validate_remote_tool_input(
                name,
                &json!({"viewId":"view-a","expectedGeneration":"bad\nvalue"}),
            )
            .is_err());
        }

        // Remote Browser credentials belong to the authenticated transport,
        // never to tool input or browser UA/header mutation.
        assert!(validate_remote_tool_input(
            "browser_navigate",
            &json!({"url":"https://example.com","accessToken":"secret"}),
        )
        .is_err());
        for method in [
            "Network.setExtraHTTPHeaders",
            "Network.setUserAgentOverride",
            "Emulation.setUserAgentOverride",
            "Fetch.continueRequest",
        ] {
            assert!(validate_remote_tool_input(
                "browser_cdp",
                &json!({"method":method,"params":{}}),
            )
            .is_err(), "{method} must not bypass Remote Browser credential/UA fencing");
        }
    }

    #[test]
    fn browser_navigation_observer_uses_only_runner_observed_page_identity() {
        let observed = Arc::new(Mutex::new(Vec::<RemoteBrowserNavigationObservation>::new()));
        let observer = {
            let observed = Arc::clone(&observed);
            Arc::new(move |value: RemoteBrowserNavigationObservation| {
                observed.lock().unwrap().push(value);
            }) as RemoteBrowserNavigationObserver
        };
        let root = tempfile::tempdir().unwrap();
        let settings = AndroidUserSettings::open(root.path().join("user-settings.json")).unwrap();
        let tools = RemoteRoutedTools {
            delegate: Arc::new(NeverCalledDelegate),
            broker: SharedCapabilityBroker::open(root.path().join("broker.json"), 1).unwrap(),
            binding: Arc::new(Mutex::new(None)),
            runner: Arc::new(Mutex::new(None)),
            approvals: RemoteApprovalRegistry::default(),
            user_settings: Arc::new(Mutex::new(settings)),
            local_tool_permission_ceiling: None,
            live_account_fence: Arc::new(Mutex::new(None)),
            events: Arc::new(Mutex::new(VecDeque::new())),
            parent_operation_id: "parent".into(),
            parent_request_id: "request".into(),
            cancelled: Arc::new(AtomicBool::new(false)),
            client_paused: Arc::new(AtomicBool::new(false)),
            auto_review_preflight: None,
            browser_navigation_observer: Some(observer),
        };
        tools.observe_browser_navigation(
            "browser_navigate",
            &json!({"text":"ok","imageB64":null,"isError":false,"url":"https://www.google.com/sorry/index?secret=x","pageTitle":"Automated queries"}),
            "remote-op-1",
        );
        tools.observe_browser_navigation(
            "browser_navigate",
            &json!({"text":"ok","imageB64":null,"isError":false}),
            "remote-op-2",
        );
        tools.observe_browser_navigation(
            "browser_navigate",
            &json!({"text":"blocked","imageB64":null,"isError":true,"url":"https://example.com"}),
            "remote-op-3",
        );
        let observed = observed.lock().unwrap();
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].operation_id, "remote-op-1");
        assert_eq!(observed[0].url, "https://www.google.com/sorry/index?secret=x");
        assert_eq!(observed[0].page_title, "Automated queries");
    }

    #[test]
    fn browser_remote_result_must_match_desktop_bridge_shape() {
        let valid = parse_remote_tool_output(
            "browser_snapshot",
            &json!({}),
            r#"{"text":"snapshot","imageB64":null,"isError":false}"#,
        )
        .expect("desktop browser output");
        assert_eq!(valid["text"], "snapshot");
        assert_eq!(valid["isError"], false);

        assert!(parse_remote_tool_output(
            "browser_snapshot",
            &json!({}),
            r#"{"text":"snapshot","isError":false}"#,
        )
        .is_err());
        assert!(parse_remote_tool_output(
            "browser_snapshot",
            &json!({}),
            r#"{"text":"snapshot","imageB64":12,"isError":false}"#,
        )
        .is_err());
        assert!(parse_remote_tool_output(
            "browser_snapshot",
            &json!({}),
            r#"{"text":"snapshot","imageB64":null,"isError":"false"}"#,
        )
        .is_err());
        assert_eq!(
            parse_remote_tool_output("Shell", &json!({}), "plain-text").unwrap(),
            Value::String("plain-text".into()),
        );
    }

    #[test]
    fn browser_history_navigation_rejects_stale_view_and_generation_results() {
        let args = json!({"viewId":"view-a","expectedGeneration":"generation-7"});
        let valid = parse_remote_tool_output(
            "browser_refresh",
            &args,
            r#"{"text":"refreshed","imageB64":null,"isError":false,"viewId":"view-a","generation":"generation-8"}"#,
        )
        .unwrap();
        assert_eq!(valid["generation"], "generation-8");
        assert!(parse_remote_tool_output(
            "browser_back",
            &args,
            r#"{"text":"back","imageB64":null,"isError":false,"viewId":"view-old","generation":"generation-8"}"#,
        )
        .is_err());
        assert!(parse_remote_tool_output(
            "browser_forward",
            &args,
            r#"{"text":"forward","imageB64":null,"isError":false,"viewId":"view-a","generation":"generation-7"}"#,
        )
        .is_err());
        assert!(parse_remote_tool_output(
            "browser_refresh",
            &args,
            r#"{"text":"refresh","imageB64":null,"isError":false,"viewId":"view-a"}"#,
        )
        .is_err());
        // A driver-declared error is terminal input to the Host. Never rewrite it into a
        // synthetic success or retry the navigation side effect.
        assert!(parse_remote_tool_output(
            "browser_refresh",
            &args,
            r#"{"text":"stale generation","imageB64":null,"isError":true}"#,
        )
        .is_ok());
    }

    #[test]
    fn external_machine_validation_matches_desktop_contract() {
        assert_eq!(
            validate_remote_tool_input(
                "ExternalShell",
                &json!({"command":"pwd","workingDirectory":"/tmp"}),
            )
            .unwrap(),
            30_000,
        );
        assert!(validate_remote_tool_input(
            "ExternalShell",
            &json!({"command":"pwd","timeoutMs":1000}),
        )
        .is_err());

        assert_eq!(
            validate_remote_tool_input(
                "ExternalRead",
                &json!({"path":"notes.txt","offset":-1,"limit":0,"encodingHint":"utf-8"}),
            )
            .unwrap(),
            30_000,
        );
        assert!(validate_remote_tool_input(
            "ExternalRead",
            &json!({"path":"notes.txt","limit":-1}),
        )
        .is_err());
        assert!(validate_remote_tool_input(
            "ExternalRead",
            &json!({"path":"notes.txt","extra":true}),
        )
        .is_err());
    }

    #[test]
    fn remote_screenshot_rejects_nonempty_arguments() {
        assert_eq!(validate_remote_tool_input("Screenshot", &json!({})).unwrap(), 30_000);
        assert!(validate_remote_tool_input("Screenshot", &json!({"x":1})).is_err());
    }

    #[test]
    fn remote_computer_validation_matches_desktop_action_bounds() {
        assert!(validate_remote_computer_action(&json!({"action":"screenshot"}), true).is_ok());
        assert!(validate_remote_computer_action(&json!({
            "action":"drag",
            "path":[{"x":1,"y":2},{"x":3,"y":4}],
            "then":[{"action":"wait","durationMs":30000}]
        }), true).is_ok());
        assert!(validate_remote_computer_action(&json!({"action":"drag","x":1,"y":2}), true).is_err());
        assert!(validate_remote_computer_action(&json!({"action":"click","count":4}), true).is_err());
        assert!(validate_remote_computer_action(&json!({
            "action":"click",
            "then":[{"action":"screenshot"}]
        }), true).is_err());
        assert!(validate_remote_computer_action(&json!({
            "action":"wait",
            "then":[{"action":"wait","then":[{"action":"wait"}]}]
        }), true).is_err());
    }

    #[test]
    fn remote_tool_identity_changes_when_arguments_change() {
        let first = sha256_hex(b"parent\ncall\nShell\n{\"command\":\"one\"}");
        let second = sha256_hex(b"parent\ncall\nShell\n{\"command\":\"two\"}");
        assert_ne!(first, second);
    }
}
