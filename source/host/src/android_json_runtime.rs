use crate::account_service::{AccountSessionMutation, AndroidAccountService};
use crate::capability_broker::{CapabilityDecision, PendingCapabilityCall, SharedCapabilityBroker};
use crate::automation_runtime::{run_json as automation_run_json, AutomationRuntime, AutomationSpec};
use crate::android_agent_roster::AndroidAgentRoster;
use crate::android_sidebar_sections::{AndroidSidebarSection, AndroidSidebarSections};
use crate::host_secret_store::get_or_create_host_machine_id;
use crate::host_runner_composition::AuthenticatedRemoteHostRunner;
use crate::messaging_service::AndroidMessagingService;
use crate::plugin_variable_store::{variable_fields_json, PluginVariableStore, PreparedPluginVariableWrite};
use crate::mcp_auth::{
    cleanup_legacy_mcp_auth_credentials, AndroidMcpAuthWatchManager,
    AndroidMcpAuthWatchOwner, CursorDashboardMcpAuthBackend, McpAuthAdminPolicyPort,
    McpAuthBackendPort, McpAuthOwnerEvent, McpAuthenticateResult,
    SandPrivacyMode as BackendSandPrivacyMode,
};
use crate::extensions::transcript::TranscriptStore;
use crate::extensions::webauthn_proxy::{
    WebAuthnBridgeError, WebAuthnProxyExtension, WebAuthnProxyExtensionConfig,
};
use crate::runner::{
    AndroidHostInferenceProvider, AndroidInferenceMode, AndroidSubagentReviewDecision,
    DurableTurnJournal, DurableTurnState,
    ProductionTurnAgentBuildBindings, ProductionTurnAgentLifecycleBindings,
    ProductionTurnAgentOwner, ProductionTurnAgentStaticConfig, ProductionTurnEvent,
    ProductionDiskPressureLevel, ProductionTurnInput, ProductionTurnLifecycleStore,
    ProductionTurnPrivacyMode, ProductionTurnProfileAnnouncementCommit, SAND_AGENT_TOKEN_LIMIT,
    build_turn_subagent_types, parse_turn_subagent_capability_projection,
    COORDINATOR_SUBAGENT_CAPABILITIES_FIELD, DurableSubagentOwner, SubagentFrozenTurnConfig,
    SubagentRunOutcome, SubagentSteerReview, SubagentTaskReviewCallback,
    SubagentSteerReviewCallback, SubagentToolBridge, SubagentToolContext,
    build_parent_subagent_routed_tools, spawn_generated_subagent,
    with_agent_management_tools, with_multitask_todo_tools, with_remote_routed_tools,
    AgentTurnInterruptionRegistry, RemoteApprovalRegistry, RemoteDispatchBinding,
    DurableMultitaskTodoStore,
};
use fabushi_constants::composer::text_size_allowed;
use fabushi_android_shared::node::mcp::mcp_auth_watch_lifecycle::{
    McpAuthPollOutcome, McpAuthPollRequest, McpAuthPollSettlement, McpAuthPollTick,
    McpAuthWatchCompletion,
};
use fabushi_android_shared::webauthn_gateway::{
    WebAuthnCeremony, WebAuthnRequestFrame, WebAuthnResponseFrame, WebAuthnStage,
    WebAuthnStageOutcome,
};
use mahayana_js_runtime::{DeepSeekJsHost, HostEvent};
use fabushi_android_box_exec_daemon::{AuthenticatedRemoteHttpTransport, RemoteTransportPolicy};
use mahayana_plugin_runtime::{
    ExternalReleaseManifest, PermissionManager, PluginInstaller,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ASSISTANT_READ_MARKER_ID: &str = "projection:mahayana-assistant:last-read";

const SUBAGENT_REVIEW_APPROVAL_TTL_MS: u64 = 10 * 60 * 1_000;
const SUBAGENT_REVIEW_MAX_PENDING_PER_AGENT: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubagentReviewApprovalExpiryPolicy {
    Park,
    Ttl,
}

fn subagent_review_approval_expiry_policy(
    request_source: Option<&str>,
) -> SubagentReviewApprovalExpiryPolicy {
    if matches!(request_source, Some("turn" | "handoff-resume")) {
        SubagentReviewApprovalExpiryPolicy::Park
    } else {
        SubagentReviewApprovalExpiryPolicy::Ttl
    }
}

#[derive(Default)]
struct SubagentReviewApprovalRegistry {
    pending: Mutex<BTreeMap<String, SubagentReviewApprovalWaiter>>,
}

struct SubagentReviewApprovalWaiter {
    parent_agent_id: String,
    signal: Arc<(Mutex<Option<bool>>, Condvar)>,
}

impl SubagentReviewApprovalRegistry {
    fn register(
        &self,
        approval_id: &str,
        parent_agent_id: &str,
    ) -> Result<Arc<(Mutex<Option<bool>>, Condvar)>, String> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "subagent review approval registry lock poisoned".to_string())?;
        if pending.contains_key(approval_id) {
            return Err("subagent review approval identity is duplicate".into());
        }
        let count = pending
            .values()
            .filter(|entry| entry.parent_agent_id == parent_agent_id)
            .count();
        if count >= SUBAGENT_REVIEW_MAX_PENDING_PER_AGENT {
            return Err("too many pending subagent review approvals for agent".into());
        }
        let signal = Arc::new((Mutex::new(None), Condvar::new()));
        pending.insert(
            approval_id.to_string(),
            SubagentReviewApprovalWaiter {
                parent_agent_id: parent_agent_id.to_string(),
                signal: Arc::clone(&signal),
            },
        );
        Ok(signal)
    }

    fn contains(&self, approval_id: &str) -> bool {
        self.pending
            .lock()
            .is_ok_and(|pending| pending.contains_key(approval_id))
    }

    fn has_pending_for_agent(&self, parent_agent_id: &str) -> bool {
        self.pending.lock().is_ok_and(|pending| {
            pending
                .values()
                .any(|entry| entry.parent_agent_id == parent_agent_id)
        })
    }

    fn resolve(&self, approval_id: &str, approved: bool) -> Result<(), String> {
        let signal = self
            .pending
            .lock()
            .map_err(|_| "subagent review approval registry lock poisoned".to_string())?
            .get(approval_id)
            .map(|entry| Arc::clone(&entry.signal))
            .ok_or("subagent review approval waiter is unknown")?;
        let (state, wake) = &*signal;
        let mut state = state
            .lock()
            .map_err(|_| "subagent review approval waiter lock poisoned".to_string())?;
        if state.is_some() {
            return Err("subagent review approval was already resolved".into());
        }
        *state = Some(approved);
        wake.notify_all();
        Ok(())
    }

    fn remove(&self, approval_id: &str) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(approval_id);
        }
    }
}

fn wait_for_subagent_review_approval(
    broker: &SharedCapabilityBroker,
    registry: &SubagentReviewApprovalRegistry,
    turn_events: &Arc<Mutex<VecDeque<Value>>>,
    cancelled: &Arc<AtomicBool>,
    parent_agent_id: &str,
    parent_request_id: &str,
    account_fence: &str,
    process_epoch: u64,
    tool_call_id: &str,
    action: &str,
    prompt: &str,
    subagent_id: Option<&str>,
    subagent_type: Option<&str>,
    reason: &str,
    proposed_rule: Option<&str>,
    expiry_policy: SubagentReviewApprovalExpiryPolicy,
) -> Result<bool, String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("cancelled".into());
    }
    let fingerprint = crate::sha256::sha256_hex(
        format!(
            "{account_fence}\n{process_epoch}\n{parent_request_id}\n{tool_call_id}\n{action}\n{prompt}\n{}\n{}",
            subagent_id.unwrap_or_default(),
            subagent_type.unwrap_or_default(),
        )
        .as_bytes(),
    );
    let identity = &fingerprint[..32];
    let approval_id = format!("approval-subagent-review-{identity}");
    let request_id = format!("subagent-review-request-{identity}");
    let operation_id = format!("subagent-review-operation-{identity}");
    let capability = "agent.subagent.review";
    let target = json!({
        "action":"sand_subagent",
        "arguments":{
            "action":action,
            "prompt":prompt,
            "subagent_id":subagent_id,
            "subagent_type":subagent_type,
        },
        "reason":reason,
        "proposedRule":proposed_rule,
    });
    let signal = registry.register(&approval_id, parent_agent_id)?;
    if let Err(error) = broker.request_approval(
        &approval_id,
        &request_id,
        &operation_id,
        capability,
        target.clone(),
        account_fence,
        now_ms(),
    ) {
        registry.remove(&approval_id);
        return Err(error);
    }
    if let Err(error) = turn_events
        .lock()
        .map_err(|_| "turn event queue lock poisoned".to_string())
        .map(|mut events| {
            events.push_back(json!({
                "type":"approval.requested",
                "approvalId":approval_id,
                "operationId":operation_id,
                "requestId":request_id,
                "capability":capability,
                "target":target,
                "accountFence":account_fence,
                "reason":reason,
                "autoReview":true,
                "expiresAtMs":match expiry_policy {
                    SubagentReviewApprovalExpiryPolicy::Park => Value::Null,
                    SubagentReviewApprovalExpiryPolicy::Ttl => {
                        json!(now_ms().saturating_add(SUBAGENT_REVIEW_APPROVAL_TTL_MS))
                    }
                },
            }));
        })
    {
        let _ = broker.cancel_approval_operation(&operation_id, "approval event unavailable", now_ms());
        registry.remove(&approval_id);
        return Err(error);
    }

    let deadline = matches!(expiry_policy, SubagentReviewApprovalExpiryPolicy::Ttl)
        .then(|| now_ms().saturating_add(SUBAGENT_REVIEW_APPROVAL_TTL_MS));
    let (state, wake) = &*signal;
    let mut state = state
        .lock()
        .map_err(|_| "subagent review approval waiter lock poisoned".to_string())?;
    loop {
        if let Some(approved) = *state {
            drop(state);
            if approved {
                broker.consume_approval_for_dispatch(
                    &approval_id,
                    &operation_id,
                    &request_id,
                    capability,
                    account_fence,
                    now_ms(),
                )?;
            }
            registry.remove(&approval_id);
            return Ok(approved);
        }
        if cancelled.load(Ordering::Acquire) {
            drop(state);
            let _ = broker.cancel_approval_operation(&operation_id, "turn cancelled", now_ms());
            registry.remove(&approval_id);
            return Err("cancelled".into());
        }
        let now = now_ms();
        if deadline.is_some_and(|deadline| now >= deadline) {
            drop(state);
            let _ = broker.cancel_approval_operation(&operation_id, "approval expired", now);
            registry.remove(&approval_id);
            return Ok(false);
        }
        let wait_ms = deadline
            .map(|deadline| deadline.saturating_sub(now).min(100))
            .unwrap_or(100);
        let (next, _) = wake
            .wait_timeout(state, Duration::from_millis(wait_ms))
            .map_err(|_| "subagent review approval waiter lock poisoned".to_string())?;
        state = next;
    }
}

#[cfg(feature = "ci-account-session-import")]
mod ci_account_session {
    use super::*;
    use std::fs;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) struct CiAccountSessionIdentity {
        pub(super) access_token: String,
        pub(super) session_id: String,
        pub(super) device_id: String,
        pub(super) expires_at_epoch_seconds: u64,
    }

    const CI_SESSION_MAX_BYTES: u64 = 64 * 1024;
    const CI_SESSION_MAX_LIFETIME_SECONDS: u64 = 5 * 60 * 60;

    pub(super) fn parse_ci_account_session_document(
        document: &Value,
        now_epoch_seconds: u64,
    ) -> Option<CiAccountSessionIdentity> {
        let object = document.as_object()?;
        let access_token = object.get("accessToken")?.as_str()?;
        let device_id = object.get("deviceId")?.as_str()?;
        let session_id = object.get("sessionId")?.as_str()?;
        let token_type = object
            .get("tokenType")
            .and_then(Value::as_str)
            .unwrap_or("Bearer");
        let provider = object.get("provider")?.as_str()?;
        let ci_runner = object.get("ciRunner")?.as_bool()?;
        let expiry = object.get("accessTokenExpiresAt")?.as_u64()?;

        if access_token.len() < 24
            || access_token.len() > 16 * 1024
            || access_token.chars().any(char::is_whitespace)
            || token_type != "Bearer"
            || provider != "github-actions"
            || !ci_runner
            || object.contains_key("refreshToken")
            || expiry <= now_epoch_seconds.saturating_add(30)
            || expiry > now_epoch_seconds.saturating_add(CI_SESSION_MAX_LIFETIME_SECONDS)
        {
            return None;
        }

        let device_inner = device_id
            .strip_prefix("gha-")?
            .strip_suffix("-interactive")?;
        let (device_run, device_attempt) = device_inner.split_once('-')?;
        if device_run.is_empty()
            || device_attempt.is_empty()
            || !device_run.chars().all(|value| value.is_ascii_digit())
            || !device_attempt.chars().all(|value| value.is_ascii_digit())
        {
            return None;
        }

        let session_inner = session_id.strip_prefix("ci-runner:")?;
        let (session_run, session_attempt) = session_inner.split_once(':')?;
        if session_run != device_run
            || session_attempt != device_attempt
            || session_run.is_empty()
            || session_attempt.is_empty()
            || !session_run.chars().all(|value| value.is_ascii_digit())
            || !session_attempt.chars().all(|value| value.is_ascii_digit())
        {
            return None;
        }

        Some(CiAccountSessionIdentity {
            access_token: access_token.to_string(),
            session_id: session_id.to_string(),
            device_id: device_id.to_string(),
            expires_at_epoch_seconds: expiry,
        })
    }

    pub(super) fn from_environment(
        now_epoch_seconds: u64,
    ) -> Option<(PathBuf, CiAccountSessionIdentity)> {
        if std::env::var("GITHUB_ACTIONS").ok().as_deref() != Some("true") {
            return None;
        }
        let path = PathBuf::from(std::env::var("FABUSHI_CI_ACCOUNT_SESSION_FILE").ok()?);
        let metadata = fs::metadata(&path).ok()?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > CI_SESSION_MAX_BYTES {
            return None;
        }
        let raw = fs::read_to_string(&path).ok()?;
        let document: Value = serde_json::from_str(&raw).ok()?;
        let identity = parse_ci_account_session_document(&document, now_epoch_seconds)?;
        Some((path, identity))
    }


}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AndroidHostMode {
    Production,
    Test,
}

#[derive(Clone)]
struct RuntimeCallCancellationEntry {
    plugin_id: String,
    required_permissions: BTreeSet<String>,
    token: Arc<AtomicBool>,
}

#[derive(Default)]
struct RuntimeCallCancellationState {
    pending: BTreeMap<String, RuntimeCallCancellationEntry>,
    blocked_plugins: BTreeSet<String>,
    blocked_permissions: BTreeSet<(String, String)>,
}

#[derive(Default)]
pub struct RuntimeCallCancellationRegistry {
    state: Mutex<RuntimeCallCancellationState>,
}

impl RuntimeCallCancellationRegistry {
    pub fn register(
        &self,
        request_id: &str,
        plugin_id: &str,
        required_permissions: BTreeSet<String>,
    ) -> Result<Arc<AtomicBool>, String> {
        let mut state = self.state.lock().map_err(|_| "runtime call cancellation registry lock poisoned")?;
        if state.blocked_plugins.contains(plugin_id)
            || required_permissions.iter().any(|permission| {
                state.blocked_permissions.contains(&(plugin_id.to_string(), permission.clone()))
            })
        {
            return Err("runtime.call is fenced by a pending stop or permission revocation".into());
        }
        if state.pending.contains_key(request_id) {
            return Err("duplicate runtime.call request identity".into());
        }
        let token = Arc::new(AtomicBool::new(false));
        state.pending.insert(
            request_id.to_string(),
            RuntimeCallCancellationEntry {
                plugin_id: plugin_id.to_string(),
                required_permissions,
                token: token.clone(),
            },
        );
        Ok(token)
    }

    pub fn complete(&self, request_id: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.pending.remove(request_id);
        }
    }

    pub fn signal_request(&self, request_id: &str) -> bool {
        let Ok(state) = self.state.lock() else { return false; };
        let Some(entry) = state.pending.get(request_id) else { return false; };
        entry.token.store(true, Ordering::Release);
        true
    }

    pub fn signal_plugin(&self, plugin_id: &str) -> usize {
        let Ok(mut state) = self.state.lock() else { return 0; };
        state.blocked_plugins.insert(plugin_id.to_string());
        let mut signalled = 0;
        for entry in state.pending.values() {
            if entry.plugin_id == plugin_id {
                entry.token.store(true, Ordering::Release);
                signalled += 1;
            }
        }
        signalled
    }

    pub fn signal_permission(&self, plugin_id: &str, permission: &str) -> usize {
        let Ok(mut state) = self.state.lock() else { return 0; };
        state.blocked_permissions.insert((plugin_id.to_string(), permission.to_string()));
        let mut signalled = 0;
        for entry in state.pending.values() {
            if entry.plugin_id == plugin_id && entry.required_permissions.contains(permission) {
                entry.token.store(true, Ordering::Release);
                signalled += 1;
            }
        }
        signalled
    }

    pub fn signal_all(&self) -> usize {
        let Ok(state) = self.state.lock() else { return 0; };
        for entry in state.pending.values() {
            entry.token.store(true, Ordering::Release);
        }
        state.pending.len()
    }

    pub fn is_blocked(&self, plugin_id: &str, required_permissions: &BTreeSet<String>) -> bool {
        let Ok(state) = self.state.lock() else { return true; };
        state.blocked_plugins.contains(plugin_id)
            || required_permissions.iter().any(|permission| {
                state.blocked_permissions.contains(&(plugin_id.to_string(), permission.clone()))
            })
    }

    #[cfg(test)]
    fn has_request(&self, request_id: &str) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| state.pending.contains_key(request_id))
    }

    pub fn clear_plugin_block(&self, plugin_id: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.blocked_plugins.remove(plugin_id);
        }
    }

    pub fn clear_permission_block(&self, plugin_id: &str, permission: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.blocked_permissions.remove(&(plugin_id.to_string(), permission.to_string()));
        }
    }
}

pub struct AndroidJsonHost {
    mode: AndroidHostMode,
    account: AndroidAccountService,
    agents: Arc<Mutex<AndroidAgentRoster>>,
    sidebar_sections: Arc<Mutex<AndroidSidebarSections>>,
    transcript: Arc<Mutex<TranscriptStore>>,
    messaging: Arc<Mutex<AndroidMessagingService>>,
    mcp_auth_watches: Arc<Mutex<AndroidMcpAuthWatchManager>>,
    mcp_auth_owner: Option<AndroidMcpAuthWatchOwner>,
    mcp_dashboard_backend: Option<Arc<CursorDashboardMcpAuthBackend>>,
    plugin_variables: PluginVariableStore,
    pending_plugin_variable_writes: BTreeMap<String, PreparedPluginVariableWrite>,
    #[cfg(feature = "ci-account-session-import")]
    ci_session_path: Option<PathBuf>,
    #[cfg(feature = "ci-account-session-import")]
    ci_session_identity: Option<ci_account_session::CiAccountSessionIdentity>,
    logged_in: bool,
    next_attempt: u64,
    next_operation: u64,
    oauth_attempts: BTreeSet<String>,
    browser_attempts: BTreeMap<String, String>,
    events: VecDeque<Value>,
    active_operations: BTreeSet<String>,
    pending_approvals: BTreeMap<String, String>,
    remote_approvals: RemoteApprovalRegistry,
    turn_events: Arc<Mutex<VecDeque<Value>>>,
    turn_cancellations: BTreeMap<String, Arc<AtomicBool>>,
    turn_journal: Arc<Mutex<DurableTurnJournal>>,
    turn_lifecycle: Arc<Mutex<ProductionTurnLifecycleStore>>,
    turn_upgrade_quiescing: Arc<AtomicBool>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    agent_turn_interruptions: Arc<AgentTurnInterruptionRegistry>,
    agent_wake_operations: BTreeMap<String, String>,
    multitask_todos: Arc<Mutex<DurableMultitaskTodoStore>>,
    subagent_owner: Arc<Mutex<DurableSubagentOwner>>,
    subagent_tools: SubagentToolBridge,
    subagent_events: Arc<Mutex<VecDeque<Value>>>,
    subagent_review_approvals: Arc<SubagentReviewApprovalRegistry>,
    installed_plugins: BTreeSet<String>,
    plugin_installer: PluginInstaller,
    plugin_permissions: PermissionManager,
    js_runtime: Option<DeepSeekJsHost>,
    runtime_tools: BTreeMap<String, BTreeSet<String>>,
    runtime_generations: BTreeMap<String, u64>,
    runtime_call_cancellations: Arc<RuntimeCallCancellationRegistry>,
    capability_broker: SharedCapabilityBroker,
    remote_binding: Arc<Mutex<Option<RemoteDispatchBinding>>>,
    remote_runner: Arc<Mutex<Option<AuthenticatedRemoteHostRunner<AuthenticatedRemoteHttpTransport>>>>,
    remote_journal_path: PathBuf,
    automation_runtime: AutomationRuntime,
    webauthn: WebAuthnProxyExtension,
    webauthn_provider_queues: BTreeMap<String, VecDeque<WebAuthnRequestFrame>>,
}

impl AndroidJsonHost {
    pub fn new(app_data_dir: impl Into<PathBuf>, mode: AndroidHostMode) -> Self {
        Self::new_with_account_session(app_data_dir, mode, None)
    }

    pub fn new_with_account_session(
        app_data_dir: impl Into<PathBuf>,
        mode: AndroidHostMode,
        initial_account_session_json: Option<&str>,
    ) -> Self {
        let app_data_dir = app_data_dir.into();
        let _legacy_mcp_auth_cleanup = cleanup_legacy_mcp_auth_credentials(&app_data_dir);
        let device_id = get_or_create_host_machine_id(&app_data_dir.join("machine-id"))
            .unwrap_or_else(|error| panic!("failed to open canonical Android machine id: {error}"));
        let account = AndroidAccountService::with_persistent_browser_attempts(
            device_id.clone(),
            (mode == AndroidHostMode::Production)
                .then_some(initial_account_session_json)
                .flatten(),
            app_data_dir.join("account-oauth-attempt.json"),
        )
        .unwrap_or_else(|error| panic!("failed to initialize Android account service: {error}"));
        let agents = Arc::new(Mutex::new(
            AndroidAgentRoster::open(app_data_dir.join("agents.json"))
                .unwrap_or_else(|error| panic!("failed to open canonical Android agent roster: {error}")),
        ));
        let sidebar_sections = Arc::new(Mutex::new(
            AndroidSidebarSections::open(app_data_dir.join("sidebar-sections.json"))
                .unwrap_or_else(|error| panic!("failed to open canonical Android sidebar sections: {error}")),
        ));
        let transcript = Arc::new(Mutex::new(
            TranscriptStore::open(app_data_dir.join("transcript.json"))
                .unwrap_or_else(|error| panic!("failed to open canonical Android transcript: {error}")),
        ));
        let messaging = Arc::new(Mutex::new(
            AndroidMessagingService::open(&app_data_dir)
                .unwrap_or_else(|error| panic!("failed to open canonical Android messaging repository: {error}")),
        ));
        let mcp_auth_watches = Arc::new(Mutex::new(
            AndroidMcpAuthWatchManager::open(
                app_data_dir.join("mcp-auth-watches.json"),
                now_ms(),
            )
            .unwrap_or_else(|error| panic!("failed to open durable Android MCP auth watch manager: {error}")),
        ));
        let (mcp_auth_owner, mcp_dashboard_backend) = if mode == AndroidHostMode::Production {
            let dashboard = Arc::new(
                CursorDashboardMcpAuthBackend::from_process_environment(device_id.clone())
                    .unwrap_or_else(|error| panic!("failed to initialize MCP backend owner: {error}")),
            );
            let backend: Arc<dyn McpAuthBackendPort> = dashboard.clone();
            let policy: Arc<dyn McpAuthAdminPolicyPort> = dashboard.clone();
            let owner = AndroidMcpAuthWatchOwner::start(
                Arc::clone(&mcp_auth_watches),
                backend,
                policy,
            )
            .unwrap_or_else(|error| panic!("failed to start MCP auth watch owner: {error}"));
            (Some(owner), Some(dashboard))
        } else {
            (None, None)
        };
        let plugin_variables = PluginVariableStore::open(app_data_dir.join("plugin-variables.json"))
            .unwrap_or_else(|error| panic!("failed to open account-scoped plugin variable store: {error}"));
        let plugin_installer = PluginInstaller::new(app_data_dir.join("plugins"))
            .unwrap_or_else(|error| panic!("failed to open canonical Android plugin installer: {error}"));
        let plugin_permissions = PermissionManager::load(app_data_dir.join("plugin-permissions.json"))
            .unwrap_or_else(|error| panic!("failed to open canonical Android plugin permission store: {error}"));
        #[cfg(feature = "ci-account-session-import")]
        let ci_session = if mode == AndroidHostMode::Production {
            ci_account_session::from_environment(now_ms() / 1_000)
        } else {
            None
        };
        #[cfg(feature = "ci-account-session-import")]
        let logged_in = ci_session.is_some();
        #[cfg(feature = "ci-account-session-import")]
        let (ci_session_path, ci_session_identity) = ci_session
            .map(|(path, identity)| (Some(path), Some(identity)))
            .unwrap_or((None, None));
        #[cfg(not(feature = "ci-account-session-import"))]
        let logged_in = false;
        let multitask_todos = Arc::new(Mutex::new(
            DurableMultitaskTodoStore::open(app_data_dir.join("multitask-todos.json"))
                .unwrap_or_else(|error| panic!("failed to open durable multitask todo owner: {error}")),
        ));
        let subagent_owner = Arc::new(Mutex::new(
            DurableSubagentOwner::open(app_data_dir.join("generated-subagents.json"), now_ms())
                .unwrap_or_else(|error| panic!("failed to open durable generated-subagent owner: {error}")),
        ));
        let subagent_tools = SubagentToolBridge::new(Arc::clone(&subagent_owner));
        let subagent_events = Arc::new(Mutex::new(VecDeque::new()));
        let subagent_review_approvals = Arc::new(SubagentReviewApprovalRegistry::default());
        Self {
            mode,
            account,
            agents,
            sidebar_sections,
            transcript,
            messaging,
            mcp_auth_watches,
            mcp_auth_owner,
            mcp_dashboard_backend,
            plugin_variables,
            pending_plugin_variable_writes: BTreeMap::new(),
            #[cfg(feature = "ci-account-session-import")]
            ci_session_path,
            #[cfg(feature = "ci-account-session-import")]
            ci_session_identity,
            logged_in,
            next_attempt: 0,
            next_operation: 0,
            oauth_attempts: BTreeSet::new(),
            browser_attempts: BTreeMap::new(),
            events: VecDeque::new(),
            active_operations: BTreeSet::new(),
            pending_approvals: BTreeMap::new(),
            remote_approvals: RemoteApprovalRegistry::default(),
            turn_events: Arc::new(Mutex::new(VecDeque::new())),
            turn_cancellations: BTreeMap::new(),
            turn_journal: Arc::new(Mutex::new(
                DurableTurnJournal::open(app_data_dir.join("agent-turn-journal.json"), now_ms())
                    .unwrap_or_else(|error| panic!("failed to open durable Agent turn journal: {error}")),
            )),
            turn_lifecycle: Arc::new(Mutex::new(
                ProductionTurnLifecycleStore::open(
                    app_data_dir.join("agent-turn-lifecycle.json"),
                    now_ms(),
                )
                .unwrap_or_else(|error| {
                    panic!("failed to open durable Agent lifecycle store: {error}")
                }),
            )),
            turn_upgrade_quiescing: Arc::new(AtomicBool::new(false)),
            live_account_fence: Arc::new(Mutex::new(None)),
            agent_turn_interruptions: Arc::new(AgentTurnInterruptionRegistry::default()),
            agent_wake_operations: BTreeMap::new(),
            multitask_todos,
            subagent_owner,
            subagent_tools,
            subagent_events,
            subagent_review_approvals,
            installed_plugins: BTreeSet::new(),
            plugin_installer,
            plugin_permissions,
            js_runtime: None,
            runtime_tools: BTreeMap::new(),
            runtime_generations: BTreeMap::new(),
            runtime_call_cancellations: Arc::new(RuntimeCallCancellationRegistry::default()),
            capability_broker: SharedCapabilityBroker::open(app_data_dir.join("capability-broker.json"), now_ms())
                .unwrap_or_else(|error| panic!("failed to open durable Android Capability Broker: {error}")),
            remote_binding: Arc::new(Mutex::new(None)),
            remote_runner: Arc::new(Mutex::new(None)),
            remote_journal_path: app_data_dir.join("remote-execution-journal.json"),
            automation_runtime: AutomationRuntime::open(app_data_dir.join("automation-runtime.json"), now_ms())
                .unwrap_or_else(|error| panic!("failed to open durable Android automation runtime: {error}")),
            webauthn: WebAuthnProxyExtension::new(WebAuthnProxyExtensionConfig::default()),
            webauthn_provider_queues: BTreeMap::new(),
        }
    }

    pub fn set_remote_binding_json(&mut self, raw: Option<&str>) -> Result<(), String> {
        let raw = raw.map(str::trim).filter(|value| !value.is_empty());
        if raw.is_none() {
            *self.remote_binding.lock().map_err(|_| "remote binding lock poisoned".to_string())? = None;
            *self.remote_runner.lock().map_err(|_| "remote runner lock poisoned".to_string())? = None;
            return Ok(());
        }
        let binding = RemoteDispatchBinding::parse(raw.unwrap())?;
        let current_fence = self.current_turn_account_fence()?;
        if current_fence != binding.account_fence {
            return Err("protected remote binding is fenced to a different account session".into());
        }
        let runner = AuthenticatedRemoteHostRunner::production(
            &binding.endpoint,
            &self.remote_journal_path,
            RemoteTransportPolicy::default(),
            now_ms(),
        )
        .map_err(|error| format!("failed to initialize authenticated Remote Runner: {error:?}"))?;
        *self.remote_binding.lock().map_err(|_| "remote binding lock poisoned".to_string())? = Some(binding);
        *self.remote_runner.lock().map_err(|_| "remote runner lock poisoned".to_string())? = Some(runner);
        Ok(())
    }

    fn remote_binding_status(&self) -> Value {
        let current_fence = self.current_turn_account_fence().ok();
        let binding = self.remote_binding.lock().ok().and_then(|binding| binding.clone());
        let runner_ready = self.remote_runner.lock().map(|runner| runner.is_some()).unwrap_or(false);
        let ready = binding.as_ref().is_some_and(|binding| {
            runner_ready && current_fence.as_deref() == Some(binding.account_fence.as_str())
        });
        match binding {
            Some(binding) if ready => json!({
                "ready": true,
                // Outbound Shell/Read are now shipping, but no authenticated Computer adapter
                // is installed yet. Do not advertise desktop-control capability from pairing
                // metadata alone.
                "hasDesktop": false,
                "deviceId": binding.device_id,
                "accountEpoch": binding.account_epoch,
            }),
            _ => json!({
                "ready": false,
                "hasDesktop": false,
                "deviceId": Value::Null,
                "accountEpoch": Value::Null,
            }),
        }
    }

    pub fn runtime_call_control(&self) -> Arc<RuntimeCallCancellationRegistry> {
        Arc::clone(&self.runtime_call_cancellations)
    }

    pub fn dispatch(&mut self, method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "host.platform" => Ok(json!({"platform":"android"})),
            "feature.info" => Ok(json!({
                "platform": "android",
                "protocolVersion": "fabushi.feature.v1",
                "runtimeVersion": match self.mode {
                    AndroidHostMode::Production => "android-production",
                    AndroidHostMode::Test => "android-test",
                }
            })),
            "feature.auth.status" => self.account_status(),
            "feature.account.fence" => Ok(json!({"accountFence":self.current_turn_account_fence()?})),
            "feature.account.sandAccess" => self.account_sand_access(),
            "feature.account.privacyMode" => Ok(self.account_privacy_mode()),
            "feature.account.teamRules" => self.account_team_rules(),
            "feature.remote.binding.status" => Ok(self.remote_binding_status()),
            "feature.auth.deviceAgentSession" => Ok(self.device_agent_session()),
            "feature.auth.providers" => Ok(json!([
                {"id":"google","displayName":"Google"},
                {"id":"github","displayName":"GitHub"}
            ])),
            "feature.auth.browserStart" => self.auth_browser_start(),
            "feature.auth.browserReopen" => self.auth_browser_reopen(params),
            "feature.auth.browserCancel" => self.auth_browser_cancel(params),
            "feature.auth.browserPoll" => self.auth_browser_poll(params),
            "feature.auth.oauthStart" => self.oauth_start(params),
            "feature.auth.oauthPoll" => self.oauth_poll(params),
            "feature.auth.oauthCancel" => self.oauth_cancel(params),
            "feature.mcp.oauthComplete" => self.mcp_oauth_complete(params),
            "feature.mcp.authenticate" => self.mcp_authenticate(params),
            "feature.mcp.authWatch.register" => self.mcp_auth_watch_register(params),
            "feature.mcp.authWatch.poll" => self.mcp_auth_watch_poll(params),
            "feature.mcp.authWatch.settle" => self.mcp_auth_watch_settle(params),
            "feature.mcp.authWatch.cancel" => self.mcp_auth_watch_cancel(params),
            "feature.mcp.authWatch.snapshot" => self.mcp_auth_watch_snapshot(),
            "feature.auth.logout" => self.account_logout(),
            "feature.agent.diskPressure.observe" => self.agent_disk_pressure_observe(params),
            "feature.agent.diskPressure.record" => self.agent_disk_pressure_record(params),
            "feature.agent.turn.reconcile" => self.agent_turn_reconcile(params),
            "feature.agent.subagent.tool" => self.agent_subagent_tool(params),
            "feature.agent.subagent.reconcile" => self.agent_subagent_reconcile(params),
            "feature.agent.rosterMutation" => self.agent_roster_mutation(params),
            "feature.agent.sidebarSections" => self.agent_sidebar_sections(),
            "feature.agent.upgradeQuiesce" => self.agent_upgrade_quiesce(params),
            "feature.automation.upsert" => self.automation_upsert(params),
            "feature.automation.list" => self.automation_list(),
            "feature.automation.start" => self.automation_start(params),
            "feature.automation.advanceStep" => self.automation_advance_step(params),
            "feature.automation.awaitApproval" => self.automation_await_approval(params),
            "feature.automation.resolveApproval" => self.automation_resolve_approval(params),
            "feature.automation.cancel" => self.automation_cancel(params),
            "feature.automation.settle" => self.automation_settle(params),
            "feature.automation.snapshot" => self.automation_snapshot(params),
            "listAgents" => Ok(Value::Array(self.project_agent_roster()?)),
            "countAgents" => {
                let agents = self.agents.lock().map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?;
                Ok(json!(agents.count()))
            }
            "createAgent" => {
                let name = required_string(params, "name")?;
                let description = params.get("description").and_then(Value::as_str).unwrap_or("");
                let agent = self.agents
                    .lock()
                    .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
                    .create(name, description)
                    .map_err(|error| error.to_string())?;
                Ok(json!({"agent": agent.as_json()}))
            }
            "createGroup" => {
                let name = required_string(params, "name")?;
                let description = params.get("description").and_then(Value::as_str).unwrap_or("");
                let member_ids = params
                    .get("memberAgentIds")
                    .and_then(Value::as_array)
                    .ok_or("memberAgentIds array is required")?
                    .iter()
                    .map(|value| value.as_str().ok_or("memberAgentIds must contain strings").map(str::to_string))
                    .collect::<Result<Vec<_>, _>>()?;
                let group = self.agents
                    .lock()
                    .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
                    .create_group(name, description, &member_ids)
                    .map_err(|error| error.to_string())?;
                Ok(group.as_json())
            }
            "setGroupMembers" => {
                let id = required_string(params, "id")?;
                let member_ids = params
                    .get("memberAgentIds")
                    .and_then(Value::as_array)
                    .ok_or("memberAgentIds array is required")?
                    .iter()
                    .map(|value| value.as_str().ok_or("memberAgentIds must contain strings").map(str::to_string))
                    .collect::<Result<Vec<_>, _>>()?;
                let group = self.agents
                    .lock()
                    .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
                    .set_group_members(id, &member_ids)
                    .map_err(|error| error.to_string())?;
                Ok(group.as_json())
            }
            "updateAgent" => {
                let id = required_string(params, "id")?;
                let mut agents = self.agents.lock().map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?;
                let current = agents.get(id).ok_or_else(|| "agent not found".to_string())?;
                let profile = params.get("profile").and_then(Value::as_object).ok_or("profile is required")?;
                let name = profile.get("name").and_then(Value::as_str).unwrap_or(&current.name).to_string();
                let description = profile.get("description").and_then(Value::as_str).unwrap_or(&current.description).to_string();
                let agent = agents.update_profile(id, &name, &description).map_err(|error| error.to_string())?;
                Ok(agent.as_json())
            }
            "setAgentHiddenFromSidebar" => {
                let id = required_string(params, "id")?;
                let is_hidden = params.get("isHidden").and_then(Value::as_bool).ok_or("isHidden is required")?;
                let agent = self.agents
                    .lock()
                    .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
                    .set_hidden(id, is_hidden)
                    .map_err(|error| error.to_string())?;
                Ok(agent.as_json())
            }
            "setAgentUnread" => {
                let id = required_string(params, "id")?;
                let is_unread = params.get("isUnread").and_then(Value::as_bool).ok_or("isUnread is required")?;
                let agent = self.agents
                    .lock()
                    .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
                    .set_unread(id, is_unread)
                    .map_err(|error| error.to_string())?;
                Ok(agent.as_json())
            }
            "duplicateAgent" => {
                let id = required_string(params, "id")?;
                let agent = self.agents
                    .lock()
                    .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
                    .duplicate(id)
                    .map_err(|error| error.to_string())?;
                Ok(json!({"agent": agent.as_json()}))
            }
            "deleteAgents" => {
                let ids = params.get("ids").and_then(Value::as_array).ok_or("ids array is required")?
                    .iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>();
                let deleted = self.agents
                    .lock()
                    .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
                    .delete(&ids)
                    .map_err(|error| error.to_string())?;
                Ok(json!({"deletedIds": deleted}))
            }
            "getPinnedAgents" => {
                let agents = self.agents.lock().map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?;
                Ok(json!(agents.pinned_agent_ids()))
            }
            "setPinnedAgents" => {
                let ids = params.get("ids").and_then(Value::as_array).ok_or("ids array is required")?
                    .iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>();
                let ids = self.agents
                    .lock()
                    .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
                    .set_pinned_agents(&ids)
                    .map_err(|error| error.to_string())?;
                Ok(json!(ids))
            }
            "feature.execute" => self.feature_execute(params),
            "feature.receive" => self.feature_receive(),
            "feature.interrupt" => self.feature_interrupt(params),
            "feature.approval.resolve" => self.feature_approval_resolve(params),
            "feature.marketplace.browse" => self.marketplace_browse(params),
            "feature.marketplace.release" => self.marketplace_release(params),
            "feature.plugin.install" => self.plugin_install(params),
            "feature.plugin.variables.fields" => self.plugin_variable_fields(params),
            "feature.plugin.variables.prepare" => self.plugin_variable_prepare(params),
            "feature.plugin.variables.commit" => self.plugin_variable_commit(params),
            "feature.plugin.variables.runtimeConfig" => self.plugin_variable_runtime_config(params),
            "feature.plugin.uiDocument" => self.plugin_ui_document(params),
            "plugin.compatibility" => self.plugin_compatibility(params),
            "plugin.permission.grant" => self.plugin_permission_grant(params),
            "plugin.permission.revoke" => self.plugin_permission_revoke(params),
            "runtime.start" => self.runtime_start(params),
            "runtime.stop" => self.runtime_stop(params),
            "runtime.tools" => self.runtime_tools(params),
            "runtime.call" => self.runtime_call(params),
            "runtime.cancel" => self.runtime_cancel(params),
            "feature.messaging.access.issue" => self.messaging_access_issue(params),
            "feature.messaging.blob.read" => self.messaging_blob_read(params),
            "feature.messaging.execute" => self.messaging_execute(params),
            "feature.transcript.snapshot" => Ok(Value::Array(
                self.transcript
                    .lock()
                    .map_err(|_| "transcript lock poisoned".to_string())?
                    .get_transcript(),
            )),
            "feature.assistant.projection" => self.assistant_projection(),
            "feature.assistant.markRead" => self.assistant_mark_read(),
            "feature.webauthn.registerProvider" => self.webauthn_register_provider(),
            "feature.webauthn.unregisterProvider" => self.webauthn_unregister_provider(params),
            "feature.webauthn.pollRequest" => self.webauthn_poll_request(params),
            "feature.webauthn.submitResponses" => self.webauthn_submit_responses(params),
            "feature.webauthn.requestCeremony" => self.webauthn_request_ceremony(params),
            "platform.request" => self.platform_request(params),
            other => Err(format!("unknown host method {other}")),
        }
    }


    fn project_agent_roster(&self) -> Result<Vec<Value>, String> {
        let agents = self
            .agents
            .lock()
            .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
            .list();
        let transcript = self
            .transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?
            .get_transcript();

        let mut latest_message: BTreeMap<String, (u64, String)> = BTreeMap::new();
        let mut running_agents = BTreeSet::new();
        for entry in transcript {
            if entry.get("kind").and_then(Value::as_str) != Some("message") {
                continue;
            }
            let Some(agent_id) = entry
                .get("agentId")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            else {
                continue;
            };
            let timestamp = entry
                .get("timestampMs")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if let Some(content) = entry.get("content").and_then(Value::as_str) {
                let replace = latest_message
                    .get(agent_id)
                    .is_none_or(|(observed_at, _)| timestamp >= *observed_at);
                if replace {
                    latest_message.insert(agent_id.to_string(), (timestamp, content.to_string()));
                }
            }
            if entry
                .get("operationId")
                .and_then(Value::as_str)
                .is_some_and(|operation_id| self.active_operations.contains(operation_id))
            {
                running_agents.insert(agent_id.to_string());
            }
        }

        let account_fence = self.current_turn_account_fence().ok();
        let known_agent_ids = agents
            .iter()
            .map(|agent| agent.id.clone())
            .collect::<BTreeSet<_>>();
        let messaging = self
            .messaging
            .lock()
            .map_err(|_| "canonical Android messaging owner lock poisoned".to_string())?;

        Ok(agents
            .into_iter()
            .map(|agent| {
                let mut value = agent.as_json();
                if let Some(object) = value.as_object_mut() {
                    if let Some((timestamp, message)) = latest_message.get(&agent.id) {
                        object.insert("lastMessage".into(), Value::String(message.clone()));
                        object.insert(
                            "updatedAt".into(),
                            Value::from(agent.updated_at.max(*timestamp)),
                        );
                    } else {
                        object.insert("lastMessage".into(), Value::String(String::new()));
                    }
                    object.insert(
                        "isRunning".into(),
                        Value::Bool(running_agents.contains(&agent.id)),
                    );
                    let partners = account_fence
                        .as_deref()
                        .map(|fence| messaging.agent_conversation_partner_ids(fence, &agent.id))
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|partner_id| known_agent_ids.contains(partner_id))
                        .map(Value::String)
                        .collect();
                    object.insert("conversationPartnerIds".into(), Value::Array(partners));
                    // No second owner: preserve Desktop's nullable waiting-state contract until
                    // Host has an explicit durable waiting-user event/state owner.
                    object.insert("awaitingUserResponse".into(), Value::Null);
                }
                value
            })
            .collect())
    }

    fn assistant_projection(&self) -> Result<Value, String> {
        let transcript = self
            .transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?;
        Ok(assistant_projection_from_entries(&transcript.get_transcript()))
    }

    fn assistant_mark_read(&mut self) -> Result<Value, String> {
        let mut transcript = self
            .transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?;
        let entries = transcript.get_transcript();
        if let Some(latest_message_id) = latest_visible_assistant_message_id(&entries) {
            let marker = json!({
                "id": ASSISTANT_READ_MARKER_ID,
                "kind": "assistant-read-marker",
                "lastReadMessageId": latest_message_id,
                "timestampMs": now_ms(),
            });
            if transcript.contains_id(ASSISTANT_READ_MARKER_ID) {
                transcript
                    .update_entry(ASSISTANT_READ_MARKER_ID, |_| marker.clone())
                    .map_err(|error| format!("failed to persist assistant read marker: {error}"))?;
            } else {
                transcript
                    .append_entry(marker)
                    .map_err(|error| format!("failed to persist assistant read marker: {error}"))?;
            }
        }
        Ok(assistant_projection_from_entries(&transcript.get_transcript()))
    }

    fn messaging_access_issue(&mut self, params: &Value) -> Result<Value, String> {
        let requested_session = required_string(params, "sessionId")?.to_string();
        let requested_device = required_string(params, "deviceId")?.to_string();
        let scopes = params
            .get("scopes")
            .and_then(Value::as_array)
            .ok_or("messaging scopes array is required")?;
        if !scopes.iter().any(|scope| scope.as_str() == Some("messaging")) {
            return Err("messaging scope is required".into());
        }
        let (actor_id, mutation) = self.current_messaging_identity()?;
        Ok(with_account_session_mutation(
            json!({
                "status":"available",
                "actorId":actor_id,
                "sessionId":requested_session,
                "deviceId":requested_device,
                "protocolVersion":2
            }),
            mutation,
        ))
    }

    fn messaging_execute(&mut self, params: &Value) -> Result<Value, String> {
        let (actor_id, mutation) = self.current_messaging_identity()?;
        let result = self
            .messaging
            .lock()
            .map_err(|_| "canonical Android messaging owner lock poisoned".to_string())?
            .execute(params, &actor_id, i64::try_from(now_ms()).unwrap_or(i64::MAX))?;
        Ok(with_account_session_mutation(result, mutation))
    }

    fn messaging_blob_read(&mut self, params: &Value) -> Result<Value, String> {
        let (actor_id, mutation) = self.current_messaging_identity()?;
        let result = self
            .messaging
            .lock()
            .map_err(|_| "canonical Android messaging owner lock poisoned".to_string())?
            .read_blob_range(params, &actor_id)?;
        Ok(with_account_session_mutation(result, mutation))
    }

    fn current_turn_account_fence(&self) -> Result<String, String> {
        let resolved = if self.mode == AndroidHostMode::Test {
            Ok("session:test:android".to_string())
        } else {
            #[cfg(feature = "ci-account-session-import")]
            if let Some(identity) = self.ci_session_identity.as_ref() {
                if !self.logged_in {
                    Err("Sign in to Fabushi to use account-scoped capabilities.".into())
                } else {
                    let material = format!("{}\n{}", identity.session_id, identity.device_id);
                    Ok(format!("session:{}", crate::sha256::sha256_hex(material.as_bytes())))
                }
            } else {
                self.account.session_fence()
                    .ok_or_else(|| "Sign in to Fabushi to use account-scoped capabilities.".into())
            }
            #[cfg(not(feature = "ci-account-session-import"))]
            {
                self.account.session_fence()
                    .ok_or_else(|| "Sign in to Fabushi to use account-scoped capabilities.".into())
            }
        };
        if let Ok(fence) = resolved.as_ref() {
            *self
                .live_account_fence
                .lock()
                .map_err(|_| "live account fence lock poisoned".to_string())? = Some(fence.clone());
        } else if let Ok(mut live) = self.live_account_fence.lock() {
            *live = None;
        }
        resolved
    }

    fn current_messaging_identity(
        &mut self,
    ) -> Result<(String, Option<AccountSessionMutation>), String> {
        if self.mode == AndroidHostMode::Test {
            if !self.logged_in {
                return Err("Sign in to Fabushi to use messaging.".into());
            }
            return Ok(("human:android-test".into(), None));
        }
        #[cfg(feature = "ci-account-session-import")]
        if let Some(identity) = self.ci_session_identity.as_ref() {
            if !self.logged_in {
                return Err("Sign in to Fabushi to use messaging.".into());
            }
            return Ok((
                format!("human:ci:{}", identity.session_id.replace(':', "-")),
                None,
            ));
        }
        let (status, mutation) = self.account.public_status()?;
        if status.get("loggedIn").and_then(Value::as_bool) != Some(true) {
            return Err("Sign in to Fabushi to use messaging.".into());
        }
        let raw_id = status
            .get("user")
            .and_then(|user| user.get("id"))
            .ok_or("Fabushi account identity is missing a user id.")?;
        let raw_id = match raw_id {
            Value::String(value) => value.clone(),
            Value::Number(value) => value.to_string(),
            _ => return Err("Fabushi account user id has an unsupported type.".into()),
        };
        let actor_id = if raw_id.starts_with("human:") {
            raw_id
        } else {
            format!("human:{raw_id}")
        };
        Ok((actor_id, mutation))
    }

    fn webauthn_register_provider(&mut self) -> Result<Value, String> {
        let now = now_ms();
        let (provider_id, welcome) = self.webauthn.register_provider(now);
        self.webauthn_provider_queues
            .entry(provider_id.clone())
            .or_default()
            .push_back(welcome);
        Ok(json!({"providerId": provider_id}))
    }

    fn webauthn_unregister_provider(&mut self, params: &Value) -> Result<Value, String> {
        let provider_id = required_string(params, "providerId")?.to_string();
        self.webauthn.bridge_mut().unregister_provider(&provider_id);
        self.webauthn_provider_queues.remove(&provider_id);
        Ok(json!({"providerId":provider_id,"status":"unregistered"}))
    }

    fn webauthn_poll_request(&mut self, params: &Value) -> Result<Value, String> {
        let provider_id = required_string(params, "providerId")?.to_string();
        for (target_provider, _, frame) in self.webauthn.expire(now_ms()) {
            self.webauthn_provider_queues
                .entry(target_provider)
                .or_default()
                .push_back(frame);
        }
        let frame = self
            .webauthn_provider_queues
            .entry(provider_id.clone())
            .or_default()
            .pop_front();
        Ok(json!({
            "providerId": provider_id,
            "frame": frame.map(webauthn_request_frame_json).unwrap_or(Value::Null),
        }))
    }

    fn webauthn_submit_responses(&mut self, params: &Value) -> Result<Value, String> {
        let provider_id = params
            .get("providerId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let raw_frames = params
            .get("frames")
            .and_then(Value::as_array)
            .ok_or("frames array is required")?;
        let frames = raw_frames
            .iter()
            .map(parse_webauthn_response_frame)
            .collect::<Result<Vec<_>, _>>()?;
        let settlements = self
            .webauthn
            .submit_responses(now_ms(), provider_id, &frames)
            .into_iter()
            .map(|(request_id, settlement)| match settlement {
                crate::extensions::webauthn_proxy::WebAuthnBridgeSettlement::CredentialJson(value) => {
                    json!({"requestId":request_id,"kind":"result","credentialJson":value})
                }
                crate::extensions::webauthn_proxy::WebAuthnBridgeSettlement::Error { name, message, code } => {
                    json!({"requestId":request_id,"kind":"error","name":name,"message":message,"code":code})
                }
            })
            .collect::<Vec<_>>();
        Ok(json!({"accepted":true,"settlements":settlements}))
    }

    fn webauthn_request_ceremony(&mut self, params: &Value) -> Result<Value, String> {
        let kind = required_string(params, "kind")?;
        if kind != "create" && kind != "get" {
            return Err("kind must be create or get".into());
        }
        let origin = required_string(params, "origin")?;
        let payload_json = params
            .get("payloadJson")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("payloadJson is required")?;
        serde_json::from_str::<Value>(payload_json)
            .map_err(|error| format!("payloadJson must be valid JSON: {error}"))?;

        let request = self
            .webauthn
            .request_ceremony(
                now_ms(),
                WebAuthnCeremony {
                    kind: kind.to_string(),
                    origin: origin.to_string(),
                    payload_json: payload_json.to_string(),
                },
            )
            .map_err(webauthn_bridge_error_message)?;
        self.webauthn_provider_queues
            .entry(request.provider_id.clone())
            .or_default()
            .push_back(request.frame);
        Ok(json!({
            "providerId":request.provider_id,
            "requestId":request.request_id,
            "deadlineAtMs":request.deadline_at_ms,
        }))
    }

    pub fn cancel_operation(&mut self, operation_id: &str, reason: Option<&str>) -> Result<(), String> {
        if operation_id.trim().is_empty() {
            return Err("operation id is required".into());
        }
        if let Some(cancelled) = self.turn_cancellations.remove(operation_id) {
            cancelled.store(true, Ordering::Release);
        }
        self.agent_turn_interruptions.unregister_operation(operation_id);
        if let Some(wake_id) = self.agent_wake_operations.remove(operation_id) {
            self.messaging
                .lock()
                .map_err(|_| "canonical Android messaging owner lock poisoned".to_string())?
                .defer_agent_wake(
                    &wake_id,
                    i64::try_from(now_ms()).unwrap_or(i64::MAX),
                    reason.unwrap_or("cancelled"),
                )?;
        }
        self.active_operations.remove(operation_id);
        self.turn_journal
            .lock()
            .map_err(|_| "turn journal lock poisoned".to_string())?
            .settle_operation_cancelled(
                operation_id,
                reason.unwrap_or("cancelled"),
                now_ms(),
            )?;
        let _ = self
            .subagent_owner
            .lock()
            .map_err(|_| "subagent owner lock poisoned".to_string())?
            .abort_for_parent_request(
                operation_id,
                reason.unwrap_or("parent-cancelled"),
                now_ms(),
            )?;
        self.pending_approvals.retain(|_, pending_operation| pending_operation != operation_id);
        self.capability_broker.cancel_approval_operation(
            operation_id,
            reason.unwrap_or("cancelled"),
            now_ms(),
        )?;
        self.turn_events
            .lock()
            .map_err(|_| "turn event queue lock poisoned".to_string())?
            .retain(|event| {
                event.get("operationId").and_then(Value::as_str) != Some(operation_id)
            });
        self.events.push_back(json!({
            "type":"operation.interrupted",
            "operationId":operation_id,
            "reason":reason.unwrap_or("cancelled")
        }));
        Ok(())
    }

    fn account_status(&mut self) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Test {
            return Ok(self.auth_status());
        }
        #[cfg(feature = "ci-account-session-import")]
        if self.ci_session_identity.is_some() {
            return Ok(self.auth_status());
        }
        let (status, mutation) = self.account.public_status()?;
        Ok(with_account_session_mutation(status, mutation))
    }

    fn account_sand_access(&mut self) -> Result<Value, String> {
        let status = self.account_status()?;
        Ok(project_fabushi_sand_access(
            status.get("loggedIn").and_then(Value::as_bool) == Some(true),
        ))
    }

    fn account_privacy_mode(&self) -> Value {
        let mode = match self.mode {
            AndroidHostMode::Test => "no-storage",
            AndroidHostMode::Production => self
                .mcp_dashboard_backend
                .as_ref()
                .and_then(|backend| backend.resolve_sand_privacy_mode())
                .map(|mode| match mode {
                    BackendSandPrivacyMode::Unspecified => "unspecified",
                    BackendSandPrivacyMode::NoStorage => "no-storage",
                    BackendSandPrivacyMode::NoTraining => "no-training",
                    BackendSandPrivacyMode::UsageDataTrainingAllowed => "usage-data-training-allowed",
                    BackendSandPrivacyMode::UsageCodebaseTrainingAllowed => "usage-codebase-training-allowed",
                })
                .unwrap_or("unknown"),
        };
        json!({"mode":mode})
    }

    fn account_team_rules(&self) -> Result<Value, String> {
        let rules = match self.mode {
            AndroidHostMode::Test => Vec::new(),
            AndroidHostMode::Production => self
                .mcp_dashboard_backend
                .as_ref()
                .ok_or("managed team rules backend is unavailable")?
                .resolve_sand_team_rules()?,
        };
        Ok(json!({"rules": rules}))
    }

    fn automation_upsert(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let agent_id = required_string(params, "agentId")?.trim();
        if agent_id != "mahayana-assistant" {
            let agents = self
                .agents
                .lock()
                .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?;
            if agents.get(agent_id).is_none() {
                return Err("automation agent owner not found".into());
            }
        }
        let schedule = required_string(params, "schedule")?.to_string();
        let trigger_description = params
            .get("triggerDescription")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(schedule.as_str())
            .to_string();
        let spec = AutomationSpec {
            id: required_string(params, "id")?.to_string(),
            name: required_string(params, "name")?.to_string(),
            prompt: required_string(params, "prompt")?.to_string(),
            schedule,
            agent_id: agent_id.to_string(),
            trigger_description,
            enabled: params.get("enabled").and_then(Value::as_bool).unwrap_or(true),
            account_fence,
            created_at_ms: params.get("createdAtMs").and_then(Value::as_u64).unwrap_or_else(now_ms),
            last_run_at_ms: params.get("lastRunAtMs").and_then(Value::as_u64),
            next_run_at_ms: params.get("nextRunAtMs").and_then(Value::as_u64),
        };
        let id = spec.id.clone();
        self.automation_runtime.upsert_spec(spec)?;
        Ok(json!({"id":id,"stored":true}))
    }

    fn automation_list(&mut self) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        Ok(Value::Array(self.automation_runtime.list_for_account(&account_fence)))
    }

    fn automation_start(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let run = self.automation_runtime.begin_run(
            required_string(params, "automationId")?,
            required_string(params, "requestId")?,
            required_string(params, "runId")?,
            &account_fence,
            now_ms(),
        )?;
        Ok(automation_run_json(&run))
    }

    fn automation_advance_step(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let run = self.automation_runtime.advance_step(
            required_string(params, "runId")?,
            &account_fence,
            required_u64(params, "generation")?,
            now_ms(),
        )?;
        Ok(automation_run_json(&run))
    }

    fn automation_await_approval(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let run = self.automation_runtime.await_approval(
            required_string(params, "runId")?,
            &account_fence,
            required_u64(params, "generation")?,
            required_string(params, "approvalId")?,
            now_ms(),
        )?;
        Ok(automation_run_json(&run))
    }

    fn automation_resolve_approval(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let allowed = params.get("allowed").and_then(Value::as_bool).ok_or("allowed is required")?;
        let run = self.automation_runtime.resolve_approval(
            required_string(params, "runId")?,
            &account_fence,
            required_u64(params, "generation")?,
            required_string(params, "approvalId")?,
            allowed,
            now_ms(),
        )?;
        Ok(automation_run_json(&run))
    }

    fn automation_cancel(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let run = self.automation_runtime.cancel(
            required_string(params, "runId")?,
            &account_fence,
            required_u64(params, "generation")?,
            params.get("reason").and_then(Value::as_str).unwrap_or("cancelled"),
            now_ms(),
        )?;
        Ok(automation_run_json(&run))
    }

    fn automation_settle(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let succeeded = params.get("succeeded").and_then(Value::as_bool).ok_or("succeeded is required")?;
        let run = self.automation_runtime.settle(
            required_string(params, "runId")?,
            &account_fence,
            required_u64(params, "generation")?,
            succeeded,
            params.get("reason").and_then(Value::as_str).map(str::to_string),
            now_ms(),
        )?;
        Ok(automation_run_json(&run))
    }

    fn automation_snapshot(&mut self, params: &Value) -> Result<Value, String> {
        let run_id = required_string(params, "runId")?;
        let account_fence = self.current_turn_account_fence()?;
        let value = self.automation_runtime.snapshot(run_id).ok_or("automation run not found")?;
        if value.get("account_fence").and_then(Value::as_str) != Some(account_fence.as_str()) {
            return Err("automation run account fence mismatch".into());
        }
        Ok(value)
    }

    fn cancel_mcp_auth_watches_for_account_change(
        &mut self,
        reason: &str,
    ) -> Result<(), String> {
        let cancelled_watches = self
            .mcp_auth_watches
            .lock()
            .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?
            .cancel_all()?;
        for completion in cancelled_watches {
            self.events.push_back(json!({
                "type":"mcp.auth.watch.settled",
                "serverId":completion.server_id,
                "serverName":completion.server_name,
                "accountKey":completion.account_key,
                "requestingAgentId":completion.requesting_agent_id,
                "generation":completion.generation,
                "outcome":"cancelled",
                "reason":reason,
            }));
        }
        Ok(())
    }

    fn fence_turns_for_account_change(
        &mut self,
        previous_fence: &str,
        reason: &str,
    ) -> Result<(), String> {
        let now = now_ms();
        let fenced = self
            .turn_journal
            .lock()
            .map_err(|_| "turn journal lock poisoned".to_string())?
            .mark_account_outcome_unknown(previous_fence, reason, now)?;
        self.turn_lifecycle
            .lock()
            .map_err(|_| "turn lifecycle lock poisoned".to_string())?
            .mark_account_outcome_unknown(previous_fence, now)?;
        let subagent_fenced = self
            .subagent_owner
            .lock()
            .map_err(|_| "subagent owner lock poisoned".to_string())?
            .mark_account_outcome_unknown(previous_fence, reason, now)?;
        if !subagent_fenced.is_empty() {
            let mut events = self
                .subagent_events
                .lock()
                .map_err(|_| "subagent event queue lock poisoned".to_string())?;
            for subagent_id in subagent_fenced {
                events.push_back(json!({
                    "type":"subagent.outcome-unknown",
                    "subagentId":subagent_id,
                    "reason":reason,
                    "accountFence":previous_fence,
                }));
            }
        }

        for record in fenced {
            if let Some(cancelled) = self.turn_cancellations.remove(&record.operation_id) {
                cancelled.store(true, Ordering::Release);
            }
            self.agent_turn_interruptions.unregister_operation(&record.operation_id);
            if let Some(wake_id) = self.agent_wake_operations.remove(&record.operation_id) {
                let _ = self
                    .messaging
                    .lock()
                    .map_err(|_| "canonical Android messaging owner lock poisoned".to_string())?
                    .defer_agent_wake(
                        &wake_id,
                        i64::try_from(now).unwrap_or(i64::MAX),
                        "account-switch",
                    );
            }
            self.active_operations.remove(&record.operation_id);
            push_turn_event(
                &self.turn_events,
                json!({
                    "type":"operation.outcome-unknown",
                    "operationId":record.operation_id,
                    "requestId":record.request_id,
                    "reason":reason,
                    "accountFence":previous_fence,
                }),
            );
        }
        Ok(())
    }

    fn agent_roster_mutation(&mut self, params: &Value) -> Result<Value, String> {
        let operation_id = required_string(params, "operationId")?;
        let expected_account_fence = required_string(params, "accountFence")?;
        let current_account_fence = self.current_turn_account_fence()?;
        if current_account_fence != expected_account_fence {
            return Err("presentation roster mutation is fenced to a stale account".into());
        }
        let mutation = params
            .get("mutation")
            .filter(|value| value.is_object())
            .ok_or("presentation roster mutation payload is required")?;

        if mutation.get("kind").and_then(Value::as_str) == Some("sidebar-sections") {
            let sections = mutation
                .get("sections")
                .and_then(Value::as_array)
                .ok_or("sidebar sections array is required")?
                .iter()
                .cloned()
                .map(serde_json::from_value::<AndroidSidebarSection>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("invalid sidebar section payload: {error}"))?;
            let known_agent_ids = self
                .agents
                .lock()
                .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
                .list()
                .into_iter()
                .map(|agent| agent.id)
                .collect::<BTreeSet<_>>();
            return self
                .sidebar_sections
                .lock()
                .map_err(|_| "canonical Android sidebar sections lock poisoned".to_string())?
                .set(
                    &current_account_fence,
                    operation_id,
                    &sections,
                    &known_agent_ids,
                );
        }

        self.agents
            .lock()
            .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
            .apply_presentation_operation(
                &current_account_fence,
                operation_id,
                mutation,
            )
    }

    fn agent_sidebar_sections(&mut self) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let known_agent_ids = self
            .agents
            .lock()
            .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
            .list()
            .into_iter()
            .map(|agent| agent.id)
            .collect::<BTreeSet<_>>();
        let sections = self
            .sidebar_sections
            .lock()
            .map_err(|_| "canonical Android sidebar sections lock poisoned".to_string())?
            .get(&account_fence, &known_agent_ids);
        Ok(json!({"sections":sections}))
    }

    fn agent_disk_pressure_observe(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let total_bytes = params
            .get("totalBytes")
            .and_then(Value::as_u64)
            .ok_or("totalBytes is required")?;
        let available_bytes = params
            .get("availableBytes")
            .and_then(Value::as_u64)
            .ok_or("availableBytes is required")?;
        let observed = self
            .turn_lifecycle
            .lock()
            .map_err(|_| "turn lifecycle lock poisoned".to_string())?
            .observe_disk_pressure_sample(
                &account_fence,
                total_bytes,
                available_bytes,
                now_ms(),
            )?;
        let level = match observed.level {
            ProductionDiskPressureLevel::Healthy => "healthy",
            ProductionDiskPressureLevel::Soft => "soft",
            ProductionDiskPressureLevel::Hard => "hard",
        };
        Ok(json!({
            "level":level,
            "episodeId":observed.episode_id,
            "changed":observed.changed,
        }))
    }

    fn agent_disk_pressure_record(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let conversation_id = required_string(params, "conversationId")?;
        let episode_id = required_string(params, "episodeId")?;
        self.turn_lifecycle
            .lock()
            .map_err(|_| "turn lifecycle lock poisoned".to_string())?
            .record_disk_pressure_episode(
                &account_fence,
                conversation_id,
                episode_id,
                now_ms(),
            )?;
        Ok(json!({
            "conversationId":conversation_id,
            "episodeId":episode_id,
            "recorded":true,
        }))
    }

    fn agent_upgrade_quiesce(&mut self, params: &Value) -> Result<Value, String> {
        let quiescing = params
            .get("quiescing")
            .and_then(Value::as_bool)
            .ok_or("quiescing is required")?;
        self.turn_upgrade_quiescing
            .store(quiescing, Ordering::Release);
        Ok(json!({"quiescing":quiescing}))
    }

    fn agent_turn_reconcile(&mut self, params: &Value) -> Result<Value, String> {
        let request_id = required_string(params, "requestId")?;
        let outcome = required_string(params, "outcome")?;
        let account_fence = self.current_turn_account_fence()?;
        let state = match outcome {
            "completed" => DurableTurnState::Completed,
            "failed" => DurableTurnState::Failed,
            "cancelled" => DurableTurnState::Cancelled,
            _ => {
                return Err(
                    "outcome must be completed, failed, or cancelled after external reconciliation"
                        .into(),
                )
            }
        };

        let record = {
            let journal = self
                .turn_journal
                .lock()
                .map_err(|_| "turn journal lock poisoned".to_string())?;
            journal
                .record(request_id)
                .cloned()
                .ok_or("durable turn record is missing")?
        };
        if record.account_fence != account_fence {
            return Err("outcome-unknown reconciliation is fenced by account identity".into());
        }
        if record.state != DurableTurnState::OutcomeUnknown {
            return Err("durable turn is not outcome-unknown".into());
        }

        if state == DurableTurnState::Completed {
            let assistant_text = params
                .get("assistantText")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or("completed reconciliation requires proven assistantText")?;
            let mut transcript = self
                .transcript
                .lock()
                .map_err(|_| "transcript lock poisoned".to_string())?;
            let agent_id = transcript
                .get_transcript()
                .into_iter()
                .find(|entry| {
                    entry.get("role").and_then(Value::as_str) == Some("user")
                        && entry.get("operationId").and_then(Value::as_str)
                            == Some(record.operation_id.as_str())
                })
                .and_then(|entry| {
                    entry.get("agentId")
                        .and_then(Value::as_str)
                        .filter(|value| !value.trim().is_empty())
                        .map(str::to_string)
                });
            let mut assistant_entry = json!({
                "id":format!("assistant:{}", record.operation_id),
                "kind":"message",
                "role":"assistant",
                "content":assistant_text,
                "operationId":record.operation_id,
                "timestampMs":now_ms(),
                "reconciled":true,
            });
            if let Some(agent_id) = agent_id {
                assistant_entry["agentId"] = Value::String(agent_id);
            }
            transcript
                .append_entry_if_absent(assistant_entry)
                .map_err(|error| {
                    format!("failed to persist reconciled assistant transcript entry: {error}")
                })?;
        }

        let reconciled = self
            .turn_journal
            .lock()
            .map_err(|_| "turn journal lock poisoned".to_string())?
            .reconcile_outcome_unknown(
                request_id,
                &account_fence,
                state,
                params.get("reason").and_then(Value::as_str).map(str::to_string),
                now_ms(),
            )?;
        let disk_reconciled = self
            .turn_lifecycle
            .lock()
            .map_err(|_| "turn lifecycle lock poisoned".to_string())?
            .reconcile_disk_pressure_claim(
                &account_fence,
                &reconciled.operation_id,
                reconciled.state == DurableTurnState::Completed,
                now_ms(),
            )?;

        Ok(json!({
            "requestId":request_id,
            "operationId":reconciled.operation_id,
            "state":format!("{:?}", reconciled.state).to_ascii_lowercase(),
            "diskPressureReconciled":disk_reconciled,
        }))
    }

    fn agent_subagent_tool(&mut self, params: &Value) -> Result<Value, String> {
        let tool_name = required_string(params, "toolName")?;
        let tool_call_id = required_string(params, "toolCallId")?;
        let parent_agent_id = required_string(params, "parentAgentId")?;
        let parent_request_id = required_string(params, "parentRequestId")?;
        let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        if !args.is_object() {
            return Err("arguments must be a JSON object".into());
        }
        let account_fence = self.current_turn_account_fence()?;
        let is_subagent_runner = parent_agent_id.starts_with("generated:");
        let subagent_capabilities = parse_turn_subagent_capability_projection(
            params.get(COORDINATOR_SUBAGENT_CAPABILITIES_FIELD),
        )?;
        let allowed_subagent_types = build_turn_subagent_types(
            is_subagent_runner,
            subagent_capabilities.multitask_enabled,
            subagent_capabilities.remote_box_available,
            subagent_capabilities.remote_box_has_desktop,
            subagent_capabilities.browser_use_enabled,
        )
        .unwrap_or_default();
        let model_id = if tool_name == crate::runner::TASK_TOOL_NAME {
            AndroidHostInferenceProvider::resolve_model_id(required_string(params, "model")?)
        } else {
            params
                .get("model")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("management-only")
                .to_string()
        };
        let privacy_mode = match self.mode {
            AndroidHostMode::Test => "no-storage".to_string(),
            AndroidHostMode::Production => self
                .mcp_dashboard_backend
                .as_ref()
                .and_then(|backend| backend.resolve_sand_privacy_mode())
                .map(|mode| match mode {
                    BackendSandPrivacyMode::Unspecified => "unspecified",
                    BackendSandPrivacyMode::NoStorage => "no-storage",
                    BackendSandPrivacyMode::NoTraining => "no-training",
                    BackendSandPrivacyMode::UsageDataTrainingAllowed => "usage-data-training-allowed",
                    BackendSandPrivacyMode::UsageCodebaseTrainingAllowed => "usage-codebase-training-allowed",
                })
                .unwrap_or("unspecified")
                .to_string(),
        };
        let frozen_turn = SubagentFrozenTurnConfig {
            provider_id: "android-host-inference".into(),
            model_id,
            tool_names: Vec::new(),
            allowed_subagent_types,
            privacy_mode,
            summarization_binding_id: "android-host-inference:same-provider".into(),
        };
        let context = SubagentToolContext {
            parent_agent_id: parent_agent_id.to_string(),
            parent_request_id: parent_request_id.to_string(),
            root_parent_request_id: params
                .get("rootParentRequestId")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string),
            account_fence: account_fence.clone(),
            box_id: params
                .get("boxId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            quiet_origin: params
                .get("quietOrigin")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string),
            frozen_turn,
            child_capabilities: subagent_capabilities,
        };
        let review_required = tool_name == crate::runner::TASK_TOOL_NAME
            || tool_name == crate::runner::MESSAGE_SUBAGENT_TOOL_NAME;
        let (bearer_token, mutation) = if review_required {
            self.bearer_token_for_turn()?
        } else {
            (None, None)
        };
        let review_mode = match self.mode {
            AndroidHostMode::Test => AndroidInferenceMode::Test,
            AndroidHostMode::Production => AndroidInferenceMode::Production,
        };
        let review_cancelled = Arc::new(AtomicBool::new(false));
        let task_review_token = bearer_token.clone();
        let task_review_cancelled = Arc::clone(&review_cancelled);
        let task_review: SubagentTaskReviewCallback = Arc::new(
            move |prompt, subagent_type, tool_call_id| {
                if tool_call_id.trim().is_empty() {
                    return Err("generated subagent Task review input is invalid".into());
                }
                match AndroidHostInferenceProvider::run_subagent_review(
                    review_mode,
                    task_review_token.clone(),
                    Arc::clone(&task_review_cancelled),
                    "launch",
                    prompt,
                    None,
                    Some(subagent_type),
                )
                .map_err(|error| error.message)?
                {
                    AndroidSubagentReviewDecision::Allow => Ok(None),
                    AndroidSubagentReviewDecision::Block { reason, .. }
                    | AndroidSubagentReviewDecision::Reject { reason } => Ok(Some(reason)),
                }
            },
        );
        let steer_review_token = bearer_token.clone();
        let steer_review: SubagentSteerReviewCallback = Arc::new(
            move |subagent_id, message, tool_call_id| {
                if tool_call_id.trim().is_empty() {
                    return Err("generated subagent steer review input is invalid".into());
                }
                match AndroidHostInferenceProvider::run_subagent_review(
                    review_mode,
                    steer_review_token.clone(),
                    Arc::clone(&review_cancelled),
                    "steer",
                    message,
                    Some(subagent_id),
                    None,
                )
                .map_err(|error| error.message)?
                {
                    AndroidSubagentReviewDecision::Allow => Ok(SubagentSteerReview {
                        allowed: true,
                        reason: String::new(),
                    }),
                    AndroidSubagentReviewDecision::Block { reason, .. }
                    | AndroidSubagentReviewDecision::Reject { reason } => Ok(SubagentSteerReview {
                        allowed: false,
                        reason,
                    }),
                }
            },
        );
        let reviewed_tools = self
            .subagent_tools
            .clone()
            .with_task_review(task_review)
            .with_steer_review(steer_review);
        let result = reviewed_tools.call(tool_name, &args, tool_call_id, &context, now_ms())?;
        let Some(launch) = result.launch else {
            return Ok(with_account_session_mutation(result.value, mutation));
        };
        if let Err(error) = spawn_generated_subagent(
            self.mode,
            bearer_token,
            Arc::clone(&self.subagent_owner),
            reviewed_tools,
            Arc::clone(&self.subagent_events),
            launch.clone(),
        ) {
            let epoch = self
                .subagent_owner
                .lock()
                .map_err(|_| "subagent owner lock poisoned".to_string())?
                .process_epoch();
            let _ = self
                .subagent_owner
                .lock()
                .map_err(|_| "subagent owner lock poisoned".to_string())?
                .settle(
                    &launch.record.subagent_id,
                    &account_fence,
                    epoch,
                    SubagentRunOutcome::Failed(error.clone()),
                    now_ms(),
                );
            return Err(error);
        }
        Ok(with_account_session_mutation(result.value, mutation))
    }

    fn agent_subagent_reconcile(&mut self, params: &Value) -> Result<Value, String> {
        let subagent_id = required_string(params, "subagentId")?;
        let outcome = required_string(params, "outcome")?;
        let account_fence = self.current_turn_account_fence()?;
        let outcome = match outcome {
            "completed" => SubagentRunOutcome::Completed(
                params
                    .get("result")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ),
            "failed" => SubagentRunOutcome::Failed(
                params
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("externally reconciled failure")
                    .to_string(),
            ),
            "aborted" => SubagentRunOutcome::Aborted,
            _ => return Err("outcome must be completed, failed, or aborted".into()),
        };
        let record = self
            .subagent_owner
            .lock()
            .map_err(|_| "subagent owner lock poisoned".to_string())?
            .reconcile_outcome_unknown(
                subagent_id,
                &account_fence,
                outcome,
                now_ms(),
            )?;
        Ok(json!({
            "subagentId":record.subagent_id,
            "subagentRequestId":record.subagent_request_id,
            "status":crate::runner::status_label(record.status),
            "result":record.completion_result,
            "error":record.completion_error,
        }))
    }

    fn account_logout(&mut self) -> Result<Value, String> {
        let previous_fence = self.current_turn_account_fence().ok();
        if let Some(previous_fence) = previous_fence.as_deref() {
            self.fence_turns_for_account_change(previous_fence, "account-logout")?;
        }
        self.pending_plugin_variable_writes.clear();
        self.cancel_mcp_auth_watches_for_account_change("account-logout")?;
        *self
            .live_account_fence
            .lock()
            .map_err(|_| "live account fence lock poisoned".to_string())? = None;
        if self.mode == AndroidHostMode::Test {
            self.logged_in = false;
            return Ok(self.auth_status());
        }
        #[cfg(feature = "ci-account-session-import")]
        if self.ci_session_identity.is_some() {
            self.logged_in = false;
            self.ci_session_identity = None;
            if let Some(path) = self.ci_session_path.take() {
                let _ = std::fs::remove_file(path);
            }
            return Ok(self.auth_status());
        }
        let (status, mutation) = self.account.logout()?;
        Ok(with_account_session_mutation(status, Some(mutation)))
    }

    fn auth_browser_start(&mut self) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Test {
            return self.browser_start();
        }
        self.account.browser_start()
    }

    fn auth_browser_reopen(&mut self, params: &Value) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Test {
            return self.browser_reopen(params);
        }
        self.account.browser_reopen(required_string(params, "attemptId")?)
    }

    fn auth_browser_cancel(&mut self, params: &Value) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Test {
            return self.browser_cancel(params);
        }
        self.account.browser_cancel(required_string(params, "attemptId")?)
    }

    fn auth_browser_poll(&mut self, params: &Value) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Test {
            return self.browser_poll(params);
        }
        let previous_fence = self.account.session_fence();
        let (result, mutation) = self
            .account
            .browser_poll(required_string(params, "attemptId")?)?;
        let current_fence = self.account.session_fence();
        *self
            .live_account_fence
            .lock()
            .map_err(|_| "live account fence lock poisoned".to_string())? = current_fence.clone();
        if previous_fence.is_some()
            && current_fence.is_some()
            && previous_fence != current_fence
        {
            if let Some(previous_fence) = previous_fence.as_deref() {
                self.fence_turns_for_account_change(previous_fence, "account-switch")?;
            }
            self.cancel_mcp_auth_watches_for_account_change("account-switch")?;
        }
        Ok(with_account_session_mutation(result, mutation))
    }

    fn auth_status(&self) -> Value {
        if self.logged_in {
            json!({
                "loggedIn":true,
                "user":{
                    "nickname":"Fabushi",
                    "username":"fabushi",
                    "email":"fabushi@example.invalid"
                }
            })
        } else {
            json!({"loggedIn":false,"user":Value::Null})
        }
    }

    fn device_agent_session(&self) -> Value {
        if !self.logged_in {
            return json!({"loggedIn": false});
        }
        #[cfg(feature = "ci-account-session-import")]
        {
            let Some(identity) = self.ci_session_identity.as_ref() else {
                return json!({
                    "loggedIn": true,
                    "available": false,
                    "reason": "device_agent_session_unavailable"
                });
            };
            return json!({
                "loggedIn": true,
                "available": true,
                "accessToken": identity.access_token,
                "deviceId": identity.device_id,
                "sessionId": identity.session_id,
                "accessTokenExpiresAt": identity.expires_at_epoch_seconds,
            });
        }
        #[cfg(not(feature = "ci-account-session-import"))]
        {
            json!({
                "loggedIn": true,
                "available": false,
                "reason": "device_agent_session_unavailable"
            })
        }
    }

    fn next_attempt_id(&mut self, prefix: &str) -> String {
        self.next_attempt = self.next_attempt.saturating_add(1);
        format!("{prefix}_{:08}", self.next_attempt)
    }

    fn next_operation_id(&mut self, request_id: &str) -> String {
        self.next_operation = self.next_operation.saturating_add(1);
        if request_id.trim().is_empty() {
            format!("android-operation-{:08}", self.next_operation)
        } else {
            request_id.to_string()
        }
    }

    fn browser_start(&mut self) -> Result<Value, String> {
        let attempt_id = self.next_attempt_id("browser");
        let url = format!("about:blank#fabushi-test-browser-login?attemptId={attempt_id}");
        self.browser_attempts.insert(attempt_id.clone(), "pending".into());
        Ok(json!({"attemptId":attempt_id,"url":url,"status":"pending"}))
    }

    fn browser_reopen(&mut self, params: &Value) -> Result<Value, String> {
        let attempt_id = required_string(params, "attemptId")?;
        if !self.browser_attempts.contains_key(attempt_id) {
            return Err("browser login attempt is unknown".into());
        }
        Ok(json!({
            "attemptId":attempt_id,
            "url":format!("about:blank#fabushi-test-browser-login?attemptId={attempt_id}")
        }))
    }

    fn browser_cancel(&mut self, params: &Value) -> Result<Value, String> {
        let attempt_id = required_string(params, "attemptId")?.to_string();
        self.browser_attempts.insert(attempt_id.clone(), "cancelled".into());
        Ok(json!({"attemptId":attempt_id,"status":"cancelled"}))
    }

    fn browser_poll(&mut self, params: &Value) -> Result<Value, String> {
        let attempt_id = required_string(params, "attemptId")?.to_string();
        let status = self.browser_attempts.get(&attempt_id).cloned().unwrap_or_else(|| "unknown".into());
        if self.mode == AndroidHostMode::Test && status == "pending" {
            self.logged_in = true;
            self.browser_attempts.insert(attempt_id.clone(), "completed".into());
            return Ok(json!({"attemptId":attempt_id,"status":"completed","auth":self.auth_status()}));
        }
        Ok(json!({"attemptId":attempt_id,"status":status}))
    }


    fn mcp_authenticate(&mut self, params: &Value) -> Result<Value, String> {
        let owner = self
            .mcp_auth_owner
            .as_ref()
            .ok_or("MCP auth owner is available only in production")?;
        let result = owner.authenticate(
            now_ms(),
            required_string(params, "serverId")?,
            required_string(params, "accountKey")?,
            required_string(params, "oauthRedirectUri")?,
            params
                .get("requestingAgentId")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty()),
            params
                .get("forceReauth")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        )?;
        Ok(match result {
            McpAuthenticateResult::AlreadyAuthenticated => json!({"status":"already-authenticated"}),
            McpAuthenticateResult::AuthorizationRequired {
                authorization_url,
                watch,
                replaced,
            } => {
                let oauth_state = url::Url::parse(&authorization_url)
                    .ok()
                    .and_then(|url| {
                        url.query_pairs()
                            .find_map(|(key, value)| (key == "state").then(|| value.into_owned()))
                    })
                    .filter(|state| !state.trim().is_empty())
                    .ok_or("MCP authorization URL is missing OAuth state")?;
                self.mcp_auth_watches
                    .lock()
                    .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?
                    .bind_oauth_state(&oauth_state, &watch)?;
                let result = json!({
                    "status":"authorization-required",
                    "authorizationUrl":authorization_url,
                    "serverId":watch.server_id,
                    "serverName":watch.server_name,
                    "accountKey":watch.account_key,
                    "generation":watch.generation,
                    "expiresAtMs":watch.expires_at_ms,
                    "replacedGeneration":replaced.map(|value| value.generation),
                });
                self.events.push_back(json!({
                    "type":"mcp.authorization.required",
                    "authorizationUrl":result["authorizationUrl"],
                    "provider":result["serverId"],
                    "serverId":result["serverId"],
                    "serverName":result["serverName"],
                    "accountKey":result["accountKey"],
                    "generation":result["generation"],
                    "expiresAtMs":result["expiresAtMs"],
                }));
                result
            }
            McpAuthenticateResult::NotConfigured => json!({"status":"not-configured"}),
            McpAuthenticateResult::AdminBlocked => json!({"status":"admin-blocked"}),
            McpAuthenticateResult::UnsupportedTransport => json!({"status":"unsupported-transport"}),
            McpAuthenticateResult::Unreachable(detail) => json!({"status":"unreachable","detail":detail}),
            McpAuthenticateResult::NotSupported(detail) => json!({"status":"not-supported","detail":detail}),
        })
    }

    fn mcp_auth_watch_snapshot(&self) -> Result<Value, String> {
        Ok(self
            .mcp_auth_watches
            .lock()
            .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?
            .snapshot())
    }

    fn mcp_auth_watch_register(&mut self, params: &Value) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Production {
            return Err("external MCP auth watch register is disabled; Host owns the shipping lifecycle".into());
        }
        let server_id = required_string(params, "serverId")?;
        let server_name = required_string(params, "serverName")?;
        let server_url = required_string(params, "serverUrl")?;
        let account_key = required_string(params, "accountKey")?;
        let requesting_agent_id = params
            .get("requestingAgentId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let force_reauth = params
            .get("forceReauth")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let (watch, replaced) = self
            .mcp_auth_watches
            .lock()
            .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?
            .begin_watch(
            now_ms(),
            server_id,
            server_name,
            server_url,
            account_key,
            requesting_agent_id,
            force_reauth,
        )?;
        Ok(json!({
            "serverId":watch.server_id,
            "serverName":watch.server_name,
            "accountKey":watch.account_key,
            "generation":watch.generation,
            "expiresAtMs":watch.expires_at_ms,
            "replacedGeneration":replaced.map(|value| value.generation),
        }))
    }

    fn mcp_auth_watch_poll(&mut self, params: &Value) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Production {
            return Err("external MCP auth watch poll is disabled; Host owns the shipping lifecycle".into());
        }
        let server_id = required_string(params, "serverId")?;
        let account_key = required_string(params, "accountKey")?;
        let tick = self
            .mcp_auth_watches
            .lock()
            .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?
            .poll_tick(now_ms(), server_id, account_key)?;
        Ok(match tick {
            McpAuthPollTick::Idle => json!({"status":"idle"}),
            McpAuthPollTick::Suppressed => json!({"status":"suppressed"}),
            McpAuthPollTick::Expired(completion) => {
                self.emit_mcp_auth_watch_completion(&completion, Some("watch-timeout"));
                json!({
                    "status":"expired",
                    "generation":completion.generation,
                    "serverId":completion.server_id,
                    "serverName":completion.server_name,
                    "accountKey":completion.account_key,
                    "requestingAgentId":completion.requesting_agent_id,
                    "outcome":completion.outcome,
                })
            }
            McpAuthPollTick::Request(request) => json!({
                "status":"poll",
                "generation":request.generation,
                "serverId":request.server_id,
                "serverName":request.server_name,
                "serverUrl":request.server_url,
                "accountKey":request.account_key,
                "requestingAgentId":request.requesting_agent_id,
                "deadlineMs":request.deadline_ms,
            }),
        })
    }

    fn mcp_auth_watch_settle(&mut self, params: &Value) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Production {
            return Err("external MCP auth watch settle is disabled; Host owns the shipping lifecycle".into());
        }
        let request = McpAuthPollRequest {
            generation: required_u64(params, "generation")?,
            server_id: required_string(params, "serverId")?.to_string(),
            server_name: required_string(params, "serverName")?.to_string(),
            server_url: required_string(params, "serverUrl")?.to_string(),
            account_key: required_string(params, "accountKey")?.to_string(),
            requesting_agent_id: params
                .get("requestingAgentId")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string),
            deadline_ms: required_u64(params, "deadlineMs")?,
        };
        if now_ms() > request.deadline_ms {
            let settlement = self
                .mcp_auth_watches
                .lock()
                .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?
                .poll_failed(now_ms(), &request)?;
            return Ok(mcp_auth_poll_settlement_json(settlement));
        }
        let outcome = match required_string(params, "outcome")? {
            "token-valid" => McpAuthPollOutcome::TokenValid,
            "token-invalid" => McpAuthPollOutcome::TokenInvalid,
            "admin-blocked" => McpAuthPollOutcome::AdminBlocked,
            "unreachable" => McpAuthPollOutcome::Unreachable,
            _ => return Err("unsupported MCP auth poll outcome".into()),
        };
        let settlement = self
            .mcp_auth_watches
            .lock()
            .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?
            .settle_poll(now_ms(), &request, outcome)?;
        let result = mcp_auth_poll_settlement_json(settlement.clone());
        match settlement {
            McpAuthPollSettlement::Completed(completion)
            | McpAuthPollSettlement::Cancelled(completion) => {
                self.emit_mcp_auth_watch_completion(&completion, None);
            }
            McpAuthPollSettlement::Pending | McpAuthPollSettlement::Stale => {}
        }
        Ok(result)
    }

    fn emit_mcp_auth_watch_completion(
        &mut self,
        completion: &McpAuthWatchCompletion,
        reason: Option<&str>,
    ) {
        let mut event = json!({
            "type":"mcp.auth.watch.settled",
            "serverId":completion.server_id,
            "serverName":completion.server_name,
            "accountKey":completion.account_key,
            "requestingAgentId":completion.requesting_agent_id,
            "generation":completion.generation,
            "outcome":completion.outcome,
        });
        if let Some(reason) = reason {
            event["reason"] = json!(reason);
        }
        self.events.push_back(event);
    }

    fn mcp_auth_watch_cancel(&mut self, params: &Value) -> Result<Value, String> {
        let server_id = required_string(params, "serverId")?;
        let account_key = required_string(params, "accountKey")?;
        let completion = self
            .mcp_auth_watches
            .lock()
            .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?
            .cancel_watch(server_id, account_key)?;
        Ok(match completion {
            Some(completion) => {
                self.emit_mcp_auth_watch_completion(&completion, Some("user-cancelled"));
                json!({
                    "status":"cancelled",
                    "generation":completion.generation,
                    "serverId":completion.server_id,
                    "serverName":completion.server_name,
                    "accountKey":completion.account_key,
                    "requestingAgentId":completion.requesting_agent_id,
                    "outcome":completion.outcome,
                })
            }
            None => json!({"status":"stale"}),
        })
    }

    fn mcp_oauth_complete(&mut self, params: &Value) -> Result<Value, String> {
        let provider = required_string(params, "provider")?.to_string();
        let state = required_string(params, "state")?.to_string();
        let code = params
            .get("code")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let error = params
            .get("error")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        if code.is_some() == error.is_some() {
            return Err("MCP OAuth completion requires exactly one of code or error".into());
        }

        let explicit_identity = match (
            params.get("serverId").and_then(Value::as_str),
            params.get("accountKey").and_then(Value::as_str),
            params.get("generation").and_then(Value::as_u64),
        ) {
            (Some(server_id), Some(account_key), Some(generation))
                if !server_id.trim().is_empty()
                    && !account_key.trim().is_empty()
                    && generation > 0 =>
            {
                Some((
                    server_id.trim().to_string(),
                    account_key.trim().to_string(),
                    generation,
                ))
            }
            (None, None, None) => None,
            _ => {
                return Err(
                    "MCP OAuth completion identity must include serverId, accountKey, and generation"
                        .into(),
                )
            }
        };

        let durable_identity = self
            .mcp_auth_watches
            .lock()
            .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?
            .resolve_oauth_state(&state, now_ms())
            .map(|binding| (binding.server_id, binding.account_key, binding.generation));

        let identity = match (explicit_identity, durable_identity.clone()) {
            (Some(explicit), Some(durable)) if explicit != durable => {
                return Ok(json!({
                    "provider":provider,
                    "state":state,
                    "status":"stale",
                }));
            }
            (Some(explicit), _) => Some(explicit),
            (None, Some(durable)) => Some(durable),
            (None, None) => {
                return Ok(json!({
                    "provider":provider,
                    "state":state,
                    "status":"stale",
                }));
            }
        };

        let watch = if let Some((server_id, account_key, generation)) = identity.as_ref() {
            let manager = self
                .mcp_auth_watches
                .lock()
                .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?;
            let Some(watch) = manager.watch(server_id, account_key).cloned() else {
                return Ok(json!({
                    "provider":provider,
                    "state":state,
                    "status":"stale",
                }));
            };
            if watch.generation != *generation {
                return Ok(json!({
                    "provider":provider,
                    "state":state,
                    "status":"stale",
                }));
            }
            Some(watch)
        } else {
            None
        };

        if let Some(code) = code {
            if let Some(owner) = self.mcp_auth_owner.as_ref() {
                owner.complete_oauth(&state, code)?;
            } else if self.mode == AndroidHostMode::Production {
                return Err("MCP OAuth backend owner is unavailable".into());
            }
            if durable_identity.is_some() {
                let consumed = self
                    .mcp_auth_watches
                    .lock()
                    .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?
                    .consume_oauth_state(&state, now_ms())?;
                if consumed.is_none() {
                    return Ok(json!({
                        "provider":provider,
                        "state":state,
                        "status":"stale",
                    }));
                }
            }
            let server_id = watch.as_ref().map(|value| value.server_id.clone());
            let server_name = watch.as_ref().map(|value| value.server_name.clone());
            let account_key = watch.as_ref().map(|value| value.account_key.clone());
            let requesting_agent_id = watch
                .as_ref()
                .and_then(|value| value.requesting_agent_id.clone());
            self.events.push_back(json!({
                "type":"mcp.auth.callback.accepted",
                "provider":provider.clone(),
                "state":state.clone(),
                "outcome":"pending-validation",
                "serverId":server_id,
                "serverName":server_name,
                "accountKey":account_key,
                "requestingAgentId":requesting_agent_id,
            }));
            return Ok(json!({
                "provider":provider,
                "state":state,
                "outcome":"pending-validation",
                "serverId":server_id,
                "serverName":server_name,
                "accountKey":account_key,
                "requestingAgentId":requesting_agent_id,
            }));
        }

        let cancelled = if let Some((server_id, account_key, _generation)) = identity.as_ref() {
            let mut manager = self
                .mcp_auth_watches
                .lock()
                .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?;
            manager.cancel_watch(server_id, account_key)?
        } else {
            None
        };
        let server_id = cancelled.as_ref().map(|value| value.server_id.clone());
        let server_name = cancelled.as_ref().map(|value| value.server_name.clone());
        let account_key = cancelled.as_ref().map(|value| value.account_key.clone());
        let requesting_agent_id = cancelled
            .as_ref()
            .and_then(|value| value.requesting_agent_id.clone());
        self.events.push_back(json!({
            "type":"mcp.auth.failed",
            "provider":provider.clone(),
            "state":state.clone(),
            "outcome":"failed",
            "serverId":server_id,
            "serverName":server_name,
            "accountKey":account_key,
            "requestingAgentId":requesting_agent_id,
        }));
        Ok(json!({
            "provider":provider,
            "state":state,
            "outcome":"failed",
            "serverId":server_id,
            "serverName":server_name,
            "accountKey":account_key,
            "requestingAgentId":requesting_agent_id,
        }))
    }

    fn oauth_start(&mut self, params: &Value) -> Result<Value, String> {
        let provider = required_string(params, "provider")?;
        if self.mode == AndroidHostMode::Test {
            let attempt_id = self.next_attempt_id("oauth");
            self.oauth_attempts.insert(attempt_id.clone());
            return Ok(json!({
                "attemptId":attempt_id,
                "provider":provider,
                "url":format!("https://api.ombhrum.com/sign-in?attempt={attempt_id}")
            }));
        }
        let result = self.account.browser_start_for_provider(Some(provider))?;
        Ok(json!({
            "attemptId":result["attemptId"],
            "provider":provider,
            "url":result["loginUrl"],
            "status":result["status"],
            "pollAfterMs":result["pollAfterMs"],
            "expiresAt":result["expiresAt"],
        }))
    }

    fn oauth_poll(&mut self, params: &Value) -> Result<Value, String> {
        let attempt_id = required_string(params, "attemptId")?;
        if self.mode == AndroidHostMode::Test {
            if !self.oauth_attempts.remove(attempt_id) {
                return Err("OAuth attempt is unknown or already consumed".into());
            }
            self.logged_in = true;
            return Ok(json!({"attemptId":attempt_id,"status":"completed","auth":self.auth_status()}));
        }
        let previous_fence = self.account.session_fence();
        let (result, mutation) = self.account.browser_poll(attempt_id)?;
        let current_fence = self.account.session_fence();
        *self
            .live_account_fence
            .lock()
            .map_err(|_| "live account fence lock poisoned".to_string())? = current_fence.clone();
        if previous_fence.is_some()
            && current_fence.is_some()
            && previous_fence != current_fence
        {
            if let Some(previous_fence) = previous_fence.as_deref() {
                self.fence_turns_for_account_change(previous_fence, "account-switch")?;
            }
            self.cancel_mcp_auth_watches_for_account_change("account-switch")?;
        }
        Ok(with_account_session_mutation(result, mutation))
    }

    fn oauth_cancel(&mut self, params: &Value) -> Result<Value, String> {
        let attempt_id = required_string(params, "attemptId")?;
        if self.mode == AndroidHostMode::Test {
            if !self.oauth_attempts.remove(attempt_id) {
                return Err("OAuth attempt is unknown or already consumed".into());
            }
            return Ok(json!({"attemptId":attempt_id,"status":"cancelled"}));
        }
        self.account.browser_cancel(attempt_id)
    }

    fn feature_execute(&mut self, params: &Value) -> Result<Value, String> {
        let command = params.get("command").and_then(Value::as_object).ok_or("feature.execute requires command")?;
        let command = Value::Object(command.clone());
        let kind = required_string(&command, "type")?;
        let request_id = command.get("requestId").and_then(Value::as_str).unwrap_or("");
        let operation_id = self.next_operation_id(request_id);
        self.active_operations.insert(operation_id.clone());
        self.events.push_back(json!({
            "type":"operation.started",
            "operationId":operation_id,
            "requestId":request_id
        }));
        let mut private_session_mutation = None;

        match kind {
            "bot.list" => {
                let bots = self.project_agent_roster()?;
                self.events.push_back(json!({
                    "type":"bot.listed",
                    "operationId":operation_id,
                    "requestId":request_id,
                    "bots":bots,
                }));
                self.finish_operation(&operation_id);
            }
            "bot.create" => {
                let name = required_string(&command, "name")?;
                let description = command.get("description").and_then(Value::as_str).unwrap_or("");
                let agent = self.agents
                    .lock()
                    .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
                    .create(name, description)
                    .map_err(|error| error.to_string())?;
                self.events.push_back(json!({
                    "type":"bot.created",
                    "operationId":operation_id,
                    "requestId":request_id,
                    "bot":agent.as_json(),
                }));
                self.finish_operation(&operation_id);
            }
            "chat.send" => {
                let (bearer_token, session_mutation) = match self.bearer_token_for_turn() {
                    Ok(value) => value,
                    Err(error) => {
                        self.active_operations.remove(&operation_id);
                        return Err(error);
                    }
                };
                if let Err(error) = self.queue_chat_turn(
                    &operation_id,
                    request_id,
                    &command,
                    bearer_token,
                ) {
                    self.active_operations.remove(&operation_id);
                    return Err(error);
                }
                private_session_mutation = session_mutation;
            }
            "marketplace.install" => {
                if self.mode == AndroidHostMode::Test {
                    let id = required_string(&command, "miniAppId")?;
                    self.installed_plugins.insert(id.to_string());
                    self.finish_operation(&operation_id);
                } else {
                    let release = command
                        .get("release")
                        .ok_or("marketplace.install requires the immutable release manifest")?
                        .clone();
                    let installed = self.plugin_install(&json!({
                        "release":release,
                        "platform":"android",
                    }))?;
                    self.events.push_back(json!({
                        "type":"marketplace.installed",
                        "operationId":operation_id,
                        "requestId":request_id,
                        "plugin":installed,
                    }));
                    self.finish_operation(&operation_id);
                }
            }
            "miniapp.open" | "session.clear" => {
                self.finish_operation(&operation_id);
            }
            "capability.request" => {
                let capability = required_string(&command, "capability")?;
                if capability.len() > 256 || capability.chars().any(char::is_control) {
                    self.active_operations.remove(&operation_id);
                    return Err("capability identity is invalid".into());
                }
                let approval_id = format!("approval-{operation_id}");
                if self.pending_approvals.contains_key(&approval_id) {
                    self.active_operations.remove(&operation_id);
                    return Err("approval identity collision".into());
                }
                let account_fence = self.current_turn_account_fence()?;
                let approval_request_id = if request_id.trim().is_empty() {
                    operation_id.as_str()
                } else {
                    request_id
                };
                let target = command.get("target").cloned().unwrap_or(Value::Null);
                self.capability_broker.request_approval(
                    &approval_id,
                    approval_request_id,
                    &operation_id,
                    capability,
                    target.clone(),
                    &account_fence,
                    now_ms(),
                )?;
                self.pending_approvals
                    .insert(approval_id.clone(), operation_id.clone());
                self.events.push_back(json!({
                    "type":"approval.requested",
                    "operationId":operation_id,
                    "approvalId":approval_id,
                    "capability":capability,
                    "target":target,
                    "accountFence":account_fence,
                    "reason":command.get("reason").cloned().unwrap_or(Value::Null)
                }));
            }
            "runtime.longTask" => {}
            _ => {
                self.active_operations.remove(&operation_id);
                return Err(format!("unsupported Android feature command: {kind}"));
            }
        }

        Ok(with_account_session_mutation(
            json!({"requestId":request_id,"operationId":operation_id,"accepted":true}),
            private_session_mutation,
        ))
    }

    fn bearer_token_for_turn(
        &mut self,
    ) -> Result<(Option<String>, Option<AccountSessionMutation>), String> {
        if self.mode == AndroidHostMode::Test {
            return Ok((None, None));
        }
        #[cfg(feature = "ci-account-session-import")]
        if let Some(identity) = self.ci_session_identity.as_ref() {
            return Ok((Some(identity.access_token.clone()), None));
        }
        let (token, mutation) = self.account.valid_access_token()?;
        Ok((Some(token), mutation))
    }

    fn queue_chat_turn(
        &mut self,
        operation_id: &str,
        request_id: &str,
        command: &Value,
        bearer_token: Option<String>,
    ) -> Result<(), String> {
        let prompt = command
            .get("text")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("chat.send text is required")?
            .to_string();
        if !text_size_allowed(&prompt) {
            return Err("chat.send text exceeds maximum composer size".into());
        }
        let hidden = command
            .get("hidden")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        // The Coordinator overwrites this projection with process-owned capability
        // state. Validate it before *any* durable turn mutation so a malformed or
        // inconsistent capability snapshot cannot leave a ghost transcript entry.
        let subagent_capabilities = parse_turn_subagent_capability_projection(
            command.get(COORDINATOR_SUBAGENT_CAPABILITIES_FIELD),
        )?;
        if self.turn_upgrade_quiescing.load(Ordering::Acquire) {
            return Err("Agent turns are quiescing for upgrade; new dispatch is fenced.".into());
        }

        let agent_id = command
            .get("agentId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("mahayana-assistant")
            .to_string();
        let assistant_entry_id = format!("assistant:{operation_id}");
        if !hidden {
            let mut transcript = self
                .transcript
                .lock()
                .map_err(|_| "transcript lock poisoned".to_string())?;
            let user_was_new = transcript
                .append_entry_if_absent(json!({
                    "id":request_id,
                    "kind":"message",
                    "role":"user",
                    "content":prompt.clone(),
                    "operationId":operation_id,
                    "agentId":agent_id.clone(),
                    "timestampMs":now_ms(),
                }))
                .map_err(|error| format!("failed to persist user transcript entry: {error}"))?;

            if !user_was_new {
                if let Some(existing) = transcript.entry(&assistant_entry_id) {
                    let text = existing
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let mut events = self
                        .turn_events
                        .lock()
                        .map_err(|_| "turn event queue lock poisoned".to_string())?;
                    if !text.is_empty() {
                        events.push_back(json!({
                            "type":"chat.message",
                            "operationId":operation_id,
                            "requestId":request_id,
                            "role":"assistant",
                            "text":text,
                            "recovered":true,
                        }));
                    }
                    events.push_back(json!({
                        "type":"operation.completed",
                        "operationId":operation_id,
                        "requestId":request_id,
                        "finishReason":"recovered",
                        "attempts":0,
                        "deduped":true,
                    }));
                    return Ok(());
                }
            }
        }

        let model = AndroidHostInferenceProvider::resolve_model_id(
            command
                .get("model")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("default"),
        );
        let account_fence = self.current_turn_account_fence()?;
        let turn_generation = self
            .turn_journal
            .lock()
            .map_err(|_| "turn journal lock poisoned".to_string())?
            .begin(request_id, operation_id, &account_fence, now_ms())?;

        let profile = self.agents
            .lock()
            .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
            .get(&agent_id);
        let profile_revision = profile.as_ref().map(|profile| {
            crate::sha256::sha256_hex(
                format!(
                    "{}\n{}\n{}\n{}",
                    profile.id, profile.name, profile.description, profile.updated_at
                )
                .as_bytes(),
            )
        });
        let profile_prompt = match (profile.as_ref(), profile_revision.as_deref()) {
            (Some(profile), Some(revision))
                if self
                    .turn_lifecycle
                    .lock()
                    .map_err(|_| "turn lifecycle lock poisoned".to_string())?
                    .profile_announcement_needed(&account_fence, &agent_id, revision) =>
            {
                Some(format!(
                    "<agent_profile>\nName: {}\nDescription: {}\n</agent_profile>",
                    profile.name, profile.description
                ))
            }
            _ => None,
        };

        let cancelled = Arc::new(AtomicBool::new(false));
        self.turn_cancellations
            .insert(operation_id.to_string(), cancelled.clone());
        self.agent_turn_interruptions.register(
            &agent_id,
            operation_id,
            &account_fence,
            Arc::clone(&cancelled),
        )?;

        let mode = self.mode;
        let turn_events = self.turn_events.clone();
        let transcript = self.transcript.clone();
        let turn_journal = self.turn_journal.clone();
        let turn_lifecycle = self.turn_lifecycle.clone();
        let turn_upgrade_quiescing = Arc::clone(&self.turn_upgrade_quiescing);
        let account_fence_owned = account_fence.clone();
        let operation_id_owned = operation_id.to_string();
        let request_id_owned = request_id.to_string();
        let assistant_entry_id_owned = assistant_entry_id.clone();
        let conversation_id_owned = agent_id.clone();
        let model_owned = model.clone();
        let summarization_token = bearer_token.clone();
        let profile_revision_owned = profile_revision.clone();
        let subagent_owner = Arc::clone(&self.subagent_owner);
        let subagent_events = Arc::clone(&self.subagent_events);
        let subagent_tools = self.subagent_tools.clone();
        let subagent_review_process_epoch = self
            .subagent_owner
            .lock()
            .map_err(|_| "durable subagent owner lock poisoned".to_string())?
            .process_epoch();
        let subagent_review_broker = self.capability_broker.clone();
        let subagent_review_approvals = Arc::clone(&self.subagent_review_approvals);
        let subagent_review_events = Arc::clone(&self.turn_events);
        let multitask_todos = Arc::clone(&self.multitask_todos);
        let agent_roster = Arc::clone(&self.agents);
        let agent_messaging = Arc::clone(&self.messaging);
        let live_account_fence = Arc::clone(&self.live_account_fence);
        let agent_turn_interruptions = Arc::clone(&self.agent_turn_interruptions);
        let remote_capability_broker = self.capability_broker.clone();
        let remote_binding = Arc::clone(&self.remote_binding);
        let remote_runner = Arc::clone(&self.remote_runner);
        let remote_approvals = self.remote_approvals.clone();
        let frozen_privacy = match self.mode {
            AndroidHostMode::Test => ProductionTurnPrivacyMode::NoStorage,
            AndroidHostMode::Production => self
                .mcp_dashboard_backend
                .as_ref()
                .and_then(|backend| backend.resolve_sand_privacy_mode())
                .map(|privacy| match privacy {
                    BackendSandPrivacyMode::Unspecified => ProductionTurnPrivacyMode::Unspecified,
                    BackendSandPrivacyMode::NoStorage => ProductionTurnPrivacyMode::NoStorage,
                    BackendSandPrivacyMode::NoTraining => ProductionTurnPrivacyMode::NoTraining,
                    BackendSandPrivacyMode::UsageDataTrainingAllowed => {
                        ProductionTurnPrivacyMode::UsageDataTrainingAllowed
                    }
                    BackendSandPrivacyMode::UsageCodebaseTrainingAllowed => {
                        ProductionTurnPrivacyMode::UsageCodebaseTrainingAllowed
                    }
                })
                .unwrap_or(ProductionTurnPrivacyMode::Unspecified),
        };
        let frozen_privacy_label = match frozen_privacy {
            ProductionTurnPrivacyMode::Unspecified => "unspecified",
            ProductionTurnPrivacyMode::NoStorage => "no-storage",
            ProductionTurnPrivacyMode::NoTraining => "no-training",
            ProductionTurnPrivacyMode::UsageDataTrainingAllowed => "usage-data-training-allowed",
            ProductionTurnPrivacyMode::UsageCodebaseTrainingAllowed => "usage-codebase-training-allowed",
        }
        .to_string();
        let allowed_subagent_types = build_turn_subagent_types(
            false,
            subagent_capabilities.multitask_enabled,
            subagent_capabilities.remote_box_available,
            subagent_capabilities.remote_box_has_desktop,
            subagent_capabilities.browser_use_enabled,
        )
        .unwrap_or_default();
        let frozen_subagent_turn = SubagentFrozenTurnConfig {
            provider_id: "android-host-inference".into(),
            model_id: model.clone(),
            tool_names: Vec::new(),
            allowed_subagent_types,
            privacy_mode: frozen_privacy_label,
            summarization_binding_id: "android-host-inference:same-provider".into(),
        };
        let request_source = command
            .get("requestSource")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        let subagent_review_expiry_policy =
            subagent_review_approval_expiry_policy(request_source.as_deref());

        let spawn = thread::Builder::new()
            .name(format!("fabushi-turn-{}", operation_id.chars().take(32).collect::<String>()))
            .spawn(move || {
                let review_mode = match mode {
                    AndroidHostMode::Test => AndroidInferenceMode::Test,
                    AndroidHostMode::Production => AndroidInferenceMode::Production,
                };
                let task_review_token = bearer_token.clone();
                let task_review_cancelled = Arc::clone(&cancelled);
                let task_review_broker = subagent_review_broker.clone();
                let task_review_approvals = Arc::clone(&subagent_review_approvals);
                let task_review_events = Arc::clone(&subagent_review_events);
                let task_review_parent_agent = conversation_id_owned.clone();
                let task_review_parent_request = request_id_owned.clone();
                let task_review_account_fence = account_fence_owned.clone();
                let task_review: SubagentTaskReviewCallback = Arc::new(
                    move |prompt, subagent_type, tool_call_id| {
                        if tool_call_id.trim().is_empty() {
                            return Err("generated subagent Task review input is invalid".into());
                        }
                        if task_review_approvals.has_pending_for_agent(&task_review_parent_agent) {
                            return Err("a subagent approval is already pending for this agent".into());
                        }
                        match AndroidHostInferenceProvider::run_subagent_review(
                            review_mode,
                            task_review_token.clone(),
                            Arc::clone(&task_review_cancelled),
                            "launch",
                            prompt,
                            None,
                            Some(subagent_type),
                        )
                        .map_err(|error| error.message)?
                        {
                            AndroidSubagentReviewDecision::Allow => Ok(None),
                            AndroidSubagentReviewDecision::Reject { reason } => Ok(Some(reason)),
                            AndroidSubagentReviewDecision::Block { reason, proposed_rule } => {
                                let approved = wait_for_subagent_review_approval(
                                    &task_review_broker,
                                    &task_review_approvals,
                                    &task_review_events,
                                    &task_review_cancelled,
                                    &task_review_parent_agent,
                                    &task_review_parent_request,
                                    &task_review_account_fence,
                                    subagent_review_process_epoch,
                                    tool_call_id,
                                    "launch",
                                    prompt,
                                    None,
                                    Some(subagent_type),
                                    &reason,
                                    proposed_rule.as_deref(),
                                    subagent_review_expiry_policy,
                                )?;
                                Ok((!approved).then_some(reason))
                            }
                        }
                    },
                );
                let steer_review_token = bearer_token.clone();
                let steer_review_cancelled = Arc::clone(&cancelled);
                let steer_review_broker = subagent_review_broker.clone();
                let steer_review_approvals = Arc::clone(&subagent_review_approvals);
                let steer_review_events = Arc::clone(&subagent_review_events);
                let steer_review_parent_agent = conversation_id_owned.clone();
                let steer_review_parent_request = request_id_owned.clone();
                let steer_review_account_fence = account_fence_owned.clone();
                let steer_review: SubagentSteerReviewCallback = Arc::new(
                    move |subagent_id, message, tool_call_id| {
                        if tool_call_id.trim().is_empty() {
                            return Err("generated subagent steer review input is invalid".into());
                        }
                        match AndroidHostInferenceProvider::run_subagent_review(
                            review_mode,
                            steer_review_token.clone(),
                            Arc::clone(&steer_review_cancelled),
                            "steer",
                            message,
                            Some(subagent_id),
                            None,
                        )
                        .map_err(|error| error.message)?
                        {
                            AndroidSubagentReviewDecision::Allow => Ok(SubagentSteerReview {
                                allowed: true,
                                reason: String::new(),
                            }),
                            AndroidSubagentReviewDecision::Reject { reason } => Ok(SubagentSteerReview {
                                allowed: false,
                                reason,
                            }),
                            AndroidSubagentReviewDecision::Block { reason, proposed_rule } => {
                                let approved = wait_for_subagent_review_approval(
                                    &steer_review_broker,
                                    &steer_review_approvals,
                                    &steer_review_events,
                                    &steer_review_cancelled,
                                    &steer_review_parent_agent,
                                    &steer_review_parent_request,
                                    &steer_review_account_fence,
                                    subagent_review_process_epoch,
                                    tool_call_id,
                                    "steer",
                                    message,
                                    Some(subagent_id),
                                    None,
                                    &reason,
                                    proposed_rule.as_deref(),
                                    subagent_review_expiry_policy,
                                )?;
                                Ok(SubagentSteerReview {
                                    allowed: approved,
                                    reason: if approved { String::new() } else { reason },
                                })
                            }
                        }
                    },
                );
                let reviewed_subagent_tools = subagent_tools
                    .clone()
                    .with_task_review(task_review)
                    .with_steer_review(steer_review);
                let routed_subagent_tools = build_parent_subagent_routed_tools(
                    mode,
                    bearer_token.clone(),
                    Arc::clone(&subagent_owner),
                    reviewed_subagent_tools,
                    Arc::clone(&subagent_events),
                    SubagentToolContext {
                        parent_agent_id: conversation_id_owned.clone(),
                        parent_request_id: request_id_owned.clone(),
                        root_parent_request_id: Some(request_id_owned.clone()),
                        account_fence: account_fence_owned.clone(),
                        box_id: String::new(),
                        quiet_origin: request_source.clone(),
                        frozen_turn: frozen_subagent_turn.clone(),
                        child_capabilities: subagent_capabilities,
                    },
                );
                // Desktop root turns expose Agent management through the same
                // canonical roster and messaging owners. Generated child runners
                // never receive this wrapper, so root management authority is not inherited.
                let routed_subagent_tools = with_agent_management_tools(
                    routed_subagent_tools,
                    Arc::clone(&agent_roster),
                    Arc::clone(&agent_messaging),
                    Arc::clone(&live_account_fence),
                    &account_fence_owned,
                    &conversation_id_owned,
                    Arc::clone(&cancelled),
                    Arc::clone(&agent_turn_interruptions),
                );
                // Desktop exposes TodoWrite only on a root non-subagent turn when
                // multitask is enabled. Keep it outside GeneratedChildToolRegistry
                // so no generated child can inherit root bookkeeping authority.
                let routed_subagent_tools = if subagent_capabilities.multitask_enabled {
                    with_multitask_todo_tools(
                        routed_subagent_tools,
                        Arc::clone(&multitask_todos),
                        &account_fence_owned,
                        &conversation_id_owned,
                    )
                } else {
                    routed_subagent_tools
                };
                let routed_subagent_tools = if subagent_capabilities.remote_box_available {
                    with_remote_routed_tools(
                        routed_subagent_tools,
                        remote_capability_broker.clone(),
                        Arc::clone(&remote_binding),
                        Arc::clone(&remote_runner),
                        remote_approvals.clone(),
                        Arc::clone(&live_account_fence),
                        Arc::clone(&turn_events),
                        &operation_id_owned,
                        &request_id_owned,
                        Arc::clone(&cancelled),
                    )
                } else {
                    routed_subagent_tools
                };

                let provider = match mode {
                    AndroidHostMode::Test => {
                        AndroidHostInferenceProvider::new(AndroidInferenceMode::Test)
                            .with_routed_tools(Arc::clone(&routed_subagent_tools))
                    }
                    AndroidHostMode::Production => {
                        let Some(token) = bearer_token else {
                            let settled = turn_journal
                                .lock()
                                .map_err(|_| "turn journal lock poisoned".to_string())
                                .and_then(|mut journal| journal.settle(
                                    &request_id_owned,
                                    &operation_id_owned,
                                    &account_fence_owned,
                                    turn_generation,
                                    DurableTurnState::Failed,
                                    Some("provider_credentials_unavailable".into()),
                                    now_ms(),
                                ))
                                .is_ok();
                            if settled {
                                push_turn_event(
                                    &turn_events,
                                    json!({
                                        "type":"operation.failed",
                                        "operationId":operation_id_owned,
                                        "requestId":request_id_owned,
                                        "message":"provider_credentials_unavailable",
                                    }),
                                );
                            }
                            return;
                        };
                        match AndroidHostInferenceProvider::production(token, cancelled.clone()) {
                            Ok(provider) => provider.with_routed_tools(Arc::clone(&routed_subagent_tools)),
                            Err(error) => {
                                let message = error.message;
                                let settled = turn_journal
                                    .lock()
                                    .map_err(|_| "turn journal lock poisoned".to_string())
                                    .and_then(|mut journal| journal.settle(
                                        &request_id_owned,
                                        &operation_id_owned,
                                        &account_fence_owned,
                                        turn_generation,
                                        DurableTurnState::Failed,
                                        Some(message.clone()),
                                        now_ms(),
                                    ))
                                    .is_ok();
                                if settled {
                                    push_turn_event(
                                        &turn_events,
                                        json!({
                                            "type":"operation.failed",
                                            "operationId":operation_id_owned,
                                            "requestId":request_id_owned,
                                            "message":message,
                                        }),
                                    );
                                }
                                return;
                            }
                        }
                    }
                };

                let privacy_mode_resolver = Arc::new(move || Some(frozen_privacy));
                let summarization_cancelled = Arc::clone(&cancelled);
                let summarization_model = model_owned.clone();
                let summarization_prompt = Arc::new(
                    move |system_prompt: &str,
                          user_prompt: &str,
                          should_cancel: &dyn Fn() -> bool| {
                        AndroidHostInferenceProvider::run_summarization_prompt_with_model(
                            match mode {
                                AndroidHostMode::Test => AndroidInferenceMode::Test,
                                AndroidHostMode::Production => AndroidInferenceMode::Production,
                            },
                            summarization_token.clone(),
                            Arc::clone(&summarization_cancelled),
                            &summarization_model,
                            system_prompt,
                            user_prompt,
                            should_cancel,
                        )
                    },
                );
                let build_bindings = ProductionTurnAgentBuildBindings::new(
                    ProductionTurnAgentStaticConfig {
                        model_id: model_owned.clone(),
                        agent_token_limit: SAND_AGENT_TOKEN_LIMIT,
                        conversation_id: conversation_id_owned.clone(),
                        is_box_scoped_subagent: false,
                        is_subagent_runner: false,
                        is_shared_room_runner: false,
                        sand_send_message_delivery_owed: false,
                        transcripts_folder_available: true,
                    },
                    privacy_mode_resolver,
                    summarization_prompt,
                );

                let claim_store = Arc::clone(&turn_lifecycle);
                let claim_account = account_fence_owned.clone();
                let claim_conversation = conversation_id_owned.clone();
                let claim_operation = operation_id_owned.clone();
                let commit_store = Arc::clone(&turn_lifecycle);
                let commit_account = account_fence_owned.clone();
                let commit_conversation = conversation_id_owned.clone();
                let commit_operation = operation_id_owned.clone();
                let release_store = Arc::clone(&turn_lifecycle);
                let release_account = account_fence_owned.clone();
                let release_conversation = conversation_id_owned.clone();
                let release_operation = operation_id_owned.clone();

                let mut lifecycle_bindings = ProductionTurnAgentLifecycleBindings::new(
                    account_fence_owned.clone(),
                    conversation_id_owned.clone(),
                    operation_id_owned.clone(),
                )
                .with_disk_pressure_callbacks(
                    Arc::new(move || {
                        claim_store
                            .lock()
                            .map_err(|_| "turn lifecycle lock poisoned".to_string())?
                            .claim_disk_pressure(
                                &claim_account,
                                &claim_conversation,
                                &claim_operation,
                                now_ms(),
                            )
                    }),
                    Arc::new(move || {
                        commit_store
                            .lock()
                            .map_err(|_| "turn lifecycle lock poisoned".to_string())?
                            .commit_disk_pressure(
                                &commit_account,
                                &commit_conversation,
                                &commit_operation,
                                now_ms(),
                            )
                    }),
                    Arc::new(move || {
                        release_store
                            .lock()
                            .map_err(|_| "turn lifecycle lock poisoned".to_string())?
                            .release_disk_pressure(
                                &release_account,
                                &release_conversation,
                                &release_operation,
                                now_ms(),
                            )
                    }),
                );

                if let (Some(profile_prompt), Some(profile_revision)) =
                    (profile_prompt.clone(), profile_revision_owned.clone())
                {
                    let profile_store = Arc::clone(&turn_lifecycle);
                    let profile_account = account_fence_owned.clone();
                    let profile_agent = conversation_id_owned.clone();
                    let profile_revision_commit = profile_revision.clone();
                    let profile_commit: ProductionTurnProfileAnnouncementCommit =
                        Arc::new(move || {
                            if let Ok(mut store) = profile_store.lock() {
                                let _ = store.commit_profile_announcement(
                                    &profile_account,
                                    &profile_agent,
                                    &profile_revision_commit,
                                    now_ms(),
                                );
                            }
                        });
                    lifecycle_bindings = lifecycle_bindings.with_profile_announcement(
                        Some(profile_prompt),
                        Some(profile_commit),
                    );
                }

                let mut owner = match ProductionTurnAgentOwner::new(provider)
                    .with_upgrade_quiesce_signal(turn_upgrade_quiescing)
                    .with_build_bindings(build_bindings)
                    .with_lifecycle_bindings(lifecycle_bindings)
                {
                    Ok(owner) => owner,
                    Err(error) => {
                        let settled = turn_journal
                            .lock()
                            .map_err(|_| "turn journal lock poisoned".to_string())
                            .and_then(|mut journal| {
                                journal.settle(
                                    &request_id_owned,
                                    &operation_id_owned,
                                    &account_fence_owned,
                                    turn_generation,
                                    DurableTurnState::Failed,
                                    Some(error.message.clone()),
                                    now_ms(),
                                )
                            })
                            .is_ok();
                        if settled {
                            push_turn_event(
                                &turn_events,
                                json!({
                                    "type":"operation.failed",
                                    "operationId":operation_id_owned,
                                    "requestId":request_id_owned,
                                    "message":error.message,
                                }),
                            );
                        }
                        return;
                    }
                };
                let mut final_text = String::new();
                let mut terminal_emitted = false;
                let mut sink = |event: ProductionTurnEvent| -> Result<(), String> {
                    turn_journal
                        .lock()
                        .map_err(|_| "turn journal lock poisoned".to_string())?
                        .assert_current(
                            &request_id_owned,
                            &operation_id_owned,
                            &account_fence_owned,
                            turn_generation,
                        )?;
                    if cancelled.load(Ordering::Acquire) {
                        return Err("cancelled".into());
                    }
                    match event {
                        ProductionTurnEvent::Retrying {
                            attempt,
                            delay_ms,
                            reason,
                        } => push_turn_event(
                            &turn_events,
                            json!({
                                "type":"turn.retrying",
                                "operationId":operation_id_owned,
                                "requestId":request_id_owned,
                                "attempt":attempt,
                                "delayMs":delay_ms,
                                "reason":reason,
                            }),
                        ),
                        ProductionTurnEvent::Delta(delta) => {
                            final_text.push_str(&delta);
                            push_turn_event(
                                &turn_events,
                                json!({
                                    "type":"chat.delta",
                                    "operationId":operation_id_owned,
                                    "requestId":request_id_owned,
                                    "delta":delta,
                                }),
                            );
                        }
                        ProductionTurnEvent::Completed {
                            finish_reason,
                            attempts,
                        } => {
                            if !final_text.is_empty() {
                                let persist = transcript
                                    .lock()
                                    .map_err(|_| "transcript lock poisoned".to_string())?
                                    .append_entry_if_absent(json!({
                                        "id":assistant_entry_id_owned,
                                        "kind":"message",
                                        "role":"assistant",
                                        "content":final_text.clone(),
                                        "operationId":operation_id_owned,
                                        "agentId":conversation_id_owned.clone(),
                                        "timestampMs":now_ms(),
                                    }))
                                    .map_err(|error| {
                                        format!("failed to persist assistant transcript entry: {error}")
                                    })?;
                                let _ = persist;
                                push_turn_event(
                                    &turn_events,
                                    json!({
                                        "type":"chat.message",
                                        "operationId":operation_id_owned,
                                        "requestId":request_id_owned,
                                        "role":"assistant",
                                        "text":final_text.clone(),
                                    }),
                                );
                            }
                            turn_journal
                                .lock()
                                .map_err(|_| "turn journal lock poisoned".to_string())?
                                .settle(
                                    &request_id_owned,
                                    &operation_id_owned,
                                    &account_fence_owned,
                                    turn_generation,
                                    DurableTurnState::Completed,
                                    None,
                                    now_ms(),
                                )?;
                            terminal_emitted = true;
                            push_turn_event(
                                &turn_events,
                                json!({
                                    "type":"operation.completed",
                                    "operationId":operation_id_owned,
                                    "requestId":request_id_owned,
                                    "finishReason":finish_reason,
                                    "attempts":attempts,
                                }),
                            );
                        }
                        ProductionTurnEvent::Failed { message } => {
                            turn_journal
                                .lock()
                                .map_err(|_| "turn journal lock poisoned".to_string())?
                                .settle(
                                    &request_id_owned,
                                    &operation_id_owned,
                                    &account_fence_owned,
                                    turn_generation,
                                    DurableTurnState::Failed,
                                    Some(message.clone()),
                                    now_ms(),
                                )?;
                            terminal_emitted = true;
                            push_turn_event(
                                &turn_events,
                                json!({
                                    "type":"operation.failed",
                                    "operationId":operation_id_owned,
                                    "requestId":request_id_owned,
                                    "message":message,
                                }),
                            );
                        }
                        ProductionTurnEvent::Cancelled => {
                            let _ = turn_journal
                                .lock()
                                .map_err(|_| "turn journal lock poisoned".to_string())?
                                .settle_operation_cancelled(
                                    &operation_id_owned,
                                    "runner cancelled",
                                    now_ms(),
                                )?;
                            terminal_emitted = true;
                            push_turn_event(
                                &turn_events,
                                json!({
                                    "type":"operation.interrupted",
                                    "operationId":operation_id_owned,
                                    "requestId":request_id_owned,
                                    "reason":"cancelled",
                                }),
                            );
                        }
                    }
                    Ok(())
                };

                let result = owner.run_with_event_sink(
                    ProductionTurnInput {
                        operation_id: operation_id_owned.clone(),
                        request_id: request_id_owned.clone(),
                        agent_id,
                        model,
                        prompt,
                        resume_checkpoint_available: false,
                    },
                    &mut sink,
                );

                if let Err(error) = result {
                    if !cancelled.load(Ordering::Acquire) && !terminal_emitted {
                        let settled = turn_journal
                            .lock()
                            .map_err(|_| "turn journal lock poisoned".to_string())
                            .and_then(|mut journal| journal.settle(
                                &request_id_owned,
                                &operation_id_owned,
                                &account_fence_owned,
                                turn_generation,
                                DurableTurnState::Failed,
                                Some(error.message.clone()),
                                now_ms(),
                            ))
                            .is_ok();
                        if settled {
                            push_turn_event(
                                &turn_events,
                                json!({
                                    "type":"operation.failed",
                                    "operationId":operation_id_owned,
                                    "requestId":request_id_owned,
                                    "message":error.message,
                                }),
                            );
                        }
                    }
                }
            });

        if let Err(error) = spawn {
            self.turn_cancellations.remove(operation_id);
            self.agent_turn_interruptions.unregister_operation(operation_id);
            self.active_operations.remove(operation_id);
            let _ = self
                .turn_journal
                .lock()
                .map_err(|_| "turn journal lock poisoned".to_string())?
                .settle(
                    request_id,
                    operation_id,
                    &account_fence,
                    turn_generation,
                    DurableTurnState::Failed,
                    Some(format!("failed to start turn worker: {error}")),
                    now_ms(),
                );
            return Err(format!("failed to start turn worker: {error}"));
        }

        Ok(())
    }

    fn queue_mcp_auth_resume_turn(
        &mut self,
        completion: &McpAuthWatchCompletion,
    ) -> Result<Option<AccountSessionMutation>, String> {
        let Some(agent_id) = completion
            .requesting_agent_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        else {
            return Ok(None);
        };
        let display_name = mcp_auth_display_name(&completion.server_name, &completion.account_key);
        let prompt = format!(
            "[The \"{display_name}\" MCP server finished authorizing — it's connected and its tools are available now. Your first action is a SendMessage telling the user it's connected, then pick up whatever you paused to authorize it. If there was nothing else to do, just confirm it's ready and ask what they'd like to do with it. Remember: nothing reaches the user unless it's inside a SendMessage.]"
        );
        let request_id = format!(
            "mcp-auth-resume:{}:{}:{}:{}",
            completion.server_id,
            completion.account_key,
            completion.generation,
            agent_id,
        );
        let operation_id = self.next_operation_id(&request_id);
        self.active_operations.insert(operation_id.clone());
        let (bearer_token, session_mutation) = match self.bearer_token_for_turn() {
            Ok(value) => value,
            Err(error) => {
                self.active_operations.remove(&operation_id);
                return Err(error);
            }
        };
        let command = json!({
            "type":"chat.send",
            "requestId":request_id,
            "agentId":agent_id,
            "text":prompt,
            "hidden":true,
            "requestSource":"mcp-auth-resume",
            "skipAckObligation":true,
        });
        if let Err(error) = self.queue_chat_turn(
            &operation_id,
            command.get("requestId").and_then(Value::as_str).unwrap_or_default(),
            &command,
            bearer_token,
        ) {
            self.active_operations.remove(&operation_id);
            return Err(error);
        }
        Ok(session_mutation)
    }

    fn drain_mcp_auth_owner_events(&mut self) -> Result<(), String> {
        let Some(owner) = self.mcp_auth_owner.as_ref() else {
            return Ok(());
        };
        for event in owner.drain_events()? {
            match event {
                McpAuthOwnerEvent::Completed(completion) => {
                    let completion_is_pending = self
                        .mcp_auth_watches
                        .lock()
                        .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?
                        .pending_completions()
                        .iter()
                        .any(|pending| {
                            pending.generation == completion.generation
                                && pending.server_id == completion.server_id
                                && pending.account_key == completion.account_key
                        });
                    if !completion_is_pending {
                        continue;
                    }
                    let (resume_mutation, resume_accepted) =
                        match self.queue_mcp_auth_resume_turn(&completion) {
                            Ok(mutation) => (mutation, true),
                            Err(error) => {
                                self.events.push_back(json!({
                                    "type":"mcp.auth.resume.failed",
                                    "serverId":completion.server_id,
                                    "serverName":completion.server_name,
                                    "accountKey":completion.account_key,
                                    "requestingAgentId":completion.requesting_agent_id,
                                    "generation":completion.generation,
                                    "message":error,
                                    "source":"host-auth-watch-owner",
                                }));
                                (None, false)
                            }
                        };
                    if resume_accepted {
                        self.mcp_auth_watches
                            .lock()
                            .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?
                            .ack_completion(
                                completion.generation,
                                &completion.server_id,
                                &completion.account_key,
                            )?;
                    }
                    self.events.push_back(with_account_session_mutation(
                        json!({
                            "type":"mcp.auth.completed",
                            "serverId":completion.server_id,
                            "serverName":completion.server_name,
                            "accountKey":completion.account_key,
                            "requestingAgentId":completion.requesting_agent_id,
                            "generation":completion.generation,
                            "outcome":completion.outcome,
                            "source":"host-auth-watch-owner",
                        }),
                        resume_mutation,
                    ));
                }
                McpAuthOwnerEvent::Cancelled(completion) => {
                    self.events.push_back(json!({
                        "type":"mcp.auth.failed",
                        "serverId":completion.server_id,
                        "serverName":completion.server_name,
                        "accountKey":completion.account_key,
                        "requestingAgentId":completion.requesting_agent_id,
                        "generation":completion.generation,
                        "outcome":"cancelled",
                        "source":"host-auth-watch-owner",
                    }));
                }
                McpAuthOwnerEvent::Expired(completion) => {
                    self.events.push_back(json!({
                        "type":"mcp.auth.failed",
                        "serverId":completion.server_id,
                        "serverName":completion.server_name,
                        "accountKey":completion.account_key,
                        "requestingAgentId":completion.requesting_agent_id,
                        "generation":completion.generation,
                        "outcome":"timeout",
                        "source":"host-auth-watch-owner",
                    }));
                }
                McpAuthOwnerEvent::BackendUnavailable {
                    generation,
                    server_id,
                    account_key,
                } => {
                    self.events.push_back(json!({
                        "type":"mcp.auth.poll.unreachable",
                        "serverId":server_id,
                        "accountKey":account_key,
                        "generation":generation,
                        "source":"host-auth-watch-owner",
                    }));
                }
            }
        }
        Ok(())
    }

    fn drain_agent_wake_once(&mut self) -> Result<(), String> {
        let account_fence = match self.current_turn_account_fence() {
            Ok(value) => value,
            Err(_) => return Ok(()),
        };
        let wake = {
            let messaging = self
                .messaging
                .lock()
                .map_err(|_| "canonical Android messaging owner lock poisoned".to_string())?;
            messaging
                .pending_agent_wakes(
                    &account_fence,
                    i64::try_from(now_ms()).unwrap_or(i64::MAX),
                )
                .into_iter()
                .next()
        };
        let Some(wake) = wake else { return Ok(()); };
        let wake_id = required_string(&wake, "wakeId")?.to_string();
        let target_agent_id = required_string(&wake, "targetAgentId")?.to_string();
        let source_agent_id = required_string(&wake, "sourceAgentId")?.to_string();
        if self
            .agent_turn_interruptions
            .has_active_turn(&target_agent_id, &account_fence)
        {
            return Ok(());
        }
        let target_exists = self
            .agents
            .lock()
            .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
            .get(&target_agent_id)
            .is_some();
        if !target_exists {
            self.messaging
                .lock()
                .map_err(|_| "canonical Android messaging owner lock poisoned".to_string())?
                .defer_agent_wake(
                    &wake_id,
                    i64::try_from(now_ms()).unwrap_or(i64::MAX),
                    "target-agent-missing",
                )?;
            return Ok(());
        }
        let source_name = self
            .agents
            .lock()
            .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
            .get(&source_agent_id)
            .map(|agent| agent.name)
            .unwrap_or_else(|| source_agent_id.clone());
        let message = required_string(&wake, "message")?;
        let priority = wake.get("priority").and_then(Value::as_bool).unwrap_or(false);
        let images = wake
            .get("images")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut prompt = vec![
            format!(
                "[agent] A message just arrived from another of your user's agents: {} (id: {}).",
                source_name, source_agent_id
            ),
            if priority {
                "This is a PRIORITY instruction from another assistant — not the user typing here. It interrupted your previous non-user work. Drop conflicting in-flight work and follow it now. Your user can already see it in this chat.".to_string()
            } else {
                "This is another assistant reaching out — not the user typing here. It arrived asynchronously, and your user can already see it in this chat.".to_string()
            },
            String::new(),
            format!("{}: {}", source_name, message),
        ];
        if !images.is_empty() {
            prompt.push(String::new());
            prompt.push(format!(
                "{} attached {} image(s) to this message:",
                source_name,
                images.len()
            ));
            for image in images {
                let url = image.get("url").and_then(Value::as_str).unwrap_or_default();
                let alt = image
                    .get("alt")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| format!(" — {value}"))
                    .unwrap_or_default();
                prompt.push(format!("- {url}{alt}"));
            }
        }
        prompt.push(String::new());
        prompt.push(format!(
            "If it needs a reply or an action, handle it and reply to {} with SendToAgent using id {}. Do not wait or poll for a reply; it arrives later on a fresh turn.",
            source_name, source_agent_id
        ));
        let request_id = wake_id.clone();
        let operation_id = self.next_operation_id(&request_id);
        if self.active_operations.contains(&operation_id)
            || self.agent_wake_operations.values().any(|value| value == &wake_id)
        {
            return Ok(());
        }
        let (bearer_token, _session_mutation) = match self.bearer_token_for_turn() {
            Ok(value) => value,
            Err(error) => {
                self.messaging
                    .lock()
                    .map_err(|_| "canonical Android messaging owner lock poisoned".to_string())?
                    .defer_agent_wake(
                        &wake_id,
                        i64::try_from(now_ms()).unwrap_or(i64::MAX),
                        &error,
                    )?;
                return Ok(());
            }
        };
        self.active_operations.insert(operation_id.clone());
        let command = json!({
            "type":"chat.send",
            "requestId":request_id,
            "agentId":target_agent_id,
            "model":"default",
            "text":prompt.join("\n"),
            "hidden":true,
            "requestSource":"agent-message",
            "skipAckObligation":true
        });
        match self.queue_chat_turn(
            &operation_id,
            command.get("requestId").and_then(Value::as_str).unwrap_or_default(),
            &command,
            bearer_token,
        ) {
            Ok(()) => {
                self.agent_wake_operations.insert(operation_id, wake_id);
            }
            Err(error) => {
                self.active_operations.remove(&operation_id);
                self.agent_turn_interruptions.unregister_operation(&operation_id);
                self.messaging
                    .lock()
                    .map_err(|_| "canonical Android messaging owner lock poisoned".to_string())?
                    .defer_agent_wake(
                        &wake_id,
                        i64::try_from(now_ms()).unwrap_or(i64::MAX),
                        &error,
                    )?;
            }
        }
        Ok(())
    }

    fn feature_receive(&mut self) -> Result<Value, String> {
        self.drain_mcp_auth_owner_events()?;
        self.drain_agent_wake_once()?;
        if let Some(event) = self.events.pop_front() {
            return Ok(event);
        }
        if let Some(event) = self
            .subagent_events
            .lock()
            .map_err(|_| "subagent event queue lock poisoned".to_string())?
            .pop_front()
        {
            return Ok(event);
        }

        for wait in 0..=10 {
            let event = self
                .turn_events
                .lock()
                .map_err(|_| "turn event queue lock poisoned".to_string())?
                .pop_front();
            if let Some(event) = event {
                if matches!(
                    event.get("type").and_then(Value::as_str),
                    Some("operation.completed")
                        | Some("operation.failed")
                        | Some("operation.interrupted")
                ) {
                    if let Some(operation_id) =
                        event.get("operationId").and_then(Value::as_str)
                    {
                        self.active_operations.remove(operation_id);
                        self.turn_cancellations.remove(operation_id);
                        self.agent_turn_interruptions.unregister_operation(operation_id);
                        if let Some(wake_id) = self.agent_wake_operations.remove(operation_id) {
                            let terminal = event.get("type").and_then(Value::as_str).unwrap_or_default();
                            let mut messaging = self
                                .messaging
                                .lock()
                                .map_err(|_| "canonical Android messaging owner lock poisoned".to_string())?;
                            if terminal == "operation.completed" {
                                messaging.complete_agent_wake(&wake_id)?;
                            } else {
                                messaging.defer_agent_wake(
                                    &wake_id,
                                    i64::try_from(now_ms()).unwrap_or(i64::MAX),
                                    terminal,
                                )?;
                            }
                        }
                    }
                }
                return Ok(event);
            }
            if wait < 10 && !self.active_operations.is_empty() {
                thread::sleep(Duration::from_millis(5));
            } else {
                break;
            }
        }

        Ok(json!({}))
    }

    fn finish_operation(&mut self, operation_id: &str) {
        self.active_operations.remove(operation_id);
        self.events.push_back(json!({
            "type":"operation.completed",
            "operationId":operation_id
        }));
    }

    fn feature_interrupt(&mut self, params: &Value) -> Result<Value, String> {
        let operation_id = required_string(params, "operationId")?.to_string();
        self.cancel_operation(&operation_id, Some("user"))?;
        Ok(json!({"operationId":operation_id,"status":"interrupted"}))
    }

    fn feature_approval_resolve(&mut self, params: &Value) -> Result<Value, String> {
        let approval_id = required_string(params, "approvalId")?.to_string();
        let approved = params
            .get("approved")
            .and_then(Value::as_bool)
            .ok_or("approved is required")?;
        let account_fence = self.current_turn_account_fence()?;
        if self.subagent_review_approvals.contains(&approval_id) {
            let resolved = self.capability_broker.resolve_approval(
                &approval_id,
                approved,
                &account_fence,
                now_ms(),
            )?;
            if let Err(error) = self.subagent_review_approvals.resolve(&approval_id, approved) {
                let _ = self.capability_broker.cancel_approval_operation(
                    &resolved.operation_id,
                    "auto-review waiter disappeared before resolution delivery",
                    now_ms(),
                );
                return Err(error);
            }
            self.events.push_back(json!({
                "type":"approval.resolved",
                "approvalId":approval_id,
                "operationId":resolved.operation_id,
                "capability":resolved.capability,
                "approved":approved,
                "autoReview":true,
            }));
            return Ok(json!({
                "status":"resolved",
                "approved":approved,
                "operationId":resolved.operation_id,
                "capability":resolved.capability,
                "execution": if approved { "review-unlocked" } else { "denied" },
            }));
        }
        if let Some(resolution) = self.remote_approvals.resolve_from_ui(
            &self.capability_broker,
            &approval_id,
            approved,
            &account_fence,
            now_ms(),
        )? {
            self.events.push_back(json!({
                "type":"approval.resolved",
                "approvalId":approval_id,
                "operationId":resolution.remote_operation_id,
                "parentOperationId":resolution.parent_operation_id,
                "capability":resolution.capability,
                "approved":approved,
                "remoteDispatch":true,
            }));
            return Ok(json!({
                "status":"resolved",
                "approved":approved,
                "operationId":resolution.remote_operation_id,
                "parentOperationId":resolution.parent_operation_id,
                "capability":resolution.capability,
                "execution": if approved { "dispatch-unlocked" } else { "denied" },
            }));
        }
        let operation_id = self
            .pending_approvals
            .get(&approval_id)
            .cloned()
            .ok_or("approval is unknown, stale, cancelled, or already consumed")?;
        if !self.active_operations.contains(&operation_id) {
            return Err("approval operation is no longer active".into());
        }
        let resolved = self.capability_broker.resolve_approval(
            &approval_id,
            approved,
            &account_fence,
            now_ms(),
        )?;
        if resolved.operation_id != operation_id {
            return Err("approval operation identity disagrees with durable broker state".into());
        }
        self.pending_approvals.remove(&approval_id);

        self.events.push_back(json!({
            "type":"approval.resolved",
            "approvalId":approval_id,
            "operationId":operation_id,
            "capability":resolved.capability,
            "approved":approved,
        }));
        self.active_operations.remove(&operation_id);
        if approved {
            self.events.push_back(json!({
                "type":"operation.completed",
                "operationId":operation_id,
                "approvalId":approval_id,
                "outcome":"authorized"
            }));
            Ok(json!({
                "status":"resolved",
                "approved":true,
                "operationId":operation_id,
                "capability":resolved.capability,
                "execution":"authorized"
            }))
        } else {
            self.events.push_back(json!({
                "type":"operation.interrupted",
                "operationId":operation_id,
                "reason":"approval-denied"
            }));
            Ok(json!({
                "status":"resolved",
                "approved":false,
                "operationId":operation_id,
                "capability":resolved.capability,
                "execution":"denied"
            }))
        }
    }

    fn marketplace_browse(&mut self, _params: &Value) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Test {
            return Ok(json!({"plugins":[{
                "pluginId":"global-dharma",
                "displayName":"全球法布施",
                "description":"Android deterministic Mini App",
                "latestVersion":"1.0.0",
                "commands":[]
            }]}));
        }
        let (result, mutation) = self.authenticated_platform_api(
            "GET",
            "/v1/marketplace/plugins?platform=android",
            None,
        )?;
        Ok(with_account_session_mutation(result, mutation))
    }

    fn marketplace_release(&mut self, params: &Value) -> Result<Value, String> {
        let plugin_id = required_string(params, "pluginId")?;
        let version = required_string(params, "version")?;
        let plugin_id = encode_api_path_segment(plugin_id)?;
        let version = encode_api_path_segment(version)?;
        if self.mode == AndroidHostMode::Test {
            let install = json!({
                "protocol":"fabushi.marketplace.install.v1",
                "strategy":"github-immutable",
                "source":{
                    "sourceRef":"test-fixture-only",
                    "marketplaceHostsPackage":false
                }
            });
            return Ok(json!({
                "pluginId":plugin_id,
                "version":version,
                "install":install,
                "releaseManifest":{
                    "pluginId":plugin_id,
                    "version":version,
                    "install":install
                }
            }));
        }
        let path = format!("/v1/marketplace/plugins/{plugin_id}/releases/{version}");
        let (result, mutation) = self.authenticated_platform_api("GET", &path, None)?;
        Ok(with_account_session_mutation(result, mutation))
    }

    fn platform_request(&mut self, params: &Value) -> Result<Value, String> {
        if params.get("authenticated").and_then(Value::as_bool) != Some(true) {
            return Err("platform.request requires the authenticated Fabushi account boundary".into());
        }
        let method = required_string(params, "method")?.to_ascii_uppercase();
        if !matches!(method.as_str(), "GET" | "POST") {
            return Err("platform.request supports only GET and POST".into());
        }
        let path = required_string(params, "path")?;
        validate_platform_api_path(path)?;
        let body = match params.get("body") {
            None | Some(Value::Null) => None,
            Some(value) if value.is_object() => Some(value.clone()),
            Some(_) => return Err("platform.request body must be a JSON object".into()),
        };
        if method == "GET" && body.is_some() {
            return Err("platform.request GET must not carry a request body".into());
        }
        let (result, mutation) = self.authenticated_platform_api(&method, path, body)?;
        Ok(with_account_session_mutation(
            json!({"ok":true,"data":result}),
            mutation,
        ))
    }

    fn authenticated_platform_api(
        &mut self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<(Value, Option<AccountSessionMutation>), String> {
        validate_platform_api_path(path)?;
        if self.mode == AndroidHostMode::Test {
            return Err("production platform API access is disabled in deterministic Host test mode".into());
        }
        #[cfg(feature = "ci-account-session-import")]
        if let Some(identity) = self.ci_session_identity.as_ref() {
            let result = self.account.api_request_with_bearer(
                method,
                path,
                body,
                &identity.access_token,
            )?;
            return Ok((result, None));
        }
        self.account.authenticated_api_request(method, path, body)
    }


    fn plugin_variable_fields(&self, params: &Value) -> Result<Value, String> {
        let schema = params.get("schema").unwrap_or(&Value::Null);
        Ok(json!({
            "fields": variable_fields_json(&PluginVariableStore::fields(schema)),
        }))
    }

    fn plugin_variable_prepare(&mut self, params: &Value) -> Result<Value, String> {
        let plugin_id = required_string(params, "pluginId")?.to_string();
        let schema = params.get("schema").cloned().unwrap_or_else(|| json!({}));
        let values = params.get("values").cloned().unwrap_or_else(|| json!({}));
        let team_configured = params
            .get("teamConfigured")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let account_key = self.current_turn_account_fence()?;
        let prepared = self.plugin_variables.prepare_write(
            &account_key,
            &plugin_id,
            &schema,
            &values,
            team_configured,
        )?;
        let write_id = self.next_attempt_id("plugin-vars");
        let secret_values = prepared.secret_values.clone();
        let secret_keys = prepared.secret_keys.iter().cloned().collect::<Vec<_>>();
        let public_config = prepared.public_config.clone();
        self.pending_plugin_variable_writes
            .insert(write_id.clone(), prepared);
        Ok(json!({
            "writeId":write_id,
            "pluginId":plugin_id,
            "accountKey":account_key,
            "publicConfig":public_config,
            "secretKeys":secret_keys,
            "secretValues":secret_values,
            "teamConfigured":team_configured,
        }))
    }

    fn plugin_variable_commit(&mut self, params: &Value) -> Result<Value, String> {
        let write_id = required_string(params, "writeId")?.to_string();
        let prepared = self
            .pending_plugin_variable_writes
            .remove(&write_id)
            .ok_or("plugin variable write is stale, cancelled, or already committed")?;
        let current_account_key = self.current_turn_account_fence()?;
        if prepared.account_key != current_account_key {
            return Err("plugin variable write account fence changed before commit".into());
        }
        self.plugin_variables.commit_write(&prepared)?;
        Ok(json!({
            "pluginId":prepared.plugin_id,
            "accountKey":prepared.account_key,
            "configured":true,
            "secretKeys":prepared.secret_keys,
            "teamConfigured":prepared.team_configured,
        }))
    }

    fn plugin_variable_runtime_config(&self, params: &Value) -> Result<Value, String> {
        let plugin_id = required_string(params, "pluginId")?;
        let account_key = self.current_turn_account_fence()?;
        if !self.plugin_variables.has_entry(&account_key, plugin_id) {
            return Ok(json!({
                "pluginId":plugin_id,
                "accountKey":account_key,
                "configured":false,
                "publicConfig":{},
                "secretKeys":[],
                "teamConfigured":false,
            }));
        }
        let config = self.plugin_variables.runtime_config(&account_key, plugin_id)?;
        Ok(json!({
            "pluginId":config.plugin_id,
            "accountKey":config.account_key,
            "configured":true,
            "publicConfig":config.public_config,
            "secretKeys":config.secret_keys,
            "teamConfigured":config.team_configured,
        }))
    }

    fn plugin_install(&mut self, params: &Value) -> Result<Value, String> {
        let release = params.get("release").ok_or("feature.plugin.install requires release")?;
        let plugin_id = release
            .get("pluginId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("plugin release is missing pluginId")?
            .to_string();
        if self.mode == AndroidHostMode::Test {
            self.installed_plugins.insert(plugin_id.clone());
            return Ok(json!({
                "pluginId":plugin_id,
                "runtime":"deepseek-js",
                "requestedPermissions":[],
                "fixture":true
            }));
        }

        let manifest: ExternalReleaseManifest = serde_json::from_value(release.clone())
            .map_err(|error| format!("plugin release manifest is invalid: {error}"))?;
        let pointer = self
            .plugin_installer
            .install(
                &manifest,
                "android",
                &["deepseek-js", "javascript", "cordis-js", "mcp", "wasm"],
            )
            .map_err(|error| format!("verified plugin installation failed: {error}"))?;
        self.plugin_permissions
            .retain_requested(&pointer.plugin_id, &pointer.requested_permissions)
            .map_err(|error| format!("failed to reconcile plugin permissions: {error}"))?;
        Ok(json!({
            "pluginId":pointer.plugin_id,
            "version":pointer.version,
            "artifactId":pointer.artifact_id,
            "artifactSha256":pointer.artifact_sha256,
            "runtime":pointer.runtime,
            "entry":pointer.entry,
            "requestedPermissions":pointer.requested_permissions,
            "installedPath":pointer.installed_path,
            "fixture":false
        }))
    }

    fn plugin_compatibility(&self, params: &Value) -> Result<Value, String> {
        let plugin_id = required_string(params, "pluginId")?;
        if self.mode == AndroidHostMode::Test && self.installed_plugins.contains(plugin_id) {
            return Ok(json!({
                "pluginId":plugin_id,
                "portableCompatible":true,
                "runtime":"deepseek-js",
                "fixture":true
            }));
        }
        let active = self
            .plugin_installer
            .active(plugin_id)
            .map_err(|error| format!("failed to inspect installed plugin: {error}"))?
            .ok_or("plugin is not installed")?;
        let portable = matches!(
            active.runtime.as_str(),
            "deepseek-js" | "javascript" | "cordis-js" | "mcp" | "wasm"
        );
        Ok(json!({
            "pluginId":active.plugin_id,
            "version":active.version,
            "runtime":active.runtime,
            "portableCompatible":portable,
            "requestedPermissions":active.requested_permissions,
        }))
    }

    fn plugin_permission_grant(&mut self, params: &Value) -> Result<Value, String> {
        let plugin_id = required_string(params, "pluginId")?;
        let permission = required_string(params, "permission")?;
        let active = self
            .plugin_installer
            .active(plugin_id)
            .map_err(|error| format!("failed to inspect installed plugin: {error}"))?
            .ok_or("plugin is not installed")?;
        self.plugin_permissions
            .grant(plugin_id, &active.requested_permissions, permission)
            .map_err(|error| format!("plugin permission grant rejected: {error}"))?;
        self.runtime_call_cancellations.clear_permission_block(plugin_id, permission);
        Ok(json!({
            "pluginId":plugin_id,
            "permission":permission,
            "granted":true,
            "grants":self.plugin_permissions.grants_for(plugin_id),
        }))
    }

    fn plugin_permission_revoke(&mut self, params: &Value) -> Result<Value, String> {
        let plugin_id = required_string(params, "pluginId")?;
        let permission = required_string(params, "permission")?;
        if self
            .plugin_installer
            .active(plugin_id)
            .map_err(|error| format!("failed to inspect installed plugin: {error}"))?
            .is_none()
        {
            return Err("plugin is not installed".into());
        }
        self.runtime_call_cancellations.signal_permission(plugin_id, permission);
        self.plugin_permissions
            .revoke(plugin_id, permission)
            .map_err(|error| format!("plugin permission revoke failed: {error}"))?;
        self.capability_broker.cancel_plugin(
            plugin_id,
            "capability grant revoked while call was pending",
            now_ms(),
        )?;
        self.runtime_call_cancellations.clear_permission_block(plugin_id, permission);
        Ok(json!({
            "pluginId":plugin_id,
            "permission":permission,
            "granted":false,
            "grants":self.plugin_permissions.grants_for(plugin_id),
        }))
    }

    fn runtime_start(&mut self, params: &Value) -> Result<Value, String> {
        let plugin_id = required_string(params, "pluginId")?.to_string();
        let config = params.get("config").cloned().unwrap_or_else(|| json!({}));
        if !config.is_object() {
            return Err("runtime.start config must be a JSON object".into());
        }
        let active = self
            .plugin_installer
            .active(&plugin_id)
            .map_err(|error| format!("failed to inspect installed plugin: {error}"))?
            .ok_or("plugin is not installed")?;
        if !matches!(
            active.runtime.as_str(),
            "deepseek-js" | "javascript" | "cordis-js"
        ) {
            return Err(format!(
                "runtime {} has no Android portable execution adapter",
                active.runtime
            ));
        }
        let entry = active
            .entry
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or("installed JS plugin release is missing entry")?;
        let root = PathBuf::from(&active.installed_path);
        let grants = self.plugin_permissions.grants_for(&plugin_id);
        if self.js_runtime.is_none() {
            self.js_runtime = Some(
                DeepSeekJsHost::new()
                    .map_err(|error| format!("failed to create portable JS runtime: {error}"))?,
            );
        }
        let runtime = self.js_runtime.as_mut().expect("JS runtime initialized");
        let state = if runtime.plugin_state(&plugin_id).is_some() {
            runtime
                .set_plugin_grants(&plugin_id, grants.iter().cloned())
                .map_err(|error| format!("failed to update plugin runtime grants: {error}"))?;
            runtime
                .enable_plugin(&plugin_id)
                .map_err(|error| format!("failed to start plugin runtime: {error}"))?
        } else {
            runtime
                .register_plugin_with_grants(
                    &plugin_id,
                    &root,
                    std::path::Path::new(entry),
                    &config,
                    &grants,
                )
                .map_err(|error| format!("failed to register plugin runtime: {error}"))?
        };
        self.sync_js_runtime_events()?;
        let owned_tools = self
            .js_runtime
            .as_ref()
            .ok_or("plugin runtime disappeared during start")?
            .registered_tools_for_plugin(&plugin_id)
            .map_err(|error| format!("failed to read plugin-scoped runtime tools: {error}"))?
            .into_iter()
            .collect::<BTreeSet<_>>();
        if owned_tools.is_empty() {
            self.runtime_tools.remove(&plugin_id);
        } else {
            self.runtime_tools.insert(plugin_id.clone(), owned_tools);
        }
        let generation = self.runtime_generations.entry(plugin_id.clone()).or_insert(0);
        *generation = generation.saturating_add(1);
        self.runtime_call_cancellations.clear_plugin_block(&plugin_id);
        Ok(json!({
            "pluginId":plugin_id,
            "runtime":active.runtime,
            "generation":*generation,
            "state":serde_json::to_value(state).unwrap_or(Value::String(format!("{state:?}"))),
            "tools":self.runtime_tools.get(&plugin_id).cloned().unwrap_or_default(),
        }))
    }

    fn runtime_stop(&mut self, params: &Value) -> Result<Value, String> {
        let plugin_id = required_string(params, "pluginId")?.to_string();
        self.runtime_call_cancellations.signal_plugin(&plugin_id);
        self.capability_broker.cancel_plugin(&plugin_id, "runtime stopped", now_ms())?;
        let generation = self.runtime_generations.entry(plugin_id.clone()).or_insert(0);
        *generation = generation.saturating_add(1);
        let runtime = self
            .js_runtime
            .as_mut()
            .ok_or("plugin runtime is not started")?;
        runtime
            .disable_plugin(&plugin_id)
            .map_err(|error| format!("failed to stop plugin runtime: {error}"))?;
        self.sync_js_runtime_events()?;
        self.runtime_tools.remove(&plugin_id);
        Ok(json!({"pluginId":plugin_id,"state":"DISPOSED"}))
    }

    fn runtime_tools(&mut self, params: &Value) -> Result<Value, String> {
        let plugin_id = required_string(params, "pluginId")?.to_string();
        if self
            .js_runtime
            .as_ref()
            .and_then(|runtime| runtime.plugin_state(&plugin_id))
            .is_none()
        {
            return Err("plugin runtime is not started".into());
        }
        self.sync_js_runtime_events()?;
        let owned_tools = self
            .js_runtime
            .as_ref()
            .ok_or("plugin runtime is not started")?
            .registered_tools_for_plugin(&plugin_id)
            .map_err(|error| format!("failed to read plugin-scoped runtime tools: {error}"))?
            .into_iter()
            .collect::<BTreeSet<_>>();
        if owned_tools.is_empty() {
            self.runtime_tools.remove(&plugin_id);
        } else {
            self.runtime_tools.insert(plugin_id.clone(), owned_tools);
        }
        Ok(json!({
            "pluginId":plugin_id,
            "tools":self.runtime_tools.get(&plugin_id).cloned().unwrap_or_default(),
        }))
    }


    fn runtime_cancel(&mut self, params: &Value) -> Result<Value, String> {
        let request_id = required_string(params, "requestId")?;
        self.runtime_call_cancellations.signal_request(request_id);
        let cancelled = self.capability_broker.cancel_request(
            request_id,
            params.get("reason").and_then(Value::as_str).unwrap_or("runtime call cancelled"),
            now_ms(),
        )?;
        Ok(json!({"requestId":request_id,"cancelled":cancelled}))
    }

    fn runtime_call(&mut self, params: &Value) -> Result<Value, String> {
        let plugin_id = required_string(params, "pluginId")?.to_string();
        let tool = required_string(params, "tool")?.to_string();
        let request_id = required_string(params, "requestId")?.to_string();
        let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        if !arguments.is_object() {
            return Err("runtime.call arguments must be a JSON object".into());
        }
        if request_id.len() > 256 || tool.len() > 256 {
            return Err("runtime.call identity exceeds bounded length".into());
        }
        if self.capability_broker.needs_reconciliation(&request_id) {
            return Err("runtime.call has outcome-unknown state from a prior Host lifetime; reconcile before replay".into());
        }
        let active = self.plugin_installer.active(&plugin_id)
            .map_err(|error| format!("failed to inspect installed plugin: {error}"))?
            .ok_or("plugin is not installed")?;
        let grants = self.plugin_permissions.grants_for(&plugin_id);
        let capability = format!("plugin.{plugin_id}.tool.{tool}");
        let required_permissions = active
            .requested_permissions
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        if self.runtime_call_cancellations.is_blocked(&plugin_id, &required_permissions) {
            return Err("runtime.call is fenced by a pending stop or permission revocation".into());
        }
        let all_declared_granted = required_permissions
            .iter()
            .all(|permission| grants.contains(permission));
        let generation = *self.runtime_generations.get(&plugin_id).ok_or("plugin runtime is not started")?;
        let registered = self.runtime_tools.get(&plugin_id).is_some_and(|tools| tools.contains(&tool));
        if !registered { return Err("runtime.call tool is not registered by this plugin instance".into()); }
        let account_fence = self.current_turn_account_fence()?;
        match self.capability_broker.authorize(
            &request_id,&plugin_id,&capability,&tool,&account_fence,generation,
            true,all_declared_granted,now_ms())? {
            CapabilityDecision::Allow => {}
            CapabilityDecision::NeedsUser => return Err("runtime.call requires all current immutable-release permission grants".into()),
            CapabilityDecision::Deny => return Err("runtime.call denied by Capability Broker".into()),
        }
        let timeout_ms=params.get("timeoutMs").and_then(Value::as_u64).unwrap_or(15_000).clamp(100,30_000);
        let started=now_ms();
        self.capability_broker.begin(PendingCapabilityCall{
            request_id:request_id.clone(),plugin_id:plugin_id.clone(),capability:capability.clone(),
            tool:tool.clone(),arguments:arguments.clone(),required_permissions:required_permissions.clone(),
            account_fence:account_fence.clone(),
            runtime_generation:generation,started_at_ms:started,deadline_at_ms:started.saturating_add(timeout_ms),
            state:"pending".into(),
        })?;
        if let Err(error) = self
            .capability_broker
            .assert_current(&request_id, &plugin_id, &account_fence, generation, now_ms())
        {
            self.capability_broker.settle(
                &request_id,
                "failed",
                Some(format!(
                    "runtime.call rejected before tool dispatch; no side effect started: {error}"
                )),
                now_ms(),
            )?;
            return Err(error);
        }
        let runtime = self.js_runtime.as_ref().ok_or("plugin runtime is not started")?;
        let cancellation = match self.runtime_call_cancellations.register(
            &request_id,
            &plugin_id,
            required_permissions.clone(),
        ) {
            Ok(token) => token,
            Err(error) => {
                let _ = self.capability_broker.cancel_request(
                    &request_id,
                    "runtime cancellation/control registration failed",
                    now_ms(),
                );
                return Err(error);
            }
        };
        let mut result = runtime.call_plugin_tool_json_bounded(
            &plugin_id,
            &tool,
            &arguments,
            Duration::from_millis(timeout_ms),
            cancellation.as_ref(),
        );
        if cancellation.load(Ordering::Acquire) && result.is_ok() {
            result = Err(mahayana_js_runtime::JsRuntimeError::Cancelled);
        }
        self.runtime_call_cancellations.complete(&request_id);
        let finished=now_ms();

        let active_after = self.plugin_installer.active(&plugin_id)
            .map_err(|error| format!("failed to recheck installed plugin after runtime.call: {error}"))?;
        let release_unchanged = active_after.as_ref().is_some_and(|current| {
            current.version == active.version
                && current.artifact_id == active.artifact_id
                && current.artifact_sha256 == active.artifact_sha256
                && current.installed_path == active.installed_path
        });
        let grants_after = self.plugin_permissions.grants_for(&plugin_id);
        let grant_still_present = required_permissions
            .iter()
            .all(|permission| grants_after.contains(permission));
        let current_generation = self.runtime_generations.get(&plugin_id).copied();
        let current_account = self.current_turn_account_fence();
        if !release_unchanged
            || !grant_still_present
            || current_generation != Some(generation)
            || current_account.as_deref() != Ok(account_fence.as_str())
        {
            if self.capability_broker.assert_current(
                &request_id, &plugin_id, &account_fence, generation, finished
            ).is_ok() {
                self.capability_broker.settle(
                    &request_id,
                    "outcome_unknown",
                    Some("installed release, grant, account fence, or runtime generation changed during execution".into()),
                    finished,
                )?;
            }
            return Err("runtime.call result fenced by changed release/grant/account/runtime state; reconciliation required".into());
        }

        match result {
            Err(mahayana_js_runtime::JsRuntimeError::TimedOut) => {
                if !self.capability_broker.needs_reconciliation(&request_id) {
                    self.capability_broker.settle(
                        &request_id,
                        "outcome_unknown",
                        Some("bounded runtime deadline expired after tool dispatch; reconcile side effects before replay".into()),
                        finished,
                    )?;
                }
                return Err("runtime.call timed out with outcome unknown; reconciliation required".into());
            }
            Err(mahayana_js_runtime::JsRuntimeError::Cancelled) => {
                let _ = self.capability_broker.cancel_request(
                    &request_id,
                    "runtime adapter cancelled",
                    finished,
                )?;
                return Err("runtime.call cancelled; reconciliation may be required".into());
            }
            other => result = other,
        }

        if let Err(error)=self.capability_broker.assert_current(&request_id,&plugin_id,&account_fence,generation,finished) {
            return Err(error);
        }
        match result {
            Ok(value) => {
                self.capability_broker.settle(&request_id,"completed",None,finished)?;
                Ok(json!({"requestId":request_id,"pluginId":plugin_id,"tool":tool,"result":value,"generation":generation}))
            }
            Err(error) => {
                self.capability_broker.settle(&request_id,"failed",Some(error.to_string()),finished)?;
                Err(format!("runtime.call failed: {error}"))
            }
        }
    }

    fn sync_js_runtime_events(&mut self) -> Result<(), String> {
        let Some(runtime) = self.js_runtime.as_ref() else {
            return Ok(());
        };
        let events = runtime
            .drain_events()
            .map_err(|error| format!("failed to drain plugin runtime events: {error}"))?;
        for event in events {
            match event {
                HostEvent::ToolRegistered { plugin_id, tool, .. } => {
                    self.runtime_tools.entry(plugin_id).or_default().insert(tool);
                }
                HostEvent::ToolUnregistered { plugin_id, tool } => {
                    let remove_entry = self
                        .runtime_tools
                        .get_mut(&plugin_id)
                        .is_some_and(|tools| {
                            tools.remove(&tool);
                            tools.is_empty()
                        });
                    if remove_entry {
                        self.runtime_tools.remove(&plugin_id);
                    }
                }
                HostEvent::ServiceRegistered { .. } | HostEvent::ServiceUnregistered { .. } => {}
            }
        }
        Ok(())
    }

    fn plugin_ui_document(&self, params: &Value) -> Result<Value, String> {
        let plugin_id = required_string(params, "pluginId")?;
        if self.mode != AndroidHostMode::Test {
            return Err(
                "feature.plugin.uiDocument is unavailable until the installed package UI document is verified and loaded from the canonical plugin store; Android refuses placeholder HTML"
                    .into(),
            );
        }
        if !self.installed_plugins.contains(plugin_id) {
            return Err("test plugin is not installed".into());
        }
        Ok(json!({
            "pluginId":plugin_id,
            "html":"<!doctype html><html><body><main id=\"app\">Fabushi Mini App test fixture</main></body></html>",
            "fixture":true
        }))
    }
}



fn mcp_auth_poll_settlement_json(settlement: McpAuthPollSettlement) -> Value {
    match settlement {
        McpAuthPollSettlement::Pending => json!({"status":"pending"}),
        McpAuthPollSettlement::Stale => json!({"status":"stale"}),
        McpAuthPollSettlement::Completed(completion) => json!({
            "status":"completed",
            "generation":completion.generation,
            "serverId":completion.server_id,
            "serverName":completion.server_name,
            "accountKey":completion.account_key,
            "requestingAgentId":completion.requesting_agent_id,
        }),
        McpAuthPollSettlement::Cancelled(completion) => json!({
            "status":"cancelled",
            "generation":completion.generation,
            "serverId":completion.server_id,
            "serverName":completion.server_name,
            "accountKey":completion.account_key,
            "requestingAgentId":completion.requesting_agent_id,
        }),
    }
}

fn mcp_auth_display_name(server_name: &str, account_key: &str) -> String {
    if account_key == "default" {
        return server_name.to_string();
    }
    let inert = account_key
        .chars()
        .filter(|character| {
            let code = *character as u32;
            let hostile_ascii = matches!(
                *character,
                '"' | '\'' | '`' | '\\' | '[' | ']' | '{' | '}' | '(' | ')' | '<' | '>'
            );
            !(matches!(code, 0x00..=0x1f | 0x7f | 0x2028 | 0x2029) || hostile_ascii)
        })
        .collect::<String>();
    let collapsed = inert.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut rendered = String::new();
    let mut utf16_units = 0usize;
    for character in collapsed.chars() {
        let next = character.len_utf16();
        if utf16_units + next > 64 {
            break;
        }
        rendered.push(character);
        utf16_units += next;
    }
    format!("{server_name} ({rendered})")
}

fn with_account_session_mutation(
    mut result: Value,
    mutation: Option<AccountSessionMutation>,
) -> Value {
    if let Some(mutation) = mutation {
        if let Some(object) = result.as_object_mut() {
            object.insert(
                "_accountSessionMutation".into(),
                mutation.as_private_projection(),
            );
        }
    }
    result
}

fn project_fabushi_sand_access(logged_in: bool) -> Value {
    if logged_in {
        json!({"state":"granted","reason":"none"})
    } else {
        json!({"state":"unknown","reason":"unspecified"})
    }
}

fn push_turn_event(events: &Arc<Mutex<VecDeque<Value>>>, event: Value) {
    if let Ok(mut queue) = events.lock() {
        queue.push_back(event);
    }
}

impl Drop for AndroidJsonHost {
    fn drop(&mut self) {
        for cancelled in self.turn_cancellations.values() {
            cancelled.store(true, Ordering::Release);
        }
        self.turn_cancellations.clear();
    }
}

fn required_u64(value: &Value, key: &str) -> Result<u64, String> {
    value.get(key).and_then(Value::as_u64).ok_or_else(|| format!("{key} is required"))
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn webauthn_bridge_error_message(error: WebAuthnBridgeError) -> String {
    match error {
        WebAuthnBridgeError::NoProvider { message }
        | WebAuthnBridgeError::ProviderStale { message } => message.to_string(),
        WebAuthnBridgeError::DispatchFailed(message) => message,
        WebAuthnBridgeError::UnknownRequest => "unknown WebAuthn request".into(),
        WebAuthnBridgeError::TimedOut => "WebAuthn request timed out".into(),
    }
}

fn webauthn_request_frame_json(frame: WebAuthnRequestFrame) -> Value {
    match frame {
        WebAuthnRequestFrame::Welcome { provider_id } => {
            json!({"kind":"welcome","providerId":provider_id})
        }
        WebAuthnRequestFrame::Ceremony { request_id, ceremony } => json!({
            "kind":"ceremony",
            "requestId":request_id,
            "ceremony":{
                "kind":ceremony.kind,
                "origin":ceremony.origin,
                "payloadJson":ceremony.payload_json,
            }
        }),
        WebAuthnRequestFrame::Cancel { request_id } => {
            json!({"kind":"cancel","requestId":request_id})
        }
    }
}

fn parse_webauthn_response_frame(value: &Value) -> Result<WebAuthnResponseFrame, String> {
    let kind = required_string(value, "kind")?;
    match kind {
        "hello" => Ok(WebAuthnResponseFrame::Hello {
            computer_id: value.get("computerId").and_then(Value::as_str).map(str::to_string),
            label: value.get("label").and_then(Value::as_str).map(str::to_string),
        }),
        "ping" => Ok(WebAuthnResponseFrame::Ping),
        "stage" => {
            let request_id = required_string(value, "requestId")?.to_string();
            let stage = match required_string(value, "stage")? {
                "grant" => WebAuthnStage::Grant,
                "sign" => WebAuthnStage::Sign,
                _ => return Err("stage must be grant or sign".into()),
            };
            let outcome = match required_string(value, "outcome")? {
                "ok" => WebAuthnStageOutcome::Ok,
                "declined" => WebAuthnStageOutcome::Declined,
                "failed" => WebAuthnStageOutcome::Failed,
                _ => return Err("outcome must be ok, declined, or failed".into()),
            };
            Ok(WebAuthnResponseFrame::Stage {
                request_id,
                stage,
                outcome,
            })
        }
        "result" => Ok(WebAuthnResponseFrame::Result {
            request_id: required_string(value, "requestId")?.to_string(),
            credential_json: required_string(value, "credentialJson")?.to_string(),
        }),
        "error" => Ok(WebAuthnResponseFrame::Error {
            request_id: required_string(value, "requestId")?.to_string(),
            name: required_string(value, "name")?.to_string(),
            message: required_string(value, "message")?.to_string(),
            code: value.get("code").and_then(Value::as_str).map(str::to_string),
        }),
        _ => Err(format!("unsupported WebAuthn response frame kind {kind}")),
    }
}

fn latest_visible_assistant_message_id(entries: &[Value]) -> Option<String> {
    let visible_operation_ids = entries
        .iter()
        .filter(|entry| {
            entry.get("kind").and_then(Value::as_str) == Some("message")
                && entry.get("role").and_then(Value::as_str) == Some("user")
        })
        .filter_map(|entry| entry.get("operationId").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();

    entries
        .iter()
        .rev()
        .find(|entry| {
            entry.get("kind").and_then(Value::as_str) == Some("message")
                && entry.get("role").and_then(Value::as_str) == Some("assistant")
                && entry
                    .get("operationId")
                    .and_then(Value::as_str)
                    .is_some_and(|operation_id| visible_operation_ids.contains(operation_id))
        })
        .and_then(|entry| entry.get("id").and_then(Value::as_str))
        .map(str::to_string)
}

fn assistant_projection_from_entries(entries: &[Value]) -> Value {
    let latest_message_id = latest_visible_assistant_message_id(entries);
    let last_read_message_id = entries
        .iter()
        .find(|entry| entry.get("id").and_then(Value::as_str) == Some(ASSISTANT_READ_MARKER_ID))
        .and_then(|entry| entry.get("lastReadMessageId"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let has_unread = latest_message_id.is_some() && latest_message_id != last_read_message_id;
    json!({
        "agentId": "mahayana-assistant",
        "latestMessageId": latest_message_id,
        "lastReadMessageId": last_read_message_id,
        "hasUnread": has_unread,
    })
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{key} is required"))
}

fn validate_platform_api_path(path: &str) -> Result<(), String> {
    if path.len() > 4_096
        || !path.starts_with("/v1/")
        || path.contains(['\r', '\n', '#', '\\'])
    {
        return Err("platform.request path must be a bounded /v1/* Fabushi API path".into());
    }
    let path_only = path.split('?').next().unwrap_or(path);
    if path_only
        .split('/')
        .any(|segment| matches!(segment, "." | ".."))
    {
        return Err("platform.request path traversal is forbidden".into());
    }
    let lower = path_only.to_ascii_lowercase();
    if lower.contains("%2e") || lower.contains("%2f") || lower.contains("%5c") {
        return Err("platform.request encoded path traversal is forbidden".into());
    }
    Ok(())
}

fn encode_api_path_segment(value: &str) -> Result<String, String> {
    if value.is_empty()
        || value.len() > 200
        || value.chars().any(|character| character.is_control())
    {
        return Err("Fabushi API path identifier is invalid".into());
    }
    Ok(url::form_urlencoded::byte_serialize(value.as_bytes()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fabushi_shipping_sand_access_is_owned_by_authenticated_product_policy() {
        assert_eq!(
            project_fabushi_sand_access(true),
            json!({"state":"granted","reason":"none"})
        );
        assert_eq!(
            project_fabushi_sand_access(false),
            json!({"state":"unknown","reason":"unspecified"})
        );
    }

    #[test]
    fn subagent_review_expiry_policy_matches_desktop_turn_contract() {
        assert_eq!(
            subagent_review_approval_expiry_policy(Some("turn")),
            SubagentReviewApprovalExpiryPolicy::Park
        );
        assert_eq!(
            subagent_review_approval_expiry_policy(Some("handoff-resume")),
            SubagentReviewApprovalExpiryPolicy::Park
        );
        assert_eq!(
            subagent_review_approval_expiry_policy(Some("background")),
            SubagentReviewApprovalExpiryPolicy::Ttl
        );
        assert_eq!(
            subagent_review_approval_expiry_policy(None),
            SubagentReviewApprovalExpiryPolicy::Ttl
        );
    }

    #[test]
    fn subagent_review_block_waits_for_durable_broker_approval_and_consumes_once() {
        let app_data = tempfile::tempdir().unwrap();
        let broker = SharedCapabilityBroker::open(
            app_data.path().join("review-capabilities.json"),
            now_ms(),
        )
        .unwrap();
        let registry = Arc::new(SubagentReviewApprovalRegistry::default());
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_broker = broker.clone();
        let worker_registry = Arc::clone(&registry);
        let worker_events = Arc::clone(&events);
        let worker_cancelled = Arc::clone(&cancelled);
        let worker = thread::spawn(move || {
            wait_for_subagent_review_approval(
                &worker_broker,
                &worker_registry,
                &worker_events,
                &worker_cancelled,
                "agent-parent",
                "parent-request",
                "session:account-a",
                7,
                "tool-call-1",
                "launch",
                "sensitive task",
                None,
                Some("executor"),
                "manual approval required",
                Some("allow this task"),
                SubagentReviewApprovalExpiryPolicy::Ttl,
            )
        });

        let approval = loop {
            if let Some(event) = events.lock().unwrap().pop_front() {
                break event;
            }
            thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(approval["type"], "approval.requested");
        assert_eq!(approval["capability"], "agent.subagent.review");
        assert_eq!(approval["autoReview"], true);
        let approval_id = approval["approvalId"].as_str().unwrap().to_string();
        let resolved = broker
            .resolve_approval(
                &approval_id,
                true,
                "session:account-a",
                now_ms(),
            )
            .unwrap();
        assert_eq!(resolved.state, "allowed_once");
        registry.resolve(&approval_id, true).unwrap();
        assert_eq!(worker.join().unwrap().unwrap(), true);
        assert!(
            broker
                .resolve_approval(
                    &approval_id,
                    true,
                    "session:account-a",
                    now_ms(),
                )
                .is_err(),
            "one-time auto-review approval must not be reusable"
        );
    }

    #[test]
    fn subagent_review_approval_denial_cancel_and_pending_bound_fail_closed() {
        let registry = SubagentReviewApprovalRegistry::default();
        let mut signals = Vec::new();
        for index in 0..SUBAGENT_REVIEW_MAX_PENDING_PER_AGENT {
            signals.push(
                registry
                    .register(&format!("approval-{index}"), "agent-parent")
                    .unwrap(),
            );
        }
        assert!(
            registry
                .register("approval-overflow", "agent-parent")
                .unwrap_err()
                .contains("too many pending")
        );
        registry.resolve("approval-0", false).unwrap();
        let (state, _) = &*signals[0];
        assert_eq!(*state.lock().unwrap(), Some(false));
        for index in 0..SUBAGENT_REVIEW_MAX_PENDING_PER_AGENT {
            registry.remove(&format!("approval-{index}"));
        }

        let app_data = tempfile::tempdir().unwrap();
        let broker = SharedCapabilityBroker::open(
            app_data.path().join("review-cancel.json"),
            now_ms(),
        )
        .unwrap();
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let cancelled = Arc::new(AtomicBool::new(true));
        let result = wait_for_subagent_review_approval(
            &broker,
            &registry,
            &events,
            &cancelled,
            "agent-parent",
            "parent-request",
            "session:account-a",
            8,
            "tool-call-cancelled",
            "steer",
            "cancelled steer",
            Some("generated:child"),
            None,
            "manual approval required",
            None,
            SubagentReviewApprovalExpiryPolicy::Ttl,
        );
        assert!(result.unwrap_err().contains("cancelled"));
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn pending_subagent_review_restart_is_outcome_unknown_and_old_approval_is_stale() {
        let root = tempfile::tempdir().unwrap();
        let mut first = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let accepted = first
            .dispatch(
                "feature.execute",
                &json!({"command":{
                    "type":"chat.send",
                    "requestId":"restart-pending-review",
                    "agentId":"mahayana-assistant",
                    "requestSource":"turn",
                    "text":"[[tool:Task]]{\"prompt\":\"[[review:block]] hold before launch\",\"subagent_type\":\"general-purpose\"}"
                }}),
            )
            .expect("root turn accepted");
        let operation_id = accepted["operationId"].as_str().unwrap().to_string();

        let approval = (0..80)
            .find_map(|_| {
                let event = first.dispatch("feature.receive", &json!({})).ok()?;
                if event["type"] == "approval.requested"
                    && event["capability"] == "agent.subagent.review"
                {
                    Some(event)
                } else {
                    thread::sleep(Duration::from_millis(5));
                    None
                }
            })
            .expect("subagent auto-review approval requested");
        let approval_id = approval["approvalId"].as_str().unwrap().to_string();
        let approval_operation_id = approval["operationId"].as_str().unwrap().to_string();
        assert_ne!(approval_operation_id, operation_id);
        assert!(
            approval_operation_id.starts_with("subagent-review-operation-"),
            "review approval must retain its own stable side-effect operation identity"
        );
        assert!(approval["expiresAtMs"].is_null(), "root turn approvals must be parked");

        let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let recovered = reopened
            .turn_journal
            .lock()
            .unwrap()
            .record("restart-pending-review")
            .cloned()
            .expect("durable root turn survives process restart");
        assert_eq!(recovered.state, DurableTurnState::OutcomeUnknown);
        assert_eq!(recovered.operation_id, operation_id);

        let stale = reopened.dispatch(
            "feature.approval.resolve",
            &json!({"approvalId":approval_id,"approved":true}),
        );
        assert!(stale.is_err(), "pre-restart approval must never be reusable");

        let children = reopened
            .subagent_owner
            .lock()
            .unwrap()
            .all_records()
            .into_iter()
            .filter(|record| {
                record.parent_agent_id == "mahayana-assistant"
                    && (record.lineage.parent_request_id.as_deref()
                        == Some("restart-pending-review")
                        || record.lineage.root_parent_request_id.as_deref()
                            == Some("restart-pending-review"))
            })
            .collect::<Vec<_>>();
        assert!(
            children.is_empty(),
            "blocked Task must not launch before a fresh reconciled approval"
        );

        let replay = reopened.dispatch(
            "feature.execute",
            &json!({"command":{
                "type":"chat.send",
                "requestId":"restart-pending-review",
                "agentId":"mahayana-assistant",
                    "requestSource":"turn",
                "text":"[[tool:Task]]{\"prompt\":\"[[review:block]] hold before launch\",\"subagent_type\":\"general-purpose\"}"
            }}),
        );
        assert!(
            replay.is_err(),
            "outcome-unknown root turn must reconcile before any blind replay"
        );

        drop(first);
    }

    #[test]
    fn deterministic_test_journey_covers_auth_stream_approval_and_interrupt() {
        let app_data = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(app_data.path(), AndroidHostMode::Test);
        assert_eq!(host.dispatch("feature.info", &json!({})).unwrap()["platform"], "android");
        assert!(host.dispatch("feature.auth.providers", &json!({})).unwrap().as_array().unwrap().iter().any(|p| p["id"] == "google"));

        let oauth = host.dispatch("feature.auth.oauthStart", &json!({"provider":"google"})).unwrap();
        let completed = host.dispatch("feature.auth.oauthPoll", &json!({"attemptId":oauth["attemptId"]})).unwrap();
        assert_eq!(completed["status"], "completed");

        let accepted = host.dispatch("feature.execute", &json!({"command":{
            "type":"capability.request",
            "requestId":"capability-1",
            "capability":"camera"
        }})).unwrap();
        assert_eq!(accepted["requestId"], "capability-1");
        let mut saw_approval = false;
        for _ in 0..8 {
            let event = host.dispatch("feature.receive", &json!({})).unwrap();
            if event["type"] == "approval.requested" {
                saw_approval = true;
                break;
            }
        }
        assert!(saw_approval);
        let approval = host
            .dispatch(
                "feature.approval.resolve",
                &json!({"approvalId":format!("approval-{}", accepted["operationId"].as_str().unwrap()),"approved":true}),
            )
            .unwrap();
        assert_eq!(approval["execution"], "authorized");
        assert_eq!(approval["capability"], "camera");
        assert!(host
            .dispatch(
                "feature.approval.resolve",
                &json!({"approvalId":format!("approval-{}", accepted["operationId"].as_str().unwrap()),"approved":true}),
            )
            .is_err());

        let denied_request = host.dispatch("feature.execute", &json!({"command":{
            "type":"capability.request",
            "requestId":"capability-2",
            "capability":"microphone"
        }})).unwrap();
        let denied = host.dispatch(
            "feature.approval.resolve",
            &json!({
                "approvalId":format!("approval-{}", denied_request["operationId"].as_str().unwrap()),
                "approved":false
            }),
        ).unwrap();
        assert_eq!(denied["execution"], "denied");
        assert_eq!(denied["capability"], "microphone");

        let cancelled_request = host.dispatch("feature.execute", &json!({"command":{
            "type":"capability.request",
            "requestId":"capability-3",
            "capability":"location"
        }})).unwrap();
        let cancelled_operation = cancelled_request["operationId"].as_str().unwrap();
        host.dispatch("feature.interrupt", &json!({"operationId":cancelled_operation})).unwrap();
        assert!(host.dispatch(
            "feature.approval.resolve",
            &json!({
                "approvalId":format!("approval-{cancelled_operation}"),
                "approved":true
            }),
        ).is_err());

        let long_task = host.dispatch("feature.execute", &json!({"command":{
            "type":"runtime.longTask",
            "requestId":"long-1"
        }})).unwrap();
        let operation_id = long_task["operationId"].as_str().unwrap();
        let interrupted = host.dispatch("feature.interrupt", &json!({"operationId":operation_id})).unwrap();
        assert_eq!(interrupted["status"], "interrupted");
    }

    #[test]
    fn webauthn_provider_transport_queues_ceremony_and_settles_result_once() {
        let mut host = AndroidJsonHost::new("/tmp/fabushi-host-webauthn", AndroidHostMode::Test);
        let registered = host
            .dispatch("feature.webauthn.registerProvider", &json!({}))
            .unwrap();
        let provider_id = registered["providerId"].as_str().unwrap().to_string();

        let welcome = host
            .dispatch(
                "feature.webauthn.pollRequest",
                &json!({"providerId":provider_id}),
            )
            .unwrap();
        assert_eq!(welcome["frame"]["kind"], "welcome");

        host.dispatch(
            "feature.webauthn.submitResponses",
            &json!({"providerId":provider_id,"frames":[{"kind":"ping"}]}),
        )
        .unwrap();

        let requested = host
            .dispatch(
                "feature.webauthn.requestCeremony",
                &json!({
                    "kind":"get",
                    "origin":"https://cursor.com",
                    "payloadJson":"{\"challenge\":\"abc\"}"
                }),
            )
            .unwrap();
        let request_id = requested["requestId"].as_str().unwrap().to_string();

        let ceremony = host
            .dispatch(
                "feature.webauthn.pollRequest",
                &json!({"providerId":provider_id}),
            )
            .unwrap();
        assert_eq!(ceremony["frame"]["kind"], "ceremony");
        assert_eq!(ceremony["frame"]["requestId"], request_id);

        let settled = host
            .dispatch(
                "feature.webauthn.submitResponses",
                &json!({
                    "providerId":provider_id,
                    "frames":[{
                        "kind":"result",
                        "requestId":request_id,
                        "credentialJson":"{\"id\":\"cred-1\"}"
                    }]
                }),
            )
            .unwrap();
        assert_eq!(settled["settlements"].as_array().unwrap().len(), 1);

        let duplicate = host
            .dispatch(
                "feature.webauthn.submitResponses",
                &json!({
                    "providerId":provider_id,
                    "frames":[{
                        "kind":"result",
                        "requestId":request_id,
                        "credentialJson":"{}"
                    }]
                }),
            )
            .unwrap();
        assert!(duplicate["settlements"].as_array().unwrap().is_empty());
    }

    #[test]
    fn production_unknown_methods_fail_closed() {
        let mut host = AndroidJsonHost::new("/tmp/fabushi-host-prod", AndroidHostMode::Production);
        assert!(host.dispatch("arbitrary.renderer.method", &json!({})).is_err());
    }
    #[test]
    fn canonical_agent_roster_drives_direct_and_bot_surfaces() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-json-host-roster-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);

        let created = host.dispatch("createAgent", &json!({
            "name":"First Agent",
            "description":"one",
            "origin":"user"
        })).unwrap();
        let id = created["agent"]["id"].as_str().unwrap().to_string();

        host.dispatch("updateAgent", &json!({
            "id":id,
            "profile":{"name":"Renamed Agent","description":"two"}
        })).unwrap();
        host.dispatch("setAgentHiddenFromSidebar", &json!({"id":id,"isHidden":true})).unwrap();
        host.dispatch("setAgentUnread", &json!({"id":id,"isUnread":true})).unwrap();
        host.dispatch("setPinnedAgents", &json!({"ids":[id]})).unwrap();

        let list = host.dispatch("listAgents", &json!({})).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["name"], "Renamed Agent");
        assert_eq!(list[0]["isHiddenFromSidebar"], true);
        assert_eq!(list[0]["hasUnread"], true);
        assert_eq!(list[0]["isPinned"], true);

        host.dispatch("feature.execute", &json!({"command":{
            "type":"bot.list",
            "requestId":"bot-list-1"
        }})).unwrap();
        let mut listed = None;
        for _ in 0..4 {
            let event = host.dispatch("feature.receive", &json!({})).unwrap();
            if event["type"] == "bot.listed" {
                listed = Some(event);
                break;
            }
        }
        let listed = listed.expect("bot.listed event");
        assert_eq!(listed["bots"][0]["id"], id);

        let duplicate = host.dispatch("duplicateAgent", &json!({"id":id})).unwrap();
        let duplicate_id = duplicate["agent"]["id"].as_str().unwrap().to_string();
        assert_ne!(duplicate_id, id);
        assert_eq!(host.dispatch("countAgents", &json!({})).unwrap(), 2);

        host.dispatch("deleteAgents", &json!({"ids":[id]})).unwrap();
        assert_eq!(host.dispatch("countAgents", &json!({})).unwrap(), 1);

        drop(host);
        let reopened = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let reopened_list = reopened
            .agents
            .lock()
            .unwrap()
            .list();
        assert_eq!(reopened_list.len(), 1);
        assert_eq!(reopened_list[0].id, duplicate_id);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn transcript_survives_host_reopen_and_dedupes_settled_send() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-json-host-transcript-{}-{}",
            std::process::id(),
            now_ms()
        ));

        {
            let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
            let accepted = host
                .dispatch(
                    "feature.execute",
                    &json!({"command":{
                        "type":"chat.send",
                        "requestId":"recover-request-1",
                        "text":"hello after restart",
                        "agentId":"mahayana-assistant"
                    }}),
                )
                .unwrap();
            assert_eq!(accepted["operationId"], "recover-request-1");

            let mut saw_delta = false;
            let mut completed = false;
            for _ in 0..16 {
                let event = host.dispatch("feature.receive", &json!({})).unwrap();
                if event["type"] == "chat.delta" {
                    saw_delta = true;
                }
                if event["type"] == "operation.completed" {
                    completed = true;
                    break;
                }
            }
            assert!(saw_delta);
            assert!(completed);
            let snapshot = host
                .dispatch("feature.transcript.snapshot", &json!({}))
                .unwrap();
            let entries = snapshot.as_array().unwrap();
            assert_eq!(entries.len(), 2);
            assert!(
                entries.iter().all(|entry| entry["agentId"] == "mahayana-assistant"),
                "new canonical transcript entries must persist their owning Agent identity"
            );
        }

        {
            let mut reopened = AndroidJsonHost::new(&root, AndroidHostMode::Test);
            let snapshot = reopened
                .dispatch("feature.transcript.snapshot", &json!({}))
                .unwrap();
            let reopened_entries = snapshot.as_array().unwrap();
            assert_eq!(reopened_entries.len(), 2);
            assert!(
                reopened_entries.iter().all(|entry| entry["agentId"] == "mahayana-assistant"),
                "Agent identity must survive Host reopen with the canonical transcript"
            );

            reopened
                .dispatch(
                    "feature.execute",
                    &json!({"command":{
                        "type":"chat.send",
                        "requestId":"recover-request-1",
                        "text":"hello after restart",
                        "agentId":"mahayana-assistant"
                    }}),
                )
                .unwrap();

            let mut recovered = false;
            let mut deduped = false;
            for _ in 0..16 {
                let event = reopened.dispatch("feature.receive", &json!({})).unwrap();
                if event["type"] == "chat.message" && event["recovered"] == true {
                    recovered = true;
                }
                if event["type"] == "operation.completed" && event["deduped"] == true {
                    deduped = true;
                    break;
                }
            }
            assert!(recovered);
            assert!(deduped);
            let snapshot = reopened
                .dispatch("feature.transcript.snapshot", &json!({}))
                .unwrap();
            assert_eq!(snapshot.as_array().unwrap().len(), 2);
        }

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn assistant_projection_refreshes_after_visible_completion_and_survives_reopen() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-assistant-projection-{}-{}",
            std::process::id(),
            now_ms()
        ));
        {
            let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
            host.transcript
                .lock()
                .unwrap()
                .append_entry(json!({
                    "id":"user-visible",
                    "kind":"message",
                    "role":"user",
                    "content":"hello",
                    "operationId":"operation-visible",
                    "timestampMs":1
                }))
                .unwrap();
            host.transcript
                .lock()
                .unwrap()
                .append_entry(json!({
                    "id":"assistant:operation-visible",
                    "kind":"message",
                    "role":"assistant",
                    "content":"reply",
                    "operationId":"operation-visible",
                    "timestampMs":2
                }))
                .unwrap();
            host.transcript
                .lock()
                .unwrap()
                .append_entry(json!({
                    "id":"assistant:hidden-operation",
                    "kind":"message",
                    "role":"assistant",
                    "content":"hidden continuation",
                    "operationId":"hidden-operation",
                    "timestampMs":3
                }))
                .unwrap();

            let unread = host.dispatch("feature.assistant.projection", &json!({})).unwrap();
            assert_eq!(unread["latestMessageId"], "assistant:operation-visible");
            assert_eq!(unread["hasUnread"], true);

            let read = host.dispatch("feature.assistant.markRead", &json!({})).unwrap();
            assert_eq!(read["lastReadMessageId"], "assistant:operation-visible");
            assert_eq!(read["hasUnread"], false);
        }

        let mut reopened = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let projection = reopened
            .dispatch("feature.assistant.projection", &json!({}))
            .unwrap();
        assert_eq!(projection["latestMessageId"], "assistant:operation-visible");
        assert_eq!(projection["lastReadMessageId"], "assistant:operation-visible");
        assert_eq!(projection["hasUnread"], false);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn hidden_mcp_auth_resume_does_not_create_synthetic_user_transcript_entry() {
        let app_data = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(app_data.path(), AndroidHostMode::Test);
        let completion = McpAuthWatchCompletion {
            generation: 7,
            server_id: "17".into(),
            server_name: "Calendar".into(),
            account_key: "work".into(),
            requesting_agent_id: Some("agent-a".into()),
            outcome: "completed",
        };
        host.queue_mcp_auth_resume_turn(&completion).unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let event = host.feature_receive().unwrap();
            if event.get("type").and_then(Value::as_str) == Some("operation.completed") {
                break;
            }
            if std::time::Instant::now() >= deadline {
                panic!("hidden MCP auth resume did not complete");
            }
            thread::sleep(Duration::from_millis(10));
        }

        let snapshot = host
            .dispatch("feature.transcript.snapshot", &json!({}))
            .unwrap();
        let entries = snapshot.as_array().unwrap();
        assert!(
            entries.iter().all(|entry| entry["role"] != "user"),
            "MCP auth resume must not append a fake user turn"
        );
        assert!(
            entries.iter().any(|entry| {
                entry["role"] == "assistant"
                    && entry["content"]
                        .as_str()
                        .is_some_and(|text| text.contains("自动化测试状态正常"))
            }),
            "requesting agent should still produce the assistant continuation"
        );
    }

    #[test]
    fn mcp_oauth_callback_keeps_watch_pending_until_token_validation_and_fences_stale_generation() {
        let app_data = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(app_data.path(), AndroidHostMode::Test);
        let registered = host.dispatch(
            "feature.mcp.authWatch.register",
            &json!({
                "serverId":"17",
                "serverName":"Calendar",
                "serverUrl":"https://mcp.example.test",
                "accountKey":"default",
                "requestingAgentId":"agent-a"
            }),
        )
        .unwrap();
        let generation = registered["generation"].as_u64().unwrap();

        let accepted = host
            .dispatch(
                "feature.mcp.oauthComplete",
                &json!({
                    "provider":"calendar",
                    "state":"oauth-state-1",
                    "code":"authorization-code",
                    "serverId":"17",
                    "accountKey":"default",
                    "generation":generation,
                }),
            )
            .unwrap();
        assert_eq!(accepted["outcome"], "pending-validation");
        assert_eq!(accepted["serverId"], "17");
        assert_eq!(accepted["accountKey"], "default");
        assert_eq!(host.events.len(), 1);
        assert_eq!(host.events.front().unwrap()["type"], "mcp.auth.callback.accepted");
        assert!(
            host.mcp_auth_watches
                .lock()
                .unwrap()
                .watch("17", "default")
                .is_some(),
            "browser callback must not bypass backend token validation"
        );
        assert!(
            host.mcp_auth_watches
                .lock()
                .unwrap()
                .pending_completions()
                .is_empty(),
            "browser callback is not a canonical auth completion"
        );

        let stale = host
            .dispatch(
                "feature.mcp.oauthComplete",
                &json!({
                    "provider":"calendar",
                    "state":"oauth-state-old",
                    "code":"authorization-code",
                    "serverId":"17",
                    "accountKey":"default",
                    "generation":generation + 1,
                }),
            )
            .unwrap();
        assert_eq!(stale["status"], "stale");
        assert_eq!(host.events.len(), 1, "stale callback must not emit another event");
    }

    #[test]
    fn mcp_oauth_error_cancels_matching_watch_without_success_completion() {
        let app_data = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(app_data.path(), AndroidHostMode::Test);
        let registered = host.dispatch(
            "feature.mcp.authWatch.register",
            &json!({
                "serverId":"17",
                "serverName":"Calendar",
                "serverUrl":"https://mcp.example.test",
                "accountKey":"default",
                "requestingAgentId":"agent-a"
            }),
        )
        .unwrap();
        let generation = registered["generation"].as_u64().unwrap();
        let failed = host
            .dispatch(
                "feature.mcp.oauthComplete",
                &json!({
                    "provider":"calendar",
                    "state":"oauth-state-1",
                    "error":"access_denied",
                    "serverId":"17",
                    "accountKey":"default",
                    "generation":generation,
                }),
            )
            .unwrap();
        assert_eq!(failed["outcome"], "failed");
        assert!(host.mcp_auth_watches.lock().unwrap().watch("17", "default").is_none());
        assert!(host.mcp_auth_watches.lock().unwrap().pending_completions().is_empty());
        assert_eq!(host.events.back().unwrap()["type"], "mcp.auth.failed");
    }

    #[cfg(feature = "ci-account-session-import")]
    #[test]
    fn ci_session_validation_is_bounded_and_refresh_token_free() {
        let now = 1_000_000_u64;
        let valid = json!({
            "accessToken":"abcdefghijklmnopqrstuvwxyz0123456789",
            "deviceId":"gha-12345-7-interactive",
            "sessionId":"ci-runner:12345:7",
            "tokenType":"Bearer",
            "provider":"github-actions",
            "ciRunner":true,
            "accessTokenExpiresAt":now + 3_600,
        });
        let parsed = ci_account_session::parse_ci_account_session_document(&valid, now).unwrap();
        assert_eq!(parsed.access_token, "abcdefghijklmnopqrstuvwxyz0123456789");
        assert_eq!(parsed.session_id, "ci-runner:12345:7");
        assert_eq!(parsed.device_id, "gha-12345-7-interactive");

        let mut mismatched = valid.clone();
        mismatched["sessionId"] = Value::String("ci-runner:12345:8".into());
        assert!(ci_account_session::parse_ci_account_session_document(&mismatched, now).is_none());

        let mut refresh = valid.clone();
        refresh["refreshToken"] = Value::String("forbidden".into());
        assert!(ci_account_session::parse_ci_account_session_document(&refresh, now).is_none());

        let mut expired = valid.clone();
        expired["accessTokenExpiresAt"] = json!(now + 10);
        assert!(ci_account_session::parse_ci_account_session_document(&expired, now).is_none());
    }

    #[cfg(feature = "ci-account-session-import")]
    #[test]
    fn device_agent_session_exposes_only_the_validated_short_lived_ci_session() {
        let now = 1_000_000_u64;
        let identity = ci_account_session::parse_ci_account_session_document(
            &json!({
                "accessToken":"abcdefghijklmnopqrstuvwxyz0123456789",
                "deviceId":"gha-12345-7-interactive",
                "sessionId":"ci-runner:12345:7",
                "tokenType":"Bearer",
                "provider":"github-actions",
                "ciRunner":true,
                "accessTokenExpiresAt":now + 3_600,
            }),
            now,
        )
        .unwrap();
        let root = std::env::temp_dir().join(format!(
            "fabushi-device-session-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        host.logged_in = true;
        host.ci_session_identity = Some(identity);
        let session = host.device_agent_session();
        assert_eq!(session["loggedIn"], true);
        assert_eq!(session["available"], true);
        assert_eq!(session["deviceId"], "gha-12345-7-interactive");
        assert_eq!(session["sessionId"], "ci-runner:12345:7");
        assert_eq!(
            session["accessToken"],
            "abcdefghijklmnopqrstuvwxyz0123456789"
        );
        host.logged_in = false;
        assert_eq!(host.device_agent_session(), json!({"loggedIn":false}));
        let _ = std::fs::remove_dir_all(root);
    }


    #[test]
    fn portable_runtime_host_contract_executes_granted_tool_and_fences_errors_timeout_and_replay() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-portable-runtime-host-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let plugin_id = "contract-plugin";
        let install_dir = root
            .join("plugin-fixture")
            .join("1.0.0")
            .join("fixture");
        std::fs::create_dir_all(&install_dir).unwrap();
        std::fs::write(
            install_dir.join("plugin.mjs"),
            r#"
export const name = 'contract-plugin';
export function apply(ctx) {
  ctx.tools.register({
    name: 'contract.echo',
    async execute(args) {
      if (args.fail) throw new Error('contract-tool-failed');
      if (args.slow) await new Promise(resolve => setTimeout(resolve, 1200));
      return { echoed: args.value ?? null };
    }
  });
}
"#,
        )
        .unwrap();
        let pointer_dir = root.join("plugins").join(plugin_id);
        std::fs::create_dir_all(&pointer_dir).unwrap();
        std::fs::write(
            pointer_dir.join("active.json"),
            serde_json::to_vec_pretty(&json!({
                "pluginId":plugin_id,
                "version":"1.0.0",
                "artifactId":"fixture",
                "artifactSha256":"0000000000000000000000000000000000000000000000000000000000000000",
                "runtime":"deepseek-js",
                "entry":"plugin.mjs",
                "requestedPermissions":["storage"],
                "installedPath":install_dir.to_string_lossy(),
            }))
            .unwrap(),
        )
        .unwrap();

        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        host.logged_in = true;
        host.dispatch(
            "plugin.permission.grant",
            &json!({"pluginId":plugin_id,"permission":"storage"}),
        )
        .unwrap();
        let started = host
            .dispatch("runtime.start", &json!({"pluginId":plugin_id,"config":{}}))
            .unwrap();
        assert!(started["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool == "contract.echo")));

        let success = host
            .dispatch(
                "runtime.call",
                &json!({
                    "pluginId":plugin_id,
                    "tool":"contract.echo",
                    "requestId":"runtime-contract-success",
                    "arguments":{"value":"ok"}
                }),
            )
            .unwrap();
        assert_eq!(success["result"]["echoed"], "ok");
        let duplicate = host.dispatch(
            "runtime.call",
            &json!({
                "pluginId":plugin_id,
                "tool":"contract.echo",
                "requestId":"runtime-contract-success",
                "arguments":{"value":"duplicate"}
            }),
        );
        assert!(duplicate.is_err(), "duplicate runtime request must never execute twice");

        assert!(host
            .dispatch(
                "runtime.call",
                &json!({
                    "pluginId":plugin_id,
                    "tool":"contract.echo",
                    "requestId":"runtime-contract-error",
                    "arguments":{"fail":true}
                }),
            )
            .unwrap_err()
            .contains("runtime.call failed"));

        let timed_out = host.dispatch(
            "runtime.call",
            &json!({
                "pluginId":plugin_id,
                "tool":"contract.echo",
                "requestId":"runtime-contract-timeout",
                "timeoutMs":400,
                "arguments":{"slow":true}
            }),
        );
        assert!(
            timed_out.is_err(),
            "bounded runtime deadline must never be accepted as successful completion"
        );
        assert!(
            host.capability_broker
                .needs_reconciliation("runtime-contract-timeout"),
            "post-dispatch timeout must be durable outcome-unknown before any retry"
        );
        let replay = host.dispatch(
            "runtime.call",
            &json!({
                "pluginId":plugin_id,
                "tool":"contract.echo",
                "requestId":"runtime-contract-timeout",
                "arguments":{"value":"must-not-replay"}
            }),
        );
        assert!(
            replay.is_err(),
            "outcome-unknown side effects must never be blindly replayed"
        );

        let control = host.runtime_call_control();
        let cancel_request_id = "runtime-contract-cancel".to_string();
        let cancel_thread = std::thread::spawn({
            let control = control.clone();
            let request_id = cancel_request_id.clone();
            move || {
                for _ in 0..100 {
                    if control.signal_request(&request_id) {
                        return true;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                false
            }
        });
        let cancelled = host.dispatch(
            "runtime.call",
            &json!({
                "pluginId":plugin_id,
                "tool":"contract.echo",
                "requestId":cancel_request_id,
                "timeoutMs":1000,
                "arguments":{"slow":true}
            }),
        );
        assert!(cancel_thread.join().unwrap(), "concurrent control plane must reach the in-flight call");
        assert!(cancelled.is_err(), "signalled runtime.call must not settle successful");
        assert!(host.capability_broker.needs_reconciliation("runtime-contract-cancel"));

        let stop_control = host.runtime_call_control();
        let stop_thread = std::thread::spawn({
            let plugin_id = plugin_id.to_string();
            move || {
                for _ in 0..100 {
                    if stop_control.has_request("runtime-contract-stop-race") {
                        return stop_control.signal_plugin(&plugin_id) > 0;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                false
            }
        });
        let stop_race = host.dispatch(
            "runtime.call",
            &json!({
                "pluginId":plugin_id,
                "tool":"contract.echo",
                "requestId":"runtime-contract-stop-race",
                "timeoutMs":1000,
                "arguments":{"slow":true}
            }),
        );
        assert!(stop_thread.join().unwrap(), "runtime.stop control must cancel the in-flight plugin call");
        assert!(stop_race.is_err());
        assert!(host.capability_broker.needs_reconciliation("runtime-contract-stop-race"));

        host.dispatch("runtime.stop", &json!({"pluginId":plugin_id}))
            .unwrap();
        assert!(host
            .dispatch(
                "runtime.call",
                &json!({
                    "pluginId":plugin_id,
                    "tool":"contract.echo",
                    "requestId":"runtime-after-stop",
                    "arguments":{"value":"blocked"}
                }),
            )
            .is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn presentation_roster_mutation_is_account_fenced_and_replays_after_host_reopen() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-presentation-roster-mutation-{}",
            now_ms()
        ));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let created = host
            .dispatch(
                "createAgent",
                &json!({"name":"Original","description":"profile"}),
            )
            .unwrap();
        let agent_id = created["agent"]["id"].as_str().unwrap().to_string();
        let params = json!({
            "operationId":"presentation-operation-1",
            "accountFence":"session:test:android",
            "mutation":{"kind":"duplicate","id":agent_id},
        });
        let first = host
            .dispatch("feature.agent.rosterMutation", &params)
            .unwrap();
        assert_eq!(first["status"], "completed");
        let duplicate_id = first["result"]["agent"]["id"].as_str().unwrap().to_string();
        let replay = host
            .dispatch("feature.agent.rosterMutation", &params)
            .unwrap();
        assert_eq!(replay["result"]["agent"]["id"], duplicate_id);
        assert_eq!(host.dispatch("countAgents", &json!({})).unwrap(), json!(2));

        let stale = host.dispatch(
            "feature.agent.rosterMutation",
            &json!({
                "operationId":"presentation-operation-stale",
                "accountFence":"session:other-account",
                "mutation":{"kind":"delete","ids":[agent_id]},
            }),
        );
        assert!(stale.unwrap_err().contains("stale account"));
        assert_eq!(host.dispatch("countAgents", &json!({})).unwrap(), json!(2));
        drop(host);

        let mut reopened = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let recovered = reopened
            .dispatch("feature.agent.rosterMutation", &params)
            .unwrap();
        assert_eq!(recovered["result"]["agent"]["id"], duplicate_id);
        assert_eq!(
            reopened.dispatch("countAgents", &json!({})).unwrap(),
            json!(2),
            "Host restart must replay the committed operation instead of duplicating twice",
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn agent_roster_projects_canonical_transcript_run_and_durable_relationships() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-agent-roster-projection-{}",
            now_ms()
        ));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let agent = host
            .agents
            .lock()
            .unwrap()
            .create("Projection Agent", "profile")
            .unwrap();
        let partner = host
            .agents
            .lock()
            .unwrap()
            .create("Partner Agent", "profile")
            .unwrap();
        let account_fence = host.current_turn_account_fence().unwrap();
        host.messaging
            .lock()
            .unwrap()
            .deliver_agent_message(
                &account_fence,
                &agent.id,
                &partner.id,
                false,
                "relationship message",
                &[],
                false,
                "projection-relationship-call",
                90,
            )
            .unwrap();
        host.transcript
            .lock()
            .unwrap()
            .append_entry(json!({
                "id":"projection-user-1",
                "kind":"message",
                "role":"user",
                "content":"older",
                "operationId":"projection-op",
                "agentId":agent.id.clone(),
                "timestampMs":100_u64,
            }))
            .unwrap();
        host.transcript
            .lock()
            .unwrap()
            .append_entry(json!({
                "id":"projection-assistant-1",
                "kind":"message",
                "role":"assistant",
                "content":"canonical latest message",
                "operationId":"projection-op",
                "agentId":agent.id.clone(),
                "timestampMs":200_u64,
            }))
            .unwrap();
        host.active_operations.insert("projection-op".into());

        let list = host.dispatch("listAgents", &json!({})).unwrap();
        let row = list
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == agent.id)
            .unwrap();
        assert_eq!(row["lastMessage"], "canonical latest message");
        assert_eq!(row["isRunning"], true);
        assert_eq!(row["conversationPartnerIds"], json!([partner.id.clone()]));
        assert_eq!(row["awaitingUserResponse"], Value::Null);
        assert!(row["updatedAt"].as_u64().unwrap() >= 200);

        host.active_operations.remove("projection-op");
        drop(host);

        let mut reopened = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let reopened_list = reopened.dispatch("listAgents", &json!({})).unwrap();
        let reopened_row = reopened_list
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == agent.id)
            .unwrap();
        assert_eq!(
            reopened_row["conversationPartnerIds"],
            json!([partner.id.clone()]),
            "canonical relationship must survive Host process restart"
        );
        assert_eq!(reopened_row["isRunning"], false);

        reopened
            .agents
            .lock()
            .unwrap()
            .delete(&[partner.id.clone()])
            .unwrap();
        let filtered = reopened.dispatch("listAgents", &json!({})).unwrap();
        let filtered_row = filtered
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == agent.id)
            .unwrap();
        assert_eq!(
            filtered_row["conversationPartnerIds"],
            json!([]),
            "relationship endpoint absent from the canonical roster must not be projected"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn production_turn_lifecycle_is_wired_into_android_chat_dispatch() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-agent-lifecycle-wiring-{}",
            now_ms()
        ));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let agent = host
            .agents
            .lock()
            .unwrap()
            .create("Lifecycle Agent", "profile")
            .unwrap();
        let account_fence = host.current_turn_account_fence().unwrap();

        host.dispatch(
            "feature.agent.diskPressure.record",
            &json!({
                "conversationId":agent.id.clone(),
                "episodeId":"disk-episode-1"
            }),
        )
        .unwrap();

        let accepted = host
            .dispatch(
                "feature.execute",
                &json!({
                    "command":{
                        "type":"chat.send",
                        "requestId":"lifecycle-request-1",
                        "agentId":agent.id.clone(),
                        "model":"default",
                        "text":"hello"
                    }
                }),
            )
            .unwrap();
        let operation_id = accepted["operationId"].as_str().unwrap().to_string();

        let mut completed = false;
        for _ in 0..64 {
            let event = host.dispatch("feature.receive", &json!({})).unwrap();
            if event["type"] == "operation.completed"
                && event["operationId"] == operation_id
            {
                completed = true;
                break;
            }
        }
        assert!(completed, "test provider turn must reach one terminal completion");

        assert_eq!(
            host.turn_lifecycle
                .lock()
                .unwrap()
                .claim_disk_pressure(
                    &account_fence,
                    &agent.id,
                    "probe-after-completion",
                    now_ms(),
                )
                .unwrap(),
            None,
            "successful production owner must commit rather than release the reminder claim"
        );
        let revision = crate::sha256::sha256_hex(
            format!(
                "{}\n{}\n{}\n{}",
                agent.id, agent.name, agent.description, agent.updated_at
            )
            .as_bytes(),
        );
        assert!(!host
            .turn_lifecycle
            .lock()
            .unwrap()
            .profile_announcement_needed(&account_fence, &agent.id, &revision));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn account_change_fences_old_turn_before_stale_terminal_or_transcript_write() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-agent-account-fence-{}",
            now_ms()
        ));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let account_fence = host.current_turn_account_fence().unwrap();
        let generation = host
            .turn_journal
            .lock()
            .unwrap()
            .begin("request-old", "operation-old", &account_fence, now_ms())
            .unwrap();
        let cancellation = Arc::new(AtomicBool::new(false));
        host.turn_cancellations
            .insert("operation-old".into(), Arc::clone(&cancellation));
        host.active_operations.insert("operation-old".into());

        host.fence_turns_for_account_change(&account_fence, "account-switch")
            .unwrap();

        assert!(cancellation.load(Ordering::Acquire));
        assert!(!host.active_operations.contains("operation-old"));
        assert_eq!(
            host.turn_journal
                .lock()
                .unwrap()
                .record("request-old")
                .unwrap()
                .state,
            DurableTurnState::OutcomeUnknown
        );
        assert!(host
            .turn_journal
            .lock()
            .unwrap()
            .assert_current(
                "request-old",
                "operation-old",
                &account_fence,
                generation,
            )
            .is_err());
        let stale_entry = host
            .transcript
            .lock()
            .unwrap()
            .entry("assistant:operation-old");
        assert!(stale_entry.is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn outcome_unknown_reconcile_is_explicit_account_fenced_and_idempotent() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-agent-reconcile-{}",
            now_ms()
        ));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let account_fence = host.current_turn_account_fence().unwrap();
        host.turn_journal
            .lock()
            .unwrap()
            .begin("request-unknown", "operation-unknown", &account_fence, 1)
            .unwrap();
        host.turn_journal
            .lock()
            .unwrap()
            .mark_account_outcome_unknown(&account_fence, "process/account boundary", 2)
            .unwrap();

        let reconciled = host
            .dispatch(
                "feature.agent.turn.reconcile",
                &json!({
                    "requestId":"request-unknown",
                    "outcome":"completed",
                    "assistantText":"reconciled answer",
                    "reason":"provider transcript confirmed"
                }),
            )
            .unwrap();
        assert_eq!(reconciled["state"], "completed");
        let reconciled_entry = host
            .transcript
            .lock()
            .unwrap()
            .entry("assistant:operation-unknown");
        assert_eq!(
            reconciled_entry
                .as_ref()
                .and_then(|entry| entry.get("content"))
                .and_then(Value::as_str),
            Some("reconciled answer")
        );
        assert!(host
            .dispatch(
                "feature.agent.turn.reconcile",
                &json!({
                    "requestId":"request-unknown",
                    "outcome":"completed",
                    "assistantText":"duplicate"
                }),
            )
            .is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn upgrade_quiesce_fences_new_chat_dispatch_and_resume_reopens_it() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-agent-upgrade-quiesce-{}",
            now_ms()
        ));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        host.dispatch(
            "feature.agent.upgradeQuiesce",
            &json!({"quiescing":true}),
        )
        .unwrap();
        assert!(host
            .dispatch(
                "feature.execute",
                &json!({
                    "command":{
                        "type":"chat.send",
                        "requestId":"quiesced",
                        "text":"blocked"
                    }
                }),
            )
            .unwrap_err()
            .contains("quiescing for upgrade"));
        assert_eq!(
            host.dispatch("feature.transcript.snapshot", &json!({})).unwrap(),
            json!([]),
            "upgrade quiesce rejection must happen before canonical transcript mutation"
        );
        drop(host);

        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        assert_eq!(
            host.dispatch("feature.transcript.snapshot", &json!({})).unwrap(),
            json!([]),
            "quiesced rejection must not leave a ghost message after Host reopen"
        );

        host.dispatch(
            "feature.agent.upgradeQuiesce",
            &json!({"quiescing":false}),
        )
        .unwrap();
        assert!(host
            .dispatch(
                "feature.execute",
                &json!({
                    "command":{
                        "type":"chat.send",
                        "requestId":"resumed",
                        "text":"allowed"
                    }
                }),
            )
            .is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn subagent_review_and_allowed_type_projection_fail_closed_before_mutation() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-subagent-review-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let base = json!({
            "toolName":crate::runner::TASK_TOOL_NAME,
            "toolCallId":"review-tool",
            "parentAgentId":"agent-parent",
            "parentRequestId":"parent-request",
            "model":"default",
        });

        let mut denied = base.clone();
        denied["arguments"] = json!({
            "prompt":"[[review:deny]] unsafe child request",
            "subagent_type":"general-purpose"
        });
        let denied = host
            .dispatch("feature.agent.subagent.tool", &denied)
            .unwrap();
        assert_eq!(denied["status"], "review-denied");
        assert!(host.subagent_owner.lock().unwrap().all_records().is_empty());

        let mut review_error = base.clone();
        review_error["toolCallId"] = json!("review-error-tool");
        review_error["arguments"] = json!({
            "prompt":"[[review:error]] classifier unavailable",
            "subagent_type":"general-purpose"
        });
        assert!(host
            .dispatch("feature.agent.subagent.tool", &review_error)
            .unwrap_err()
            .contains("auto-review failure"));
        assert!(host.subagent_owner.lock().unwrap().all_records().is_empty());

        let mut invalid_type = base;
        invalid_type["toolCallId"] = json!("invalid-type-tool");
        invalid_type["arguments"] = json!({
            "prompt":"ordinary child request",
            "subagent_type":"computeruse"
        });
        assert!(host
            .dispatch("feature.agent.subagent.tool", &invalid_type)
            .unwrap_err()
            .contains("unavailable for this turn"));
        assert!(host.subagent_owner.lock().unwrap().all_records().is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn root_agent_management_routes_through_feature_execute_provider_and_replays_after_reopen() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-agent-management-provider-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let run_send = |host: &mut AndroidJsonHost, request_id: &str| {
            let accepted = host
                .dispatch(
                    "feature.execute",
                    &json!({"command":{
                        "type":"chat.send",
                        "requestId":request_id,
                        "agentId":"mahayana-assistant",
                        "model":"default",
                        "text":"[[tool:SendToAgent]] {\"target_id\":\"agent-00000001\",\"message\":\"provider delivery\",\"images\":[{\"url\":\"https://example.com/image.png\"}],\"priority\":true}"
                    }}),
                )
                .unwrap();
            let operation_id = accepted["operationId"].as_str().unwrap().to_string();
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            while std::time::Instant::now() < deadline {
                let event = host.dispatch("feature.receive", &json!({})).unwrap();
                if event["type"] == "operation.completed" && event["operationId"] == operation_id {
                    return;
                }
                if event["type"] == "operation.failed" && event["operationId"] == operation_id {
                    panic!("root Agent management provider turn failed: {event}");
                }
                if event.as_object().is_some_and(|object| object.is_empty()) {
                    thread::sleep(Duration::from_millis(10));
                }
            }
            panic!("root Agent management provider turn did not settle");
        };

        {
            let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
            host.dispatch(
                "createAgent",
                &json!({"name":"Target","description":"provider target"}),
            )
            .unwrap();
            run_send(&mut host, "agent-management-provider-1");
        }

        let first_repository: Value = serde_json::from_slice(
            &std::fs::read(root.join("messaging-repository.json")).unwrap(),
        )
        .unwrap();
        let first_count = first_repository["messages"]
            .as_object()
            .unwrap()
            .values()
            .flat_map(|messages| messages.as_object().into_iter().flat_map(|map| map.values()))
            .count();
        assert_eq!(first_count, 1, "first provider tool call must commit one message");

        {
            let mut reopened = AndroidJsonHost::new(&root, AndroidHostMode::Test);
            run_send(&mut reopened, "agent-management-provider-2");
        }
        let replayed_repository: Value = serde_json::from_slice(
            &std::fs::read(root.join("messaging-repository.json")).unwrap(),
        )
        .unwrap();
        let replayed_count = replayed_repository["messages"]
            .as_object()
            .unwrap()
            .values()
            .flat_map(|messages| messages.as_object().into_iter().flat_map(|map| map.values()))
            .count();
        assert_eq!(
            replayed_count, 1,
            "stable provider tool_call_id must replay durable SendToAgent after Host reopen"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn parent_multitask_todo_routes_through_shipping_provider_and_survives_reopen() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-multitask-todo-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let account_fence = host.current_turn_account_fence().unwrap();
        let mut command = json!({
            "type":"chat.send",
            "requestId":"todo-root-request",
            "agentId":"mahayana-assistant",
            "model":"default",
            "text":"[[tool:TodoWrite]] {\"todos\":[{\"id\":\"a\",\"content\":\"first\",\"status\":\"in_progress\"},{\"id\":\"b\",\"content\":\"second\",\"status\":\"pending\"}],\"merge\":false}"
        });
        command[COORDINATOR_SUBAGENT_CAPABILITIES_FIELD] = json!({
            "multitaskEnabled":true,
            "remoteBoxAvailable":false,
            "remoteBoxHasDesktop":false,
            "browserUseEnabled":false
        });
        let accepted = host
            .dispatch("feature.execute", &json!({"command":command}))
            .unwrap();
        let operation_id = accepted["operationId"].as_str().unwrap().to_string();

        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let mut completed = false;
        while std::time::Instant::now() < deadline && !completed {
            let event = host.dispatch("feature.receive", &json!({})).unwrap();
            completed = event["type"] == "operation.completed"
                && event["operationId"] == operation_id;
            if event.as_object().is_some_and(|object| object.is_empty()) {
                thread::sleep(Duration::from_millis(10));
            }
        }
        assert!(completed, "root TodoWrite provider turn must settle");
        let snapshot = host
            .multitask_todos
            .lock()
            .unwrap()
            .snapshot(&account_fence, "mahayana-assistant");
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].id, "a");
        drop(host);

        let reopened = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let reopened_fence = reopened.current_turn_account_fence().unwrap();
        assert_eq!(
            reopened
                .multitask_todos
                .lock()
                .unwrap()
                .snapshot(&reopened_fence, "mahayana-assistant")
                .len(),
            2,
            "process reopen must retain root multitask TODO state"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn parent_agent_provider_routes_task_through_frozen_subagent_graph() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-subagent-provider-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let accepted = host
            .dispatch(
                "feature.execute",
                &json!({"command":{
                    "type":"chat.send",
                    "requestId":"parent-tool-request",
                    "agentId":"mahayana-assistant",
                    "model":"default",
                    "text":"[[tool:Task]] {\"prompt\":\"child work\",\"subagent_type\":\"general-purpose\"}"
                }}),
            )
            .unwrap();
        let operation_id = accepted["operationId"].as_str().unwrap().to_string();

        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let mut parent_completed = false;
        let mut child_terminal = false;
        while std::time::Instant::now() < deadline && !(parent_completed && child_terminal) {
            let event = host.dispatch("feature.receive", &json!({})).unwrap();
            if event["type"] == "operation.completed" && event["operationId"] == operation_id {
                parent_completed = true;
            }
            if matches!(
                event.get("type").and_then(Value::as_str),
                Some("subagent.completed") | Some("subagent.failed") | Some("subagent.aborted")
            ) {
                child_terminal = true;
            }
            if event.as_object().is_some_and(|object| object.is_empty()) {
                thread::sleep(Duration::from_millis(10));
            }
        }
        assert!(parent_completed, "parent routed-provider turn must settle");
        assert!(child_terminal, "Task routed tool must launch and settle a durable child");

        let records = host.subagent_owner.lock().unwrap().all_records();
        assert_eq!(records.len(), 1);
        let frozen = records[0].frozen_turn.as_ref().expect("frozen child turn");
        assert_eq!(frozen.model_id, "deepseek-chat");
        assert_eq!(frozen.allowed_subagent_types, vec!["general-purpose"]);
        assert!(
            frozen.tool_names.is_empty(),
            "no outbound child adapter is currently registered, so role-derived child execution must fail closed while parent controls remain parent-only"
        );
        assert_eq!(
            frozen.summarization_binding_id,
            "android-host-inference:same-provider"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn production_platform_request_contract_rejects_auth_escape_and_fake_plugin_success() {
        assert!(validate_platform_api_path("/v1/marketplace/plugins?platform=android").is_ok());
        assert!(validate_platform_api_path("/api/auth/logout").is_err());
        assert!(validate_platform_api_path("/v1/../api/auth/logout").is_err());
        assert!(validate_platform_api_path("/v1/%2e%2e/api/auth/logout").is_err());

        let root = std::env::temp_dir().join(format!("fabushi-platform-contract-{}", now_ms()));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        for method in ["runtime.start", "runtime.stop", "runtime.tools"] {
            assert!(host.dispatch(method, &json!({"pluginId":"test"})).is_err());
        }
        assert!(host
            .dispatch("runtime.call", &json!({"pluginId":"test"}))
            .unwrap_err()
            .contains("tool is required"));
        assert!(host
            .dispatch("plugin.compatibility", &json!({"pluginId":"test"}))
            .unwrap_err()
            .contains("plugin is not installed"));
        assert!(host
            .dispatch(
                "plugin.permission.grant",
                &json!({"pluginId":"test","permission":"network"}),
            )
            .unwrap_err()
            .contains("plugin is not installed"));
        assert!(host
            .dispatch(
                "plugin.permission.revoke",
                &json!({"pluginId":"test","permission":"network"}),
            )
            .unwrap_err()
            .contains("plugin is not installed"));
        let production_root = std::env::temp_dir().join(format!("fabushi-plugin-production-{}", now_ms()));
        let mut production_feature_host = AndroidJsonHost::new(&production_root, AndroidHostMode::Production);
        assert!(production_feature_host
            .dispatch(
                "feature.execute",
                &json!({"command":{"type":"marketplace.install","requestId":"install-1","miniAppId":"global-dharma"}}),
            )
            .unwrap_err()
            .contains("requires the immutable release manifest"));
        assert!(production_feature_host
            .dispatch(
                "feature.execute",
                &json!({"command":{"type":"unknown.production.command","requestId":"unknown-1"}}),
            )
            .unwrap_err()
            .contains("unsupported Android feature command"));

        let mut production_host = AndroidJsonHost::new(&production_root, AndroidHostMode::Production);
        assert!(production_host
            .dispatch(
                "feature.plugin.install",
                &json!({"release":{"pluginId":"global-dharma"}}),
            )
            .unwrap_err()
            .contains("plugin release manifest is invalid"));
        assert!(production_host
            .dispatch("feature.plugin.uiDocument", &json!({"pluginId":"global-dharma"}))
            .unwrap_err()
            .contains("refuses placeholder HTML"));
        let _ = std::fs::remove_dir_all(production_root);

        assert!(host
            .dispatch(
                "platform.request",
                &json!({
                    "authenticated":true,
                    "method":"POST",
                    "path":"/api/auth/logout",
                    "body":{}
                }),
            )
            .is_err());
        let _ = std::fs::remove_dir_all(root);
    }


}
