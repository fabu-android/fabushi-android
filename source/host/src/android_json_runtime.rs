use crate::extensions::content_search::android_search_index::AndroidContentSearchIndex;
use crate::edge_failures::EdgeFailureBuffer;
use crate::account_service::{AccountSessionMutation, AndroidAccountService};
use crate::account_telemetry::{
    AccountTelemetryBuffer, AccountTelemetryProjection, SessionReport, SigninGateReport,
    SigninLoginReport, SigninSignoutCause,
};
use crate::account_telemetry_spill::DurableAccountTelemetryOutbox;
use crate::capability_broker::{CapabilityDecision, PendingCapabilityCall, SharedCapabilityBroker};
use crate::automation_id::stable_account_automation_id;
use crate::automation_runtime::{run_json as automation_run_json, AutomationRuntime, AutomationSpec};
use crate::auto_review_settings::{AutoReviewInstructions, DurableAutoReviewSettings};
use crate::android_agent_roster::AndroidAgentRoster;
use crate::android_process_metrics::AndroidProcessMetricsCollector;
use crate::process_crash_guard::{AndroidUncleanExitKind, AndroidUncleanExitMarker};
use crate::android_structured_log_spill::AndroidStructuredLogSpool;
use crate::agent_attachment::{
    prompt_with_agent_attachments, with_agent_attachment_tools, AgentAttachment,
    DurableAgentAttachmentOwner, COMPOSER_ATTACHMENT_LIMIT,
};
use crate::android_sidebar_sections::{AndroidSidebarSection, AndroidSidebarSections};
use crate::android_user_settings::{AndroidUserSettings, LocalToolPermission};
use crate::host_secret_store::get_or_create_host_machine_id;
use crate::onepassword_provisioning::{DurableOnePasswordProvisioningOwner, DurableProvisioningPhase};
use crate::human_call_runtime::HumanCallRuntime;
use crate::env_descriptor_account_binding::EnvDescriptorAccountBinding;
use crate::host_runner_composition::AuthenticatedRemoteHostRunner;
use crate::messaging_service::AndroidMessagingService;
use crate::mcp_server_tool_settings::McpServerToolSettings;
use crate::composer_draft::DurableComposerDraftOwner;
use crate::send_acknowledgement::{
    DurableSendAcknowledgementOwner, SendAcknowledgementEntry, SendAcknowledgementPhase,
    SendAcknowledgementRecord,
};
use crate::plugin_variable_store::{variable_fields_json, PluginVariableStore, PreparedPluginVariableWrite};
use crate::mcp_auth::{
    cleanup_legacy_mcp_auth_credentials, AndroidMcpAuthWatchManager,
    AndroidMcpAuthWatchOwner, CursorDashboardMcpAuthBackend, McpAuthAdminPolicyPort,
    McpAuthBackendPort, McpAuthOwnerEvent, McpAuthenticateResult,
    McpToolExecutionBackendResult, EXECUTE_TOOL_TIMEOUT_MS,
    SandPrivacyMode as BackendSandPrivacyMode,
};
use crate::extensions::telemetry::product_events::{
    access_blocked_event, agent_load_event, agents_unreachable_event, client_persistence_event,
    client_resource_event, connector_auth_clicked_event, entrypoint_error_event,
    event_loop_pressure_event, onboarding_step_event, reaction_ack_event, render_stream_event,
    render_ttfr_event, renderer_boundary_caught_event, renderer_unhandled_event, send_ack_event,
    send_latency_event, send_queued_flush_event,
    startup_lifecycle_event,
    StructuredTelemetryEvent, TelemetryLevel,
};
use crate::extensions::telemetry::recovery_events::{
    rebuild_escalation_event, rebuild_pending_stall_event, recovery_action_event,
    replica_resync_event, resync_completed_event, send_journal_restore_event,
};
use crate::extensions::teach_recording::{DurableTeachRecordingOwner, TeachRecordingPhase};
use crate::extensions::trays::extension::{start_trays_extension, HostTraysExtension};
use crate::extensions::trays::trays_service::{TrayEvent, TraySubscription};
use crate::extensions::transcript::{AsyncTask, SandPendingWakeStore, TranscriptRecoveryOutcome, TranscriptStore, merge_async_tasks};
use crate::extensions::transcript::pending_wake_rearm::PENDING_WAKE_STALE_MAX_AGE_MS;
use crate::extensions::webauthn_proxy::{
    WebAuthnBridgeError, WebAuthnProxyExtension, WebAuthnProxyExtensionConfig,
};
use crate::runner::{
    AndroidHostInferenceProvider, AndroidInferenceMode, AndroidSubagentReviewDecision,
    DurableTurnJournal, DurableTurnState,
    ProductionTurnAgentBuildBindings, ProductionTurnAgentLifecycleBindings,
    ProductionTurnAgentOwner, ProductionTurnAgentStaticConfig, ProductionTurnEvent,
    ProductionDiskPressureLevel, ProductionTurnInput, ProductionTurnLifecycleStore,
    ProductionTurnPrivacyMode, ProductionTurnCheckpointPersist,
    ProductionTurnProfileAnnouncementCommit, SAND_AGENT_TOKEN_LIMIT,
    shared_turn_run_shell, SharedTurnRunShell,
    build_turn_subagent_types, parse_turn_subagent_capability_projection,
    COORDINATOR_SUBAGENT_CAPABILITIES_FIELD, DurableSubagentOwner, SubagentFrozenTurnConfig,
    SubagentRunOutcome, SubagentSteerReview, SubagentTaskReviewCallback,
    SubagentSteerReviewCallback, SubagentToolBridge, SubagentToolContext,
    build_parent_subagent_routed_tools, spawn_generated_subagent,
    with_agent_management_tools, with_multitask_todo_tools, with_remote_routed_tools,
    AgentTurnInterruptionRegistry, RemoteApprovalRegistry, RemoteDispatchBinding,
    DurableMultitaskTodoStore,
};
use fabushi_android_shared::feedback::normalize_feedback_submit_args;
use fabushi_constants::composer::text_size_allowed;
use fabushi_android_shared::node::mcp::mcp_auth_watch_lifecycle::{
    McpAuthPollOutcome, McpAuthPollRequest, McpAuthPollSettlement, McpAuthPollTick,
    McpAuthTransport, McpAuthWatchCompletion,
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
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ASSISTANT_READ_MARKER_ID: &str = "projection:mahayana-assistant:last-read";
const ACCOUNT_TELEMETRY_OUTBOX_CAP: usize = 16;
const PROCESS_CRASH_EVENT_CAP: usize = 20;

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

fn mcp_account_fence_generation(account_fence: &str) -> u64 {
    let digest = crate::sha256::sha256_hex(account_fence.as_bytes());
    u64::from_str_radix(&digest[..16], 16).unwrap_or(1).max(1)
}

fn mark_turn_awaiting_user(
    shell: &SharedTurnRunShell,
    lifecycle: &Arc<Mutex<ProductionTurnLifecycleStore>>,
    account_fence: &str,
    agent_id: &str,
    request_id: &str,
    operation_id: &str,
    turn_generation: u64,
    process_epoch: u64,
    reason: &str,
) -> Result<(), String> {
    shell
        .lock()
        .map_err(|_| "turn run shell lock poisoned".to_string())?
        .mark_awaiting_user(operation_id)
        .map_err(|error| format!("turn waiting-user mark rejected: {error:?}"))?;
    let durable = lifecycle
        .lock()
        .map_err(|_| "turn lifecycle lock poisoned".to_string())?
        .mark_awaiting_user(
            account_fence,
            agent_id,
            request_id,
            operation_id,
            turn_generation,
            process_epoch,
            reason,
            now_ms(),
        );
    if let Err(error) = durable {
        if let Ok(mut shell) = shell.lock() {
            let _ = shell.resume_awaiting_user(operation_id);
        }
        return Err(error);
    }
    Ok(())
}

fn clear_turn_awaiting_user(
    shell: &SharedTurnRunShell,
    lifecycle: &Arc<Mutex<ProductionTurnLifecycleStore>>,
    account_fence: &str,
    agent_id: &str,
    request_id: &str,
    operation_id: &str,
    turn_generation: u64,
    process_epoch: u64,
) -> Result<(), String> {
    lifecycle
        .lock()
        .map_err(|_| "turn lifecycle lock poisoned".to_string())?
        .clear_awaiting_user(
            account_fence,
            agent_id,
            request_id,
            operation_id,
            turn_generation,
            process_epoch,
        )?;
    shell
        .lock()
        .map_err(|_| "turn run shell lock poisoned".to_string())?
        .resume_awaiting_user(operation_id)
        .map_err(|error| format!("turn waiting-user resume rejected: {error:?}"))?;
    Ok(())
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

#[derive(Clone, Debug)]
struct AndroidMessagingAccessGrant {
    account_fence: String,
    actor_id: String,
    device_id: String,
    session_id: String,
    scopes: BTreeSet<String>,
}

impl AndroidMessagingAccessGrant {
    fn authorize(
        &self,
        account_fence: &str,
        actor_id: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        scope: &str,
    ) -> Result<(), String> {
        if self.account_fence != account_fence || self.actor_id != actor_id {
            return Err("messaging access grant is stale for the current account session".into());
        }
        if device_id.is_some_and(|value| value != self.device_id)
            || session_id.is_some_and(|value| value != self.session_id)
        {
            return Err("messaging access grant does not match request device/session context".into());
        }
        if !self.scopes.contains(scope) {
            return Err(format!("messaging access grant is missing required scope {scope}"));
        }
        Ok(())
    }
}

fn restored_send_journal_telemetry(
    records: &[SendAcknowledgementRecord],
    observed_at_ms: u64,
) -> Vec<StructuredTelemetryEvent> {
    let mut groups = BTreeMap::<&'static str, (u64, u64)>::new();
    for record in records {
        let phase = match record.phase {
            SendAcknowledgementPhase::Pending => Some("prepared"),
            SendAcknowledgementPhase::Queued => Some("queued"),
            SendAcknowledgementPhase::Dispatching => Some("dispatching"),
            SendAcknowledgementPhase::AcceptedAwaitingEcho => Some("accepted-awaiting-echo"),
            SendAcknowledgementPhase::Failed | SendAcknowledgementPhase::OutcomeUnknown => None,
        };
        let Some(phase) = phase else { continue };
        let age_ms = if record.entered_at_ms == 0 {
            0
        } else {
            observed_at_ms.saturating_sub(record.entered_at_ms)
        };
        let group = groups.entry(phase).or_insert((0, 0));
        group.0 = group.0.saturating_add(1);
        group.1 = group.1.max(age_ms);
    }
    groups
        .into_iter()
        .filter_map(|(phase, (record_count, age_ms))| {
            send_journal_restore_event(
                "parked",
                phase,
                age_ms as f64,
                record_count as f64,
            )
            .ok()
        })
        .collect()
}

pub struct AndroidJsonHost {
    mode: AndroidHostMode,
    device_id: String,
    app_version: String,
    account: AndroidAccountService,
    account_binding: EnvDescriptorAccountBinding,
    preexisting_account_durable_data: bool,
    account_binding_refusal: Option<String>,
    account_telemetry: AccountTelemetryBuffer,
    account_telemetry_outbox: Arc<Mutex<Option<DurableAccountTelemetryOutbox>>>,
    account_telemetry_spill_path: PathBuf,
    account_telemetry_shipping: Arc<AtomicBool>,
    edge_failures: EdgeFailureBuffer,
    structured_log_spool: AndroidStructuredLogSpool,
    structured_log_counter_id: String,
    process_metrics: AndroidProcessMetricsCollector,
    process_metrics_recent: VecDeque<Value>,
    process_crash_recent: VecDeque<Value>,
    unclean_exit: AndroidUncleanExitMarker,
    agents: Arc<Mutex<AndroidAgentRoster>>,
    sidebar_sections: Arc<Mutex<AndroidSidebarSections>>,
    user_settings: Arc<Mutex<AndroidUserSettings>>,
    detected_time_zone: Arc<Mutex<Option<String>>>,
    auto_review_settings: Arc<Mutex<DurableAutoReviewSettings>>,
    transcript: Arc<Mutex<TranscriptStore>>,
    content_search: AndroidContentSearchIndex,
    send_acknowledgements: Arc<Mutex<DurableSendAcknowledgementOwner>>,
    composer_drafts: Arc<Mutex<DurableComposerDraftOwner>>,
    agent_attachments: Arc<Mutex<DurableAgentAttachmentOwner>>,
    messaging: Arc<Mutex<AndroidMessagingService>>,
    messaging_access: Option<AndroidMessagingAccessGrant>,
    mcp_auth_watches: Arc<Mutex<AndroidMcpAuthWatchManager>>,
    mcp_auth_owner: Option<AndroidMcpAuthWatchOwner>,
    mcp_dashboard_backend: Option<Arc<CursorDashboardMcpAuthBackend>>,
    mcp_server_tool_settings: Arc<Mutex<McpServerToolSettings>>,
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
    pending_wake_store: SandPendingWakeStore,
    subagent_tools: SubagentToolBridge,
    subagent_events: Arc<Mutex<VecDeque<Value>>>,
    subagent_review_approvals: Arc<SubagentReviewApprovalRegistry>,
    teach_recording: Arc<Mutex<DurableTeachRecordingOwner>>,
    onepassword_provisioning: Arc<Mutex<DurableOnePasswordProvisioningOwner>>,
    trays: HostTraysExtension,
    tray_events: Arc<Mutex<VecDeque<Value>>>,
    _tray_subscription: TraySubscription,
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
    human_calls: HumanCallRuntime,
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
        // Non-production and isolated Rust tests do not have Android Keystore access, so they
        // retain a local durable identifier. Production JNI injects the platform-protected
        // identifier through new_with_account_session_and_device_id instead.
        std::fs::create_dir_all(&app_data_dir)
            .unwrap_or_else(|error| panic!("failed to create Android Host data directory: {error}"));
        let device_id = get_or_create_host_machine_id(&app_data_dir.join("machine-id"))
            .unwrap_or_else(|error| panic!("failed to open test Android machine id: {error}"));
        Self::new_with_account_session_and_device_id(
            app_data_dir,
            mode,
            initial_account_session_json,
            device_id,
        )
    }

    pub fn new_with_account_session_and_device_id(
        app_data_dir: impl Into<PathBuf>,
        mode: AndroidHostMode,
        initial_account_session_json: Option<&str>,
        device_id: impl Into<String>,
    ) -> Self {
        Self::new_with_account_session_and_device_id_and_app_version(
            app_data_dir,
            mode,
            initial_account_session_json,
            device_id,
            "unknown",
        )
    }

    pub fn new_with_account_session_and_device_id_and_app_version(
        app_data_dir: impl Into<PathBuf>,
        mode: AndroidHostMode,
        initial_account_session_json: Option<&str>,
        device_id: impl Into<String>,
        app_version: &str,
    ) -> Self {
        let app_data_dir = app_data_dir.into();
        let preexisting_account_durable_data = has_existing_account_durable_data(&app_data_dir);
        let startup_has_protected_session = mode == AndroidHostMode::Production
            && initial_account_session_json.is_some_and(|raw| !raw.trim().is_empty());
        // Every durable Host store shares this root. Create it explicitly before opening any
        // store instead of depending on whichever store happens to create the parent first.
        // This is required for deterministic process-reopen recovery and removes a parallel-test
        // race where a later durable owner could observe ENOENT.
        std::fs::create_dir_all(&app_data_dir)
            .unwrap_or_else(|error| panic!("failed to create Android Host data directory: {error}"));
        let _legacy_mcp_auth_cleanup = cleanup_legacy_mcp_auth_credentials(&app_data_dir);
        let device_id = device_id.into();
        assert!(
            !device_id.is_empty()
                && device_id.len() <= 200
                && !device_id.chars().any(|value| value.is_whitespace() || value.is_control()),
            "invalid canonical Android machine id",
        );
        let mut account = AndroidAccountService::with_persistent_browser_attempts(
            device_id.clone(),
            (mode == AndroidHostMode::Production)
                .then_some(initial_account_session_json)
                .flatten(),
            app_data_dir.join("account-oauth-attempt.json"),
        )
        .unwrap_or_else(|error| panic!("failed to initialize Android account service: {error}"));
        let account_binding = EnvDescriptorAccountBinding::new(
            app_data_dir.join("account-environment-bindings.json"),
        );
        let structured_log_account_slot = account
            .account_storage_scope()
            .unwrap_or_else(|| "logged-out".to_string());
        let structured_log_counter_id =
            format!("android:{}", crate::sha256::sha256_hex(device_id.as_bytes()));
        let mut structured_log_spool = AndroidStructuredLogSpool::open(
            app_data_dir.join("android-structured-log-spill.v1.json"),
            &structured_log_account_slot,
            &structured_log_counter_id,
            now_ms(),
        )
        .unwrap_or_else(|error| {
            panic!("failed to open durable Android structured-log spill: {error}")
        });
        let mut account_binding_refusal = None;
        if let Some(account_slot) = account.account_storage_scope() {
            let descriptor = account.environment_descriptor_url();
            match account_binding.authorize(
                &account_slot,
                &descriptor,
                startup_has_protected_session,
                preexisting_account_durable_data,
            ) {
                Ok(true) => {}
                Ok(false) => {
                    account.clear_local_session();
                    account_binding_refusal = Some(
                        "Existing Android durable data is bound to a different account environment."
                            .to_string(),
                    );
                }
                Err(error) => {
                    account.clear_local_session();
                    account_binding_refusal = Some(format!(
                        "Android account environment binding is invalid: {error}"
                    ));
                }
            }
        }
        let agents = Arc::new(Mutex::new(
            AndroidAgentRoster::open(app_data_dir.join("agents.json"))
                .unwrap_or_else(|error| panic!("failed to open canonical Android agent roster: {error}")),
        ));
        let sidebar_sections = Arc::new(Mutex::new(
            AndroidSidebarSections::open(app_data_dir.join("sidebar-sections.json"))
                .unwrap_or_else(|error| panic!("failed to open canonical Android sidebar sections: {error}")),
        ));
        let user_settings = Arc::new(Mutex::new(
            AndroidUserSettings::open_migrating(
                app_data_dir.join("android-user-settings.json"),
                app_data_dir.join("user-settings.json"),
            )
            .unwrap_or_else(|error| panic!("failed to open canonical Android user settings: {error}")),
        ));
        let auto_review_settings = Arc::new(Mutex::new(
            DurableAutoReviewSettings::open(app_data_dir.join("auto-review-settings.json"))
                .unwrap_or_else(|error| panic!("failed to open durable Android Auto Review settings: {error}")),
        ));

        let (transcript_store, transcript_recovery) =
            TranscriptStore::open_recovering(app_data_dir.join("transcript.json"))
                .unwrap_or_else(|error| panic!("failed to recover canonical Android transcript: {error}"));
        if let Some(recovery) = transcript_recovery {
            let (level, outcome) = match recovery.outcome {
                TranscriptRecoveryOutcome::RecoveredInterruptedWrite => {
                    ("warn", "recovered_interrupted_write")
                }
                TranscriptRecoveryOutcome::ResetCorruptStore => {
                    ("error", "reset_corrupt_store")
                }
            };
            let mut metadata = BTreeMap::from([
                ("component".to_string(), "transcript_store".to_string()),
                ("action".to_string(), "recover".to_string()),
                ("outcome".to_string(), outcome.to_string()),
            ]);
            if let Some(quarantine_path) = recovery.quarantine_path.as_ref() {
                metadata.insert(
                    "quarantinePath".to_string(),
                    quarantine_path.to_string_lossy().to_string(),
                );
            }
            let _ = structured_log_spool.record(
                level,
                "sand.recovery_action",
                metadata,
                now_ms(),
            );
        }
        let transcript = Arc::new(Mutex::new(transcript_store));
        let content_search = AndroidContentSearchIndex::open(app_data_dir.join("search-index.db"))
            .unwrap_or_else(|error| panic!("failed to open Android content search index: {error}"));
        let send_acknowledgement_observed_at_ms = now_ms();
        let send_acknowledgement_owner = DurableSendAcknowledgementOwner::open(
            app_data_dir.join("send-acknowledgements.json"),
            send_acknowledgement_observed_at_ms,
        )
        .unwrap_or_else(|error| {
            panic!("failed to open durable Android send acknowledgement owner: {error}")
        });
        for event in restored_send_journal_telemetry(
            &send_acknowledgement_owner.restored_on_open(),
            send_acknowledgement_observed_at_ms,
        ) {
            let level = match event.level {
                TelemetryLevel::Info => "info",
                TelemetryLevel::Warn => "warn",
                TelemetryLevel::Error => "error",
            };
            // Restore telemetry is observational only. Failure to persist a log record must never
            // change send-journal recovery truth or trigger a retry.
            let _ = structured_log_spool.record(
                level,
                "sand.send_journal.restore",
                event.metadata,
                send_acknowledgement_observed_at_ms,
            );
        }
        let send_acknowledgements = Arc::new(Mutex::new(send_acknowledgement_owner));
        let composer_drafts = Arc::new(Mutex::new(
            DurableComposerDraftOwner::open(app_data_dir.join("composer-drafts.json"), now_ms())
                .unwrap_or_else(|error| panic!("failed to open durable Android composer draft owner: {error}")),
        ));
        let agent_attachments = Arc::new(Mutex::new(
            DurableAgentAttachmentOwner::open(app_data_dir.join("agent-attachments"))
                .unwrap_or_else(|error| panic!("failed to open durable Android Agent attachment owner: {error}")),
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
        let mcp_server_tool_settings = Arc::new(Mutex::new(
            McpServerToolSettings::open(app_data_dir.join("mcp-server-tool-settings.json"))
                .unwrap_or_else(|error| panic!("failed to open durable MCP server tool settings: {error}")),
        ));
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
        let pending_wake_store = SandPendingWakeStore::new(&app_data_dir);
        let subagent_tools = SubagentToolBridge::new(Arc::clone(&subagent_owner));
        let subagent_events = Arc::new(Mutex::new(VecDeque::new()));
        let subagent_review_approvals = Arc::new(SubagentReviewApprovalRegistry::default());
        let teach_recording = Arc::new(Mutex::new(
            DurableTeachRecordingOwner::open(app_data_dir.join("teach-recording.json"), now_ms())
                .unwrap_or_else(|error| panic!("failed to open durable Teach Recording owner: {error}")),
        ));
        let onepassword_provisioning = Arc::new(Mutex::new(
            DurableOnePasswordProvisioningOwner::open(
                app_data_dir.join("onepassword-provisioning.json"),
            )
            .unwrap_or_else(|error| {
                panic!("failed to open durable 1Password provisioning owner: {error}")
            }),
        ));
        let trays = start_trays_extension();
        let tray_events = Arc::new(Mutex::new(VecDeque::new()));
        let tray_event_sink = Arc::clone(&tray_events);
        let tray_subscription = trays.subscribe(Arc::new(move |event| {
            let projected = match event {
                TrayEvent::Pushed(tray) => serde_json::to_value(tray)
                    .ok()
                    .map(|tray| json!({"type":"tray.pushed","tray":tray})),
                TrayEvent::Cleared => Some(json!({"type":"tray.cleared"})),
                TrayEvent::Dismissed(id) => Some(json!({"type":"tray.dismissed","id":id})),
            };
            if let Some(projected) = projected {
                if let Ok(mut queue) = tray_event_sink.lock() {
                    queue.push_back(projected);
                }
            }
        }));
        let (unclean_exit, prior_unclean_exit) = AndroidUncleanExitMarker::open_with_version(
            app_data_dir.join("host-session-marker.json"),
            now_ms(),
            app_version,
        )
        .unwrap_or_else(|error| panic!("failed to arm Android Host session marker: {error}"));
        let mut process_crash_recent = VecDeque::new();
        if let Some(prior) = prior_unclean_exit {
            let kind = match prior.kind {
                AndroidUncleanExitKind::UncleanExit => "unclean_exit",
                AndroidUncleanExitKind::MarkerOrphaned => "marker_orphaned",
            };
            let is_unclean_exit = matches!(prior.kind, AndroidUncleanExitKind::UncleanExit);
            let mut telemetry_metadata = BTreeMap::from([
                ("kind".to_string(), kind.to_string()),
                (
                    "prev_uptime_ms".to_string(),
                    prior.previous_uptime_bucket_ms.to_string(),
                ),
                ("prev_version".to_string(), prior.previous_version.clone()),
                ("same_version".to_string(), prior.same_version.to_string()),
                ("crash_seq_seen".to_string(), prior.crash_seen.to_string()),
                ("prev_started_at_ms".to_string(), prior.started_at_ms.to_string()),
            ]);
            if is_unclean_exit {
                telemetry_metadata.insert("error_code".to_string(), "SAND-E0610".to_string());
                telemetry_metadata.insert("error_domain".to_string(), "android-process".to_string());
                telemetry_metadata.insert("error_retryable".to_string(), "false".to_string());
            }
            if structured_log_spool
                .record(
                    if is_unclean_exit { "warn" } else { "info" },
                    "sand.desktop.unclean_exit",
                    telemetry_metadata,
                    now_ms(),
                )
                .is_err()
            {
                // Telemetry persistence is best-effort during early Host construction: a spill
                // failure must not turn an otherwise recoverable cold start into a product outage.
            }
            process_crash_recent.push_back(json!({
                "kind":kind,
                "occurredAtMs":prior.last_alive_at_ms,
                "uptimeMs":prior.previous_uptime_bucket_ms,
                "previousVersion":prior.previous_version,
                "sameVersion":prior.same_version,
                "crashSeen":prior.crash_seen,
                "errorCode":if is_unclean_exit {
                    Value::String("SAND-E0610".into())
                } else {
                    Value::Null
                },
                "errorDomain":"android-process",
                "errorRetryable":false,
            }));
        }
        Self {
            mode,
            device_id: device_id.clone(),
            app_version: app_version.to_string(),
            account,
            account_binding,
            preexisting_account_durable_data,
            account_binding_refusal,
            account_telemetry: AccountTelemetryBuffer::default(),
            account_telemetry_outbox: Arc::new(Mutex::new(None)),
            account_telemetry_spill_path: app_data_dir.join("account-telemetry-spill.json"),
            account_telemetry_shipping: Arc::new(AtomicBool::new(false)),
            edge_failures: EdgeFailureBuffer::default(),
            structured_log_spool,
            structured_log_counter_id,
            process_metrics: AndroidProcessMetricsCollector::default(),
            process_metrics_recent: VecDeque::new(),
            process_crash_recent,
            unclean_exit,
            agents,
            sidebar_sections,
            user_settings,
            detected_time_zone: Arc::new(Mutex::new(None)),
            auto_review_settings,
            transcript,
            content_search,
            send_acknowledgements,
            composer_drafts,
            agent_attachments,
            messaging,
            messaging_access: None,
            mcp_auth_watches,
            mcp_auth_owner,
            mcp_dashboard_backend,
            mcp_server_tool_settings,
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
            pending_wake_store,
            subagent_tools,
            subagent_events,
            subagent_review_approvals,
            teach_recording,
            onepassword_provisioning,
            trays,
            tray_events,
            _tray_subscription: tray_subscription,
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
            human_calls: HumanCallRuntime::open(app_data_dir.join("human-calls.json"), now_ms())
                .unwrap_or_else(|error| panic!("failed to open durable Android Human call runtime: {error}")),
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
                // Coarse presentation status is derived from the explicit executor contract.
                // Individual tool exposure remains executor-specific inside RemoteRoutedTools.
                "hasDesktop":
                    binding.supports("computer")
                    || binding.supports("screenshot")
                    || binding.supports("external-shell")
                    || binding.supports("external-read"),
                "executors": binding.executors.iter().cloned().collect::<Vec<_>>(),
                "deviceId": binding.device_id,
                "accountEpoch": binding.account_epoch,
            }),
            _ => json!({
                "ready": false,
                "hasDesktop": false,
                "executors": [],
                "deviceId": Value::Null,
                "accountEpoch": Value::Null,
            }),
        }
    }


    fn validate_available_model_selection(
        &self,
        selected_model: Option<&str>,
    ) -> Result<Option<String>, String> {
        let Some(selected_model) = selected_model
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(None);
        };
        let backend = self
            .mcp_dashboard_backend
            .as_ref()
            .ok_or("available model catalog backend is unavailable")?;
        let catalog = backend.resolve_available_models()?;
        let available = catalog
            .get("models")
            .and_then(Value::as_array)
            .is_some_and(|models| {
                models.iter().any(|model| {
                    model.get("name").and_then(Value::as_str) == Some(selected_model)
                })
            });
        if !available {
            return Err("selected model is not available for the current account".into());
        }
        Ok(Some(selected_model.to_string()))
    }

    fn available_model_catalog(&self) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let catalog = self
            .mcp_dashboard_backend
            .as_ref()
            .ok_or("available model catalog backend is unavailable")?
            .resolve_available_models()?;
        Ok(json!({"accountFence": account_fence, "catalog": catalog}))
    }

    fn mcp_team_popularity(&self) -> Result<Value, String> {
        // Desktop exposes this as best-effort account-scoped ranking metadata. Keep the account
        // fence in the projection so Presentation can discard a late result after account switch.
        let account_fence = self.current_turn_account_fence()?;
        let counts = self
            .mcp_dashboard_backend
            .as_ref()
            .map(|backend| backend.resolve_team_plugin_popularity())
            .unwrap_or_default();
        Ok(json!({
            "accountFence": account_fence,
            "counts": counts,
        }))
    }

    fn mcp_server_catalog(&self) -> Result<Value, String> {
        // A catalog projection is account-scoped backend state. Resolve the current fence first so
        // a stale/logged-out process cannot reuse a previously initialized backend owner.
        let account_fence = self.current_turn_account_fence()?;
        let backend = self
            .mcp_dashboard_backend
            .as_ref()
            .ok_or("MCP server catalog backend is unavailable")?;
        let servers = backend
            .list_server_snapshots()?
            .into_iter()
            .map(|server| {
                json!({
                    "id": server.server_id,
                    "name": server.server_name,
                    "url": server.server_url,
                    "transport": match server.transport {
                        McpAuthTransport::Http => "http",
                        McpAuthTransport::Sse => "sse",
                        McpAuthTransport::Stdio => "stdio",
                    },
                    "disabledByTeamAdminPolicy": server.disabled_by_team_admin_policy,
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "accountFence": account_fence,
            "servers": servers,
        }))
    }

    fn mcp_server_custom_instructions(&self, params: &Value) -> Result<Value, String> {
        let server_id = required_string(params, "serverId")?;
        let account_fence = self.current_turn_account_fence()?;
        let backend = self
            .mcp_dashboard_backend
            .as_ref()
            .ok_or("MCP server catalog backend is unavailable")?;
        if !backend
            .list_server_snapshots()?
            .iter()
            .any(|server| server.server_id == server_id)
        {
            return Err("MCP server not found".into());
        }
        let instructions = self
            .mcp_server_tool_settings
            .lock()
            .map_err(|_| "MCP server tool settings lock poisoned".to_string())?
            .custom_instructions_for(&account_fence, server_id)?;
        Ok(json!({
            "accountFence": account_fence,
            "serverId": server_id,
            "instructions": instructions,
        }))
    }

    fn mcp_server_custom_instructions_set(&mut self, params: &Value) -> Result<Value, String> {
        let server_id = required_string(params, "serverId")?;
        let instructions = params
            .get("instructions")
            .and_then(Value::as_str)
            .ok_or("MCP custom instructions string is required")?;
        let account_fence = self.current_turn_account_fence()?;
        let backend = self
            .mcp_dashboard_backend
            .as_ref()
            .ok_or("MCP server catalog backend is unavailable")?;
        if !backend
            .list_server_snapshots()?
            .iter()
            .any(|server| server.server_id == server_id)
        {
            return Err("MCP server not found".into());
        }
        let saved = self
            .mcp_server_tool_settings
            .lock()
            .map_err(|_| "MCP server tool settings lock poisoned".to_string())?
            .set_custom_instructions(&account_fence, server_id, instructions)?;
        Ok(json!({
            "accountFence": account_fence,
            "serverId": server_id,
            "instructions": saved,
        }))
    }

    fn mcp_server_tools_list(&self, params: &Value) -> Result<Value, String> {
        let server_id = required_string(params, "serverId")?;
        let account_fence = self.current_turn_account_fence()?;
        let backend = self
            .mcp_dashboard_backend
            .as_ref()
            .ok_or("MCP server tool backend is unavailable")?;
        let tools = backend.list_server_tools(server_id)?;
        self.project_mcp_server_tools(&account_fence, server_id, tools)
    }

    fn mcp_server_tools_toggle(&mut self, params: &Value) -> Result<Value, String> {
        let server_id = required_string(params, "serverId")?;
        let tool_name = required_string(params, "toolName")?;
        let account_fence = self.current_turn_account_fence()?;
        let backend = self
            .mcp_dashboard_backend
            .as_ref()
            .ok_or("MCP server tool backend is unavailable")?;
        let tools = backend.list_server_tools(server_id)?;
        if !tools.iter().any(|tool| tool.name == tool_name) {
            return Err("MCP tool is not present in the current server catalog".into());
        }
        self.mcp_server_tool_settings
            .lock()
            .map_err(|_| "MCP server tool settings lock poisoned".to_string())?
            .toggle(&account_fence, server_id, tool_name)?;
        self.project_mcp_server_tools(&account_fence, server_id, tools)
    }

    fn project_mcp_server_tools(
        &self,
        account_fence: &str,
        server_id: &str,
        tools: Vec<crate::mcp_auth::dashboard_mcp_auth_backend::McpServerToolSummary>,
    ) -> Result<Value, String> {
        let disabled = self
            .mcp_server_tool_settings
            .lock()
            .map_err(|_| "MCP server tool settings lock poisoned".to_string())?
            .disabled_for(account_fence, server_id)?;
        let projected = tools
            .into_iter()
            .map(|tool| {
                let is_disabled = disabled.contains(&tool.name);
                json!({
                    "name": tool.name,
                    "title": tool.title,
                    "description": tool.description,
                    "isDisabled": is_disabled,
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "accountFence": account_fence,
            "serverId": server_id,
            "tools": projected,
        }))
    }

    fn feedback_submit(&self, params: &Value) -> Result<Value, String> {
        let current_fence = match self.current_turn_account_fence() {
            Ok(value) => value,
            Err(_) => return Ok(json!({"ok":false,"code":"not-signed-in"})),
        };
        let requested_fence = match params.get("accountFence").and_then(Value::as_str) {
            Some(value) if value == current_fence => value,
            _ => return Ok(json!({"ok":false,"code":"not-signed-in"})),
        };
        let message = params.get("message").and_then(Value::as_str).unwrap_or_default();
        let submission_id = params
            .get("submissionId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let conversation_id = params.get("conversationId").and_then(Value::as_str);
        let sentry_event_ids = params
            .get("sentryEventIds")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>();
        let Some(feedback) = normalize_feedback_submit_args(
            requested_fence,
            message,
            submission_id,
            conversation_id,
            sentry_event_ids,
        ) else {
            return Ok(json!({"ok":false,"code":"invalid-feedback"}));
        };
        let Some(backend) = self.mcp_dashboard_backend.as_ref() else {
            return Ok(json!({"ok":false,"code":"unavailable"}));
        };
        let payload = json!({
            "message": feedback.message,
            "submissionId": feedback.submission_id,
            "appVersion": self.app_version,
            "platform": "android",
            "osVersion": "android",
            "conversationId": feedback.conversation_id,
            "sentryEventIds": feedback.sentry_event_ids,
        });
        match backend.submit_feedback_report(&payload) {
            Ok(()) => Ok(json!({"ok":true})),
            Err(error) => Ok(json!({"ok":false,"code":error.code()})),
        }
    }

    fn account_usage_summary(&self) -> Result<Value, String> {
        self.mcp_dashboard_backend
            .as_ref()
            .ok_or("Account usage backend is unavailable")?
            .resolve_sand_usage_summary()
    }

    fn account_cancel_sand_trial(&self) -> Result<Value, String> {
        self.mcp_dashboard_backend
            .as_ref()
            .ok_or("Account usage backend is unavailable")?
            .cancel_sand_trial()?;
        Ok(json!({"ok":true}))
    }

    fn account_dashboard_action(&self, params: &Value) -> Result<Value, String> {
        let action = required_string(params, "action")?;
        let args_value = params
            .get("args")
            .and_then(Value::as_object)
            .ok_or("Dashboard client action args object is required")?;
        let mut args = std::collections::BTreeMap::new();
        for (key, value) in args_value {
            let string_value = value
                .as_str()
                .ok_or("Dashboard client action args must contain string values")?;
            args.insert(key.clone(), string_value.to_string());
        }
        self.mcp_dashboard_backend
            .as_ref()
            .ok_or("Dashboard client action backend is unavailable")?
            .client_action(action, &args)
    }

    fn onepassword_provisioning_status(&self) -> Result<Value, String> {
        let snapshot = self
            .onepassword_provisioning
            .lock()
            .map_err(|_| "1Password provisioning owner lock poisoned".to_string())?
            .snapshot()
            .clone();
        Ok(json!({
            "phase": match snapshot.phase {
                DurableProvisioningPhase::Idle => "idle",
                DurableProvisioningPhase::Prepared => "prepared",
                DurableProvisioningPhase::Delivering => "delivering",
                DurableProvisioningPhase::OutcomeUnknown => "outcome-unknown",
                DurableProvisioningPhase::Delivered => "delivered",
            },
            "accountFence": snapshot.account_fence,
            "accountEpoch": if snapshot.account_epoch == 0 { Value::Null } else { json!(snapshot.account_epoch) },
            "requestId": snapshot.request_id,
            "operationId": snapshot.operation_id,
            "consumerReference": snapshot.consumer_reference,
            "outcomeUnknownReason": snapshot.outcome_unknown_reason,
            "requiresReconciliation": snapshot.requires_reconciliation(),
            "remoteExecutorReady": false,
            "unavailableReason": "authorized_remote_runner_onepassword_executor_not_wired",
        }))
    }

    fn onepassword_provisioning_identity(
        &self,
        params: &Value,
    ) -> Result<(String, u64, String), String> {
        let requested_fence = required_string(params, "accountFence")?;
        let current_fence = self.current_turn_account_fence()?;
        if requested_fence != current_fence {
            return Err("1Password provisioning request is fenced to a stale account".into());
        }
        let account_epoch = params
            .get("accountEpoch")
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .ok_or("1Password provisioning accountEpoch must be positive")?;
        let operation_id = required_string(params, "operationId")?.to_string();
        Ok((current_fence, account_epoch, operation_id))
    }

    fn onepassword_provisioning_begin(&mut self, params: &Value) -> Result<Value, String> {
        let (account_fence, account_epoch, operation_id) =
            self.onepassword_provisioning_identity(params)?;
        let request_id = required_string(params, "requestId")?;
        self.onepassword_provisioning
            .lock()
            .map_err(|_| "1Password provisioning owner lock poisoned".to_string())?
            .begin(
                &account_fence,
                account_epoch,
                request_id,
                &operation_id,
            )?;
        self.onepassword_provisioning_status()
    }

    fn onepassword_provisioning_mark_delivering(
        &mut self,
        params: &Value,
    ) -> Result<Value, String> {
        let (account_fence, account_epoch, operation_id) =
            self.onepassword_provisioning_identity(params)?;
        self.onepassword_provisioning
            .lock()
            .map_err(|_| "1Password provisioning owner lock poisoned".to_string())?
            .mark_delivering(&account_fence, account_epoch, &operation_id)?;
        self.onepassword_provisioning_status()
    }

    fn onepassword_provisioning_mark_outcome_unknown(
        &mut self,
        params: &Value,
    ) -> Result<Value, String> {
        let (account_fence, account_epoch, operation_id) =
            self.onepassword_provisioning_identity(params)?;
        let reason = required_string(params, "reason")?;
        self.onepassword_provisioning
            .lock()
            .map_err(|_| "1Password provisioning owner lock poisoned".to_string())?
            .mark_outcome_unknown(
                &account_fence,
                account_epoch,
                &operation_id,
                reason,
            )?;
        self.onepassword_provisioning_status()
    }

    fn onepassword_provisioning_settle_delivered(
        &mut self,
        params: &Value,
    ) -> Result<Value, String> {
        let (account_fence, account_epoch, operation_id) =
            self.onepassword_provisioning_identity(params)?;
        let consumer_reference = required_string(params, "consumerReference")?;
        self.onepassword_provisioning
            .lock()
            .map_err(|_| "1Password provisioning owner lock poisoned".to_string())?
            .settle_delivered(
                &account_fence,
                account_epoch,
                &operation_id,
                consumer_reference,
            )?;
        self.onepassword_provisioning_status()
    }

    fn onepassword_provisioning_reconcile_not_delivered(
        &mut self,
        params: &Value,
    ) -> Result<Value, String> {
        let (account_fence, account_epoch, operation_id) =
            self.onepassword_provisioning_identity(params)?;
        self.onepassword_provisioning
            .lock()
            .map_err(|_| "1Password provisioning owner lock poisoned".to_string())?
            .reconcile_not_delivered(
                &account_fence,
                account_epoch,
                &operation_id,
            )?;
        self.onepassword_provisioning_status()
    }

    fn teach_recording_status(&self) -> Result<Value, String> {
        let snapshot = self
            .teach_recording
            .lock()
            .map_err(|_| "Teach Recording owner lock poisoned".to_string())?
            .snapshot()
            .clone();
        let remote = self.remote_binding_status();
        // Generic shell/computer enrollment is not Teach Recording authority. Actual recording
        // remains unavailable until the protected binding explicitly names a bounded Teach
        // executor; this prevents UI code from smuggling ffmpeg through arbitrary shell access.
        let remote_recording_ready = remote
            .get("executors")
            .and_then(Value::as_array)
            .is_some_and(|executors| {
                executors.iter().any(|value| value.as_str() == Some("teach-recording"))
            });
        Ok(json!({
            "phase": match snapshot.phase {
                TeachRecordingPhase::Idle => "idle",
                TeachRecordingPhase::Starting => "starting",
                TeachRecordingPhase::Recording => "recording",
                TeachRecordingPhase::Stopping => "stopping",
                TeachRecordingPhase::OutcomeUnknown => "outcome-unknown",
            },
            "accountFence": snapshot.account_fence,
            "accountEpoch": if snapshot.account_epoch == 0 { Value::Null } else { json!(snapshot.account_epoch) },
            "agentId": snapshot.agent_id,
            "requestId": snapshot.request_id,
            "operationId": snapshot.operation_id,
            "startedAtMs": snapshot.started_at_ms,
            "saveRequested": snapshot.save_requested,
            "maxDurationMs": snapshot.max_duration_ms,
            "outcomeUnknownReason": snapshot.outcome_unknown_reason,
            "requiresReconciliation": snapshot.requires_reconciliation(),
            "remoteRecordingReady": remote_recording_ready,
            "unavailableReason": if remote_recording_ready {
                Value::Null
            } else {
                json!("authorized_remote_runner_teach_recording_executor_unavailable")
            },
        }))
    }

    pub fn runtime_call_control(&self) -> Arc<RuntimeCallCancellationRegistry> {
        Arc::clone(&self.runtime_call_cancellations)
    }

    fn record_product_telemetry(
        &mut self,
        message: &str,
        event: StructuredTelemetryEvent,
    ) -> Result<Value, String> {
        self.sync_structured_log_account_slot();
        let level = match event.level {
            TelemetryLevel::Info => "info",
            TelemetryLevel::Warn => "warn",
            TelemetryLevel::Error => "error",
        };
        self.structured_log_spool
            .record(level, message, event.metadata, now_ms())
            .map_err(|error| format!("failed to persist Android product telemetry: {error}"))?;
        Ok(json!({"recorded":true}))
    }

    fn record_send_settlement_telemetry(
        &mut self,
        record: &SendAcknowledgementRecord,
        outcome: &str,
        observed_at_ms: u64,
    ) {
        // Older durable journals predate Host-owned timing anchors. Never fabricate latency for
        // those records; new attempts always persist enteredAtMs before dispatch.
        if record.entered_at_ms == 0 {
            return;
        }
        let ack_ms = observed_at_ms.saturating_sub(record.entered_at_ms) as f64;
        let commit_ms = record
            .dispatch_started_at_ms
            .map(|started_at_ms| observed_at_ms.saturating_sub(started_at_ms) as f64);
        let attachment_count = record.attachments.len() as f64;

        let ack_event = send_ack_event(
            ack_ms,
            outcome,
            record.entered_at_ms as f64,
            false,
            attachment_count,
            None,
            None,
            Some(record.nonce.as_str()),
            commit_ms,
            None,
            None,
        );
        match ack_event.and_then(|event| {
            self.record_product_telemetry("sand.send_ack", event)
                .map(|_| ())
        }) {
            Ok(()) => {}
            Err(_) => self
                .edge_failures
                .report("structured-log-spill", "send-ack-telemetry", "StorageError"),
        }

        if outcome == "acked" {
            let latency_event = send_latency_event(
                ack_ms,
                commit_ms,
                attachment_count,
                false,
                None,
                None,
                None,
            );
            match latency_event.and_then(|event| {
                self.record_product_telemetry("sand.send", event)
                    .map(|_| ())
            }) {
                Ok(()) => {}
                Err(_) => self
                    .edge_failures
                    .report("structured-log-spill", "send-latency-telemetry", "StorageError"),
            }
        }
    }

    fn signin_gate_telemetry(&mut self, params: &Value) -> Result<Value, String> {
        let gate = params.get("gate").and_then(Value::as_str).filter(|value| !value.trim().is_empty());
        let consult = params.get("consult").and_then(Value::as_str).filter(|value| !value.trim().is_empty());
        let report = match (gate, consult) {
            (Some("shell"), None) => SigninGateReport::Gate("shell"),
            (Some("onboarding"), None) => SigninGateReport::Gate("onboarding"),
            (Some("sign-in"), None) => SigninGateReport::Gate("sign-in"),
            (None, Some("veto-seen")) => SigninGateReport::Consult("veto-seen"),
            (None, Some("veto-roster")) => SigninGateReport::Consult("veto-roster"),
            (None, Some("fresh")) => SigninGateReport::Consult("fresh"),
            (None, Some("fail-open-probes")) => SigninGateReport::Consult("fail-open-probes"),
            (None, Some("fail-open-deadline")) => SigninGateReport::Consult("fail-open-deadline"),
            (Some(_), None) => return Err("invalid signin telemetry gate".into()),
            (None, Some(_)) => return Err("invalid signin telemetry consult outcome".into()),
            _ => return Err("signin gate telemetry requires exactly one of gate or consult".into()),
        };
        self.account_telemetry.report_signin_gate(report);
        Ok(json!({"recorded":true}))
    }

    fn access_blocked_telemetry(&mut self, params: &Value) -> Result<Value, String> {
        let edge = required_string(params, "edge")?;
        let cause = required_string(params, "cause")?;
        let access_reason = required_string(params, "accessReason")?;
        let ms_blocked = params.get("msBlocked").and_then(Value::as_f64);
        let event = access_blocked_event(edge, cause, access_reason, ms_blocked)?;
        self.account_telemetry.report_access_blocked(
            edge, cause, access_reason, ms_blocked.map(|value| value.round().max(0.0) as u64),
        )?;
        self.record_product_telemetry("sand.access_blocked", event)
    }

    fn agent_load_telemetry(&mut self, params: &Value) -> Result<Value, String> {
        let duration_ms = params
            .get("durationMs")
            .and_then(Value::as_f64)
            .ok_or("durationMs is required")?;
        let event = agent_load_event(
            duration_ms,
            params.get("openRpcMs").and_then(Value::as_f64),
            params.get("queueMs").and_then(Value::as_f64),
            params.get("renderMs").and_then(Value::as_f64),
            params.get("cold").and_then(Value::as_bool).ok_or("cold is required")?,
            params.get("stale").and_then(Value::as_bool),
            required_string(params, "boxState")?,
            required_string(params, "outcome")?,
        )?;
        self.record_product_telemetry("sand.agent_load", event)
    }

    fn startup_lifecycle_telemetry(&mut self, params: &Value) -> Result<Value, String> {
        let event = startup_lifecycle_event(
            required_string(params, "outcome")?,
            required_string(params, "phase")?,
            params
                .get("elapsedMs")
                .and_then(Value::as_f64)
                .ok_or("elapsedMs is required")?,
            params.get("phaseElapsedMs").and_then(Value::as_f64),
            params.get("windowMs").and_then(Value::as_f64),
        )?;
        self.record_product_telemetry("sand.desktop.startup", event)
    }

    fn onboarding_step_telemetry(&mut self, params: &Value) -> Result<Value, String> {
        let expected_scope = required_string(params, "expectedAccountScope")?;
        let account_scope = self.current_sidebar_account_scope()?;
        if expected_scope != account_scope {
            return Err("onboarding telemetry account scope changed before record".into());
        }
        let step_index = params
            .get("stepIndex")
            .and_then(Value::as_i64)
            .ok_or("stepIndex is required")?;
        let event = onboarding_step_event(
            required_string(params, "step")?,
            step_index,
            required_string(params, "direction")?,
        )?;
        self.record_product_telemetry("sand.onboarding.step_viewed", event)
    }

    fn reaction_ack_telemetry(&mut self, params: &Value) -> Result<Value, String> {
        let ack_ms = params
            .get("ackMs")
            .and_then(Value::as_f64)
            .ok_or("ackMs is required")?;
        let event = reaction_ack_event(
            ack_ms,
            required_string(params, "outcome")?,
            required_string(params, "conversationId")?,
            required_string(params, "emoji")?,
        )?;
        self.record_product_telemetry("sand.reaction.ack", event)
    }

    fn agents_unreachable_telemetry(&mut self, params: &Value) -> Result<Value, String> {
        let phase = required_string(params, "phase")?;
        let is_manual = params
            .get("isManual")
            .and_then(Value::as_bool)
            .ok_or("isManual is required")?;
        let error_class = params
            .get("errorClass")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        if error_class.is_some_and(|value| value.chars().count() > 128) {
            return Err("errorClass exceeds telemetry bound".into());
        }
        let event = agents_unreachable_event(
            phase,
            is_manual,
            error_class,
            params.get("msSinceLastReached").and_then(Value::as_f64),
        )?;
        self.record_product_telemetry("sand.agents_unreachable", event)
    }

    fn event_loop_pressure_telemetry(&mut self, params: &Value) -> Result<Value, String> {
        let p50_ms = params
            .get("p50Ms")
            .and_then(Value::as_f64)
            .ok_or("p50Ms is required")?;
        let p95_ms = params
            .get("p95Ms")
            .and_then(Value::as_f64)
            .ok_or("p95Ms is required")?;
        let max_ms = params
            .get("maxMs")
            .and_then(Value::as_f64)
            .ok_or("maxMs is required")?;
        let window_ms = params
            .get("windowMs")
            .and_then(Value::as_f64)
            .ok_or("windowMs is required")?;
        let Some(event) = event_loop_pressure_event(p50_ms, p95_ms, max_ms, window_ms)? else {
            return Ok(json!({"recorded":false}));
        };
        self.record_product_telemetry("sand.desktop.event_loop", event)?;
        Ok(json!({"recorded":true}))
    }

    fn render_telemetry(&mut self, params: &Value) -> Result<Value, String> {
        let kind = required_string(params, "kind")?;
        let (message, event) = match kind {
            "ttfr" => (
                "sand.render.ttfr",
                render_ttfr_event(
                    params
                        .get("ttfrMs")
                        .and_then(Value::as_f64)
                        .ok_or("ttfrMs is required")?,
                    required_string(params, "outcome")?,
                    params.get("chunkType").and_then(Value::as_str),
                    params
                        .get("enterEpochMs")
                        .and_then(Value::as_f64)
                        .ok_or("enterEpochMs is required")?,
                    params
                        .get("isFork")
                        .and_then(Value::as_bool)
                        .ok_or("isFork is required")?,
                    params.get("traceId").and_then(Value::as_str),
                    params.get("spanId").and_then(Value::as_str),
                    params.get("clientNonce").and_then(Value::as_str),
                    params.get("conversationId").and_then(Value::as_str),
                )?,
            ),
            "stream" => (
                "sand.render.stream",
                render_stream_event(
                    required_string(params, "outcome")?,
                    params
                        .get("chunkCount")
                        .and_then(Value::as_f64)
                        .ok_or("chunkCount is required")?,
                    params
                        .get("maxGapMs")
                        .and_then(Value::as_f64)
                        .ok_or("maxGapMs is required")?,
                    params
                        .get("stalledMs")
                        .and_then(Value::as_f64)
                        .ok_or("stalledMs is required")?,
                    params
                        .get("stallCount")
                        .and_then(Value::as_f64)
                        .ok_or("stallCount is required")?,
                    params
                        .get("streamMs")
                        .and_then(Value::as_f64)
                        .ok_or("streamMs is required")?,
                    params
                        .get("isFork")
                        .and_then(Value::as_bool)
                        .ok_or("isFork is required")?,
                    params.get("traceId").and_then(Value::as_str),
                    params.get("spanId").and_then(Value::as_str),
                    params.get("clientNonce").and_then(Value::as_str),
                    params.get("conversationId").and_then(Value::as_str),
                )?,
            ),
            _ => return Err("unsupported render telemetry kind".into()),
        };
        self.record_product_telemetry(message, event)
    }

    fn recovery_telemetry(&mut self, params: &Value) -> Result<Value, String> {
        let kind = required_string(params, "kind")?;
        let (message, event) = match kind {
            "rebuild_escalation" => (
                "sand.rebuild.escalation",
                rebuild_escalation_event(
                    required_string(params, "rebuildKind")?,
                    params
                        .get("elapsedMs")
                        .and_then(Value::as_f64)
                        .ok_or("elapsedMs is required")?,
                    required_string(params, "action")?,
                    required_string(params, "escalation")?,
                )?,
            ),
            "rebuild_pending_stall" => (
                "sand.rebuild.pending_stall",
                rebuild_pending_stall_event(
                    required_string(params, "rebuildKind")?,
                    params
                        .get("elapsedMs")
                        .and_then(Value::as_f64)
                        .ok_or("elapsedMs is required")?,
                    params
                        .get("hasAck")
                        .and_then(Value::as_bool)
                        .ok_or("hasAck is required")?,
                    params
                        .get("streamAttached")
                        .and_then(Value::as_bool)
                        .ok_or("streamAttached is required")?,
                )?,
            ),
            "replica_resync" => (
                "sand.replica.resync",
                replica_resync_event(
                    required_string(params, "replica")?,
                    required_string(params, "outcome")?,
                    params
                        .get("attempts")
                        .and_then(Value::as_f64)
                        .ok_or("attempts is required")?,
                    params
                        .get("durationMs")
                        .and_then(Value::as_f64)
                        .ok_or("durationMs is required")?,
                    params
                        .get("bufferOverflowed")
                        .and_then(Value::as_bool)
                        .ok_or("bufferOverflowed is required")?,
                    params
                        .get("epochChanged")
                        .and_then(Value::as_bool)
                        .ok_or("epochChanged is required")?,
                )?,
            ),
            "send_journal_restore" => (
                "sand.send_journal.restore",
                send_journal_restore_event(
                    required_string(params, "outcome")?,
                    required_string(params, "phase")?,
                    params
                        .get("ageMs")
                        .and_then(Value::as_f64)
                        .ok_or("ageMs is required")?,
                    params
                        .get("recordCount")
                        .and_then(Value::as_f64)
                        .ok_or("recordCount is required")?,
                )?,
            ),
            "resync_completed" => {
                let failed_steps = params
                    .get("failedSteps")
                    .and_then(Value::as_array)
                    .ok_or("failedSteps is required")?
                    .iter()
                    .map(|value| value.as_str().map(str::to_string).ok_or("failedSteps must contain strings"))
                    .collect::<Result<Vec<_>, _>>()?;
                (
                    "sand.resync.completed",
                    resync_completed_event(
                        &failed_steps,
                        params
                            .get("durationMs")
                            .and_then(Value::as_f64)
                            .ok_or("durationMs is required")?,
                    )?,
                )
            }
            "recovery_action" => (
                "sand.recovery_action",
                recovery_action_event(
                    required_string(params, "action")?,
                    required_string(params, "failureClass")?,
                    params
                        .get("consecutiveIdentical")
                        .and_then(Value::as_u64)
                        .ok_or("consecutiveIdentical is required")?,
                )?,
            ),
            _ => return Err("unsupported recovery telemetry kind".into()),
        };
        self.record_product_telemetry(message, event)
    }

    fn client_failure_telemetry(&mut self, params: &Value) -> Result<Value, String> {
        let kind = required_string(params, "kind")?;
        let (message, event) = match kind {
            "client_resource" => (
                "sand.client_resource",
                client_resource_event(
                    required_string(params, "domain")?,
                    required_string(params, "operation")?,
                    required_string(params, "state")?,
                    required_string(params, "failureCode")?,
                    required_string(params, "boundary")?,
                    params.get("retryOwner").and_then(Value::as_str),
                    params.get("transportKind").and_then(Value::as_str),
                    params.get("failedMs").and_then(Value::as_f64),
                )?,
            ),
            "renderer_unhandled" => (
                "sand.renderer_unhandled",
                renderer_unhandled_event(
                    required_string(params, "handler")?,
                    required_string(params, "errorClass")?,
                    params
                        .get("occurrence")
                        .and_then(Value::as_u64)
                        .ok_or("occurrence must be a positive integer")?,
                )?,
            ),
            "renderer_boundary_caught" => (
                "sand.renderer.boundary_caught",
                renderer_boundary_caught_event(
                    required_string(params, "boundary")?,
                    required_string(params, "errorClass")?,
                    params
                        .get("occurrence")
                        .and_then(Value::as_u64)
                        .ok_or("occurrence must be a positive integer")?,
                )?,
            ),
            "entrypoint_error" => (
                "sand.entrypoint_error",
                entrypoint_error_event(
                    required_string(params, "entrypoint")?,
                    required_string(params, "surface")?,
                    required_string(params, "phase")?,
                    required_string(params, "errorClass")?,
                )?,
            ),
            "send_queued_flush" => (
                "sand.send.queued_flush",
                send_queued_flush_event(
                    required_string(params, "outcome")?,
                    params.get("queuedMs").and_then(Value::as_f64).ok_or("queuedMs is required")?,
                    params.get("queueDepth").and_then(Value::as_f64).ok_or("queueDepth is required")?,
                    params.get("cause").and_then(Value::as_str),
                )?,
            ),
            "client_persistence" => {
                let error = params.get("error").and_then(Value::as_object);
                (
                    "sand.client_persistence",
                    client_persistence_event(
                        required_string(params, "op")?,
                        required_string(params, "outcome")?,
                        required_string(params, "slice")?,
                        params.get("bytes").and_then(Value::as_f64),
                        error.and_then(|value| value.get("code")).and_then(Value::as_str),
                        error.and_then(|value| value.get("errno")).and_then(Value::as_str),
                    )?,
                )
            }
            "connector_auth_clicked" => (
                "sand.connector_auth",
                connector_auth_clicked_event(
                    params.get("serverName").and_then(Value::as_str),
                    params.get("serverId").and_then(Value::as_str),
                )?,
            ),
            _ => return Err("unsupported client-failure telemetry kind".into()),
        };
        self.record_product_telemetry(message, event)
    }

    fn sync_structured_log_account_slot(&mut self) {
        let account_slot = self
            .account
            .account_storage_scope()
            .unwrap_or_else(|| "logged-out".to_string());
        if self
            .structured_log_spool
            .rotate_account(&account_slot, &self.structured_log_counter_id)
            .is_err()
        {
            self.edge_failures
                .report("structured-log-spill", "account-rotate", "StorageError");
        }
    }

    pub fn report_edge_failure(&mut self, area: &str, leg: &str, error_class: &str) {
        self.edge_failures.report(area, leg, error_class);
        self.sync_structured_log_account_slot();
        let Some(entry) = self.edge_failures.snapshot().last().cloned() else {
            return;
        };
        let metadata = BTreeMap::from([
            ("area".to_string(), entry.area),
            ("leg".to_string(), entry.leg),
            ("error_class".to_string(), entry.error_class),
        ]);
        if self
            .structured_log_spool
            .record("error", "sand.desktop.edge_failed", metadata, now_ms())
            .is_err()
        {
            self.edge_failures
                .report("structured-log-spill", "record", "StorageError");
        }
    }

    fn schedule_account_telemetry_ship(&mut self) {
        let Some(backend) = self.mcp_dashboard_backend.clone() else {
            return;
        };
        let Ok(account_fence) = self.current_turn_account_fence() else {
            return;
        };
        let account_slot = crate::sha256::sha256_hex(account_fence.as_bytes());
        let counter_id = format!("account-telemetry:{}", &account_slot[..16.min(account_slot.len())]);
        let mut entries = self.account_telemetry.drain();
        entries.extend(self.account.drain_session_telemetry());
        let outbox = Arc::clone(&self.account_telemetry_outbox);
        let now = now_ms();
        {
            let Ok(mut owner) = outbox.lock() else {
                return;
            };
            let rotate = owner
                .as_ref()
                .is_none_or(|current| current.account_slot() != account_slot);
            if rotate {
                let Ok(opened) = DurableAccountTelemetryOutbox::open(
                    &self.account_telemetry_spill_path,
                    &account_slot,
                    &counter_id,
                    now,
                ) else {
                    return;
                };
                *owner = Some(opened);
            }
            let Some(owner) = owner.as_mut() else {
                return;
            };
            if owner.push(entries, now).is_err() || owner.batch().is_empty() {
                return;
            }
        }

        let shipping = Arc::clone(&self.account_telemetry_shipping);
        if shipping
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let live_account_fence = Arc::clone(&self.live_account_fence);
        thread::spawn(move || loop {
            let still_current = live_account_fence
                .lock()
                .ok()
                .and_then(|fence| fence.clone())
                .is_some_and(|fence| fence == account_fence);
            if !still_current {
                shipping.store(false, Ordering::Release);
                return;
            }

            let batch = match outbox.lock() {
                Ok(owner) => owner
                    .as_ref()
                    .filter(|owner| owner.account_slot() == account_slot)
                    .map(DurableAccountTelemetryOutbox::batch)
                    .unwrap_or_default(),
                Err(_) => {
                    shipping.store(false, Ordering::Release);
                    return;
                }
            };
            if batch.is_empty() {
                shipping.store(false, Ordering::Release);
                return;
            }

            let receipt = backend.submit_account_telemetry(&batch);
            let still_current = live_account_fence
                .lock()
                .ok()
                .and_then(|fence| fence.clone())
                .is_some_and(|fence| fence == account_fence);
            if !still_current {
                shipping.store(false, Ordering::Release);
                return;
            }

            let Ok(mut locked) = outbox.lock() else {
                shipping.store(false, Ordering::Release);
                return;
            };
            let Some(owner) = locked
                .as_mut()
                .filter(|owner| owner.account_slot() == account_slot)
            else {
                shipping.store(false, Ordering::Release);
                return;
            };
            match receipt {
                Ok((processed, dropped)) => {
                    if owner.settle_success(processed, dropped, now_ms()).is_err() {
                        shipping.store(false, Ordering::Release);
                        return;
                    }
                }
                Err(_) => {
                    let _ = owner.mark_ship_failed(now_ms());
                    shipping.store(false, Ordering::Release);
                    return;
                }
            }
        });
    }

    pub fn dispatch(&mut self, method: &str, params: &Value) -> Result<Value, String> {
        let result = match method {
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
            "feature.account.telemetry.snapshot" => {
                let mut snapshot = self.account_telemetry.snapshot();
                snapshot.extend(self.account.session_telemetry_snapshot());
                serde_json::to_value(snapshot)
                    .map_err(|error| format!("failed to serialize account telemetry snapshot: {error}"))
            },
            "feature.account.telemetry.protectedStoreUnavailable" => {
                self.account_telemetry
                    .report_session(SessionReport::ProtectedStoreUnavailable);
                Ok(json!({"recorded":true}))
            },
            "feature.account.telemetry.accessBlocked" => self.access_blocked_telemetry(params),
            "feature.account.telemetry.signinGate" => self.signin_gate_telemetry(params),
            "feature.telemetry.agentLoad" => self.agent_load_telemetry(params),
            "feature.telemetry.startupLifecycle" => self.startup_lifecycle_telemetry(params),
            "feature.telemetry.onboardingStep" => self.onboarding_step_telemetry(params),
            "feature.telemetry.reactionAck" => self.reaction_ack_telemetry(params),
            "feature.telemetry.agentsUnreachable" => self.agents_unreachable_telemetry(params),
            "feature.telemetry.eventLoopPressure" => self.event_loop_pressure_telemetry(params),
            "feature.telemetry.clientFailure" => self.client_failure_telemetry(params),
            "feature.telemetry.render" => self.render_telemetry(params),
            "feature.telemetry.recovery" => self.recovery_telemetry(params),
            "feature.diagnostics.edgeFailures.snapshot" => serde_json::to_value(self.edge_failures.snapshot()).map_err(|error| format!("failed to serialize edge failure snapshot: {error}")),
            "feature.diagnostics.edgeFailures.report" => {
                let area = required_string(params, "area")?;
                let leg = required_string(params, "leg")?;
                let error_class = required_string(params, "errorClass")?;
                self.edge_failures.report(&area, &leg, &error_class);
                Ok(json!({"recorded":true}))
            },
            "feature.account.fence" => Ok(json!({"accountFence":self.current_turn_account_fence()?})),
            "feature.account.sandAccess" => self.account_sand_access(),
            "feature.account.privacyMode" => Ok(self.account_privacy_mode()),
            "feature.account.teamRules" => self.account_team_rules(),
            "feature.account.prReviewPreferences" => self.account_pr_review_preferences(),
            "feature.account.dashboardAction" => self.account_dashboard_action(params),
            "feature.account.usageSummary" => self.account_usage_summary(),
            "feature.account.cancelSandTrial" => self.account_cancel_sand_trial(),
            "feature.feedback.submit" => self.feedback_submit(params),
            "feature.settings.timeZone" => self.time_zone_settings(),
            "feature.settings.timeZone.detected" => self.set_detected_time_zone(params),
            "feature.settings.timeZone.set" => self.set_time_zone_settings(params),
            "feature.settings.localToolPermission" => self.local_tool_permission(),
            "feature.settings.localToolPermission.set" => self.set_local_tool_permission(params),
            "feature.settings.processMetrics" => self.process_metrics_settings(),
            "feature.settings.processMetrics.set" => self.set_process_metrics_settings(params),
            "feature.settings.onboardingSeen" => self.onboarding_seen(),
            "feature.settings.onboardingSeen.set" => self.set_onboarding_seen(params),
            "feature.settings.autoReviewInstructions" => self.auto_review_instructions(),
            "feature.settings.autoReviewInstructions.set" => self.set_auto_review_instructions(params),
            "feature.remote.binding.status" => Ok(self.remote_binding_status()),
            "feature.teachRecording.status" => self.teach_recording_status(),
            "feature.onePassword.provisioning.status" => self.onepassword_provisioning_status(),
            "feature.onePassword.provisioning.begin" => self.onepassword_provisioning_begin(params),
            "feature.onePassword.provisioning.markDelivering" => self.onepassword_provisioning_mark_delivering(params),
            "feature.onePassword.provisioning.markOutcomeUnknown" => self.onepassword_provisioning_mark_outcome_unknown(params),
            "feature.onePassword.provisioning.settleDelivered" => self.onepassword_provisioning_settle_delivered(params),
            "feature.onePassword.provisioning.reconcileNotDelivered" => self.onepassword_provisioning_reconcile_not_delivered(params),
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
            "feature.models.available" => self.available_model_catalog(),
            "feature.mcp.serverCatalog" => self.mcp_server_catalog(),
            "feature.mcp.teamPopularity" => self.mcp_team_popularity(),
            "feature.mcp.serverInstructions.get" => self.mcp_server_custom_instructions(params),
            "feature.mcp.serverInstructions.set" => self.mcp_server_custom_instructions_set(params),
            "feature.mcp.serverTools.list" => self.mcp_server_tools_list(params),
            "feature.mcp.serverTools.toggle" => self.mcp_server_tools_toggle(params),
            "feature.mcp.oauthComplete" => self.mcp_oauth_complete(params),
            "feature.mcp.authenticate" => self.mcp_authenticate(params),
            "feature.mcp.serverAccounts.list" => self.mcp_server_accounts(params),
            "feature.mcp.account.logout" => self.mcp_account_logout(params),
            "feature.mcp.account.rename" => self.mcp_account_rename(params),
            "feature.mcp.account.remove" => self.mcp_account_remove(params),
            "feature.mcp.authWatch.register" => self.mcp_auth_watch_register(params),
            "feature.mcp.authWatch.poll" => self.mcp_auth_watch_poll(params),
            "feature.mcp.authWatch.settle" => self.mcp_auth_watch_settle(params),
            "feature.mcp.authWatch.cancel" => self.mcp_auth_watch_cancel(params),
            "feature.mcp.authWatch.snapshot" => self.mcp_auth_watch_snapshot(),
            "feature.auth.profileUpdate" => self.account_profile_update(params),
            "feature.auth.logout" => self.account_logout(),
            "feature.telemetry.processMetrics.observe" => self.process_metrics_observe(params),
            "feature.telemetry.processMetrics.recent" => Ok(Value::Array(self.process_metrics_recent.iter().cloned().collect())),
            "feature.telemetry.processCrash.record" => self.process_crash_record(params),
            "feature.telemetry.session.heartbeat" => {
                let observed_at_ms = required_u64(params, "observedAtMs")?;
                self.unclean_exit
                    .heartbeat(observed_at_ms)
                    .map_err(|error| format!("failed to persist Android Host session heartbeat: {error}"))?;
                Ok(json!({"accepted":true,"observedAtMs":observed_at_ms}))
            },
            "feature.telemetry.processCrash.recent" => Ok(Value::Array(self.process_crash_recent.iter().cloned().collect())),
            "feature.agent.diskPressure.observe" => self.agent_disk_pressure_observe(params),
            "feature.agent.diskPressure.record" => self.agent_disk_pressure_record(params),
            "feature.agent.turn.reconcile" => self.agent_turn_reconcile(params),
            "feature.agent.subagent.tool" => self.agent_subagent_tool(params),
            "feature.agent.subagent.reconcile" => self.agent_subagent_reconcile(params),
            "feature.agent.asyncTasks" => self.agent_async_tasks(params),
            "feature.agent.avatar.generate" => self.agent_avatar_generate(params),
            "feature.agent.rosterMutation" => self.agent_roster_mutation(params),
            "feature.agent.sidebarSections" => self.agent_sidebar_sections(),
            "feature.agent.upgradeQuiesce" => self.agent_upgrade_quiesce(params),
            "feature.call.create" => self.human_call_create(params),
            "feature.call.list" => self.human_call_list(params),
            "feature.call.lease" => self.human_call_lease(params),
            "feature.call.transition" => self.human_call_transition(params),
            "feature.call.media.update" => self.human_call_media_update(params),
            "feature.call.signal.send" => self.human_call_signal_send(params),
            "feature.call.signal.list" => self.human_call_signal_list(params),
            "feature.call.reconcile" => self.human_call_reconcile(params),
            "feature.automation.upsert" => self.automation_upsert(params),
            "feature.automation.list" => self.automation_list(),
            "feature.automation.listForAgent" => self.automation_list_for_agent(params),
            "feature.automation.setAgentEnabled" => self.automation_set_agent_enabled(params),
            "feature.automation.start" => self.automation_start(params),
            "feature.automation.advanceStep" => self.automation_advance_step(params),
            "feature.automation.awaitApproval" => self.automation_await_approval(params),
            "feature.automation.resolveApproval" => self.automation_resolve_approval(params),
            "feature.automation.cancel" => self.automation_cancel(params),
            "feature.automation.settle" => self.automation_settle(params),
            "feature.automation.snapshot" => self.automation_snapshot(params),
            "getTrays" => serde_json::to_value(self.trays.list())
                .map_err(|error| format!("failed to project trays: {error}")),
            "dismissTray" => {
                let id = required_string(params, "id")?;
                Ok(json!({"dismissed":self.trays.dismiss(id)}))
            }
            "clearTrays" => {
                self.trays.clear_all();
                Ok(json!({"cleared":true}))
            }
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
                    .create_with_persona(
                        name,
                        description,
                        params.get("avatarShape"),
                        params.get("avatarColor"),
                    )
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
            "feature.sendAcknowledgement.begin" => self.send_acknowledgement_begin(params),
            "feature.sendAcknowledgement.queue" => self.send_acknowledgement_queue(params),
            "feature.sendAcknowledgement.cancelQueued" => self.send_acknowledgement_cancel_queued(params),
            "feature.sendAcknowledgement.dispatch" => self.send_acknowledgement_dispatch(params),
            "feature.sendAcknowledgement.retryPrepare" => self.send_acknowledgement_retry_prepare(params),
            "feature.sendAcknowledgement.retry" => self.send_acknowledgement_retry(params),
            "feature.sendAcknowledgement.clearApprovals" => self.send_acknowledgement_clear_approvals(params),
            "feature.sendAcknowledgement.snapshot" => self.send_acknowledgement_snapshot(params),
            "feature.sendAcknowledgement.restoredSnapshot" => self.send_acknowledgement_restored_snapshot(),
            "feature.composerDraft.read" => self.composer_draft_read(params),
            "feature.composerDraft.write" => self.composer_draft_write(params),
            "feature.composerDraft.clearIfCurrent" => self.composer_draft_clear_if_current(params),
            "feature.composerDraft.attachment.begin" => self.composer_draft_attachment_begin(params),
            "feature.composerDraft.attachment.append" => self.composer_draft_attachment_append(params),
            "feature.composerDraft.attachment.finish" => self.composer_draft_attachment_finish(params),
            "feature.composerDraft.attachment.abort" => self.composer_draft_attachment_abort(params),
            "feature.composerDraft.attachment.remove" => self.composer_draft_attachment_remove(params),
            "feature.composerDraft.attachment.readRange" => self.composer_draft_attachment_read_range(params),
            "feature.widget.respond" => self.widget_respond(params),
            "feature.widget.dismiss" => self.widget_dismiss(params),
            "feature.receive" => self.feature_receive(),
            "feature.interrupt" => self.feature_interrupt(params),
            "feature.approval.resolve" => self.feature_approval_resolve(params),
            "feature.marketplace.browse" => self.marketplace_browse(params),
            "feature.marketplace.release" => self.marketplace_release(params),
            "feature.plugin.install" => self.plugin_install(params),
            "feature.plugin.update" => self.plugin_update(params),
            "feature.plugin.uninstall" => self.plugin_uninstall(params),
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
            "feature.transcript.page" => self.transcript_page(params),
            "feature.search.agentContent" => self.agent_content_search(params),
            "feature.search.global" => self.global_content_search(params),
            "feature.contentSearch.messages" => self.content_search_messages(params),
            "feature.contentSearch.media" => self.content_search_media(params),
            "feature.assistant.projection" => self.assistant_projection(),
            "feature.assistant.markRead" => self.assistant_mark_read(),
            "feature.webauthn.registerProvider" => self.webauthn_register_provider(),
            "feature.webauthn.unregisterProvider" => self.webauthn_unregister_provider(params),
            "feature.webauthn.pollRequest" => self.webauthn_poll_request(params),
            "feature.webauthn.submitResponses" => self.webauthn_submit_responses(params),
            "feature.webauthn.requestCeremony" => self.webauthn_request_ceremony(params),
            "platform.request" => self.platform_request(params),
            other => Err(format!("unknown host method {other}")),
        };
        self.sync_structured_log_account_slot();
        self.schedule_account_telemetry_ship();
        result
    }


    fn global_content_search(&mut self, params: &Value) -> Result<Value, String> {
        let query = params.get("query").and_then(Value::as_str).unwrap_or_default();
        let message_limit = params.get("messageLimit").and_then(Value::as_u64).unwrap_or(50).min(50) as usize;
        let media_limit = params.get("mediaLimit").and_then(Value::as_u64).unwrap_or(50).min(50) as usize;
        let account_slot = self.account.account_storage_scope();
        let account_fence = self.current_turn_account_fence()?;
        let entries = self.transcript.lock().map_err(|_| "transcript lock poisoned".to_string())?.get_transcript().into_iter().filter(|entry| {
            let slot_ok = entry.get("accountSlot").and_then(Value::as_str).map(|slot| Some(slot) == account_slot.as_deref()).unwrap_or(true);
            let fence_ok = entry.get("accountFence").and_then(Value::as_str).map(|fence| account_fence.as_str() == fence).unwrap_or(true);
            slot_ok && fence_ok
        }).collect::<Vec<_>>();
        let (messages, media) = self.content_search.search(&entries, query, message_limit, media_limit)?;
        let messages = messages.into_iter().map(|item| json!({"agentId":item.agent_id,"entryId":item.entry_id,"role":item.role,"timestampMs":item.timestamp_ms,"snippet":item.snippet})).collect::<Vec<_>>();
        let media = media.into_iter().map(|item| json!({"agentId":item.agent_id,"entryId":item.entry_id,"fileName":item.file_name,"ext":item.ext,"mime":item.mime,"kind":format!("{:?}",item.kind).to_ascii_lowercase(),"timestampMs":item.timestamp_ms,"width":item.width,"height":item.height})).collect::<Vec<_>>();
        Ok(json!({"ready":self.content_search.is_ready(),"messages":messages,"media":media}))
    }

    fn agent_content_search(&self, params: &Value) -> Result<Value, String> {
        let _account_fence = self.current_turn_account_fence()?;
        use crate::extensions::content_search::agent_content_search::{
            find_agent_content_matches, AgentContentRole, SearchableTranscriptEntry,
            SearchableTranscriptEntryKind, AGENT_CONTENT_SEARCH_MAX_MATCHES_PER_AGENT,
        };

        let agent_id = required_string(params, "agentId")?.trim().to_string();
        let query = required_string(params, "query")?;
        let normalized_query = query
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        if normalized_query.is_empty() {
            return Ok(json!({"matches":[]}));
        }
        let requested_limit = params
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(AGENT_CONTENT_SEARCH_MAX_MATCHES_PER_AGENT as u64);
        let limit = usize::try_from(requested_limit)
            .map_err(|_| "agent content search limit is out of range".to_string())?
            .min(AGENT_CONTENT_SEARCH_MAX_MATCHES_PER_AGENT);
        if limit == 0 {
            return Ok(json!({"matches":[]}));
        }

        let entries = self
            .transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?
            .get_transcript()
            .into_iter()
            .filter(|entry| entry.get("agentId").and_then(Value::as_str) == Some(agent_id.as_str()))
            .filter_map(|entry| {
                let id = entry.get("id")?.as_str()?.trim();
                if id.is_empty() {
                    return None;
                }
                let kind = match entry.get("kind").and_then(Value::as_str).unwrap_or_default() {
                    "message" => SearchableTranscriptEntryKind::Message {
                        content: entry
                            .get("content")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        role: if entry.get("role").and_then(Value::as_str) == Some("user") {
                            AgentContentRole::User
                        } else {
                            AgentContentRole::Assistant
                        },
                    },
                    "send-message" => {
                        let message = entry.get("message").and_then(Value::as_object);
                        SearchableTranscriptEntryKind::SendMessage {
                            message_type: message
                                .and_then(|value| value.get("type"))
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            content: message
                                .and_then(|value| value.get("content"))
                                .and_then(Value::as_str)
                                .map(str::to_string),
                        }
                    }
                    "notice" => SearchableTranscriptEntryKind::Notice {
                        text: entry
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    },
                    _ => SearchableTranscriptEntryKind::Other,
                };
                let hidden_outbound_agent_peer_message = entry
                    .get("hiddenOutboundAgentPeerMessage")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    || (entry.get("hidden").and_then(Value::as_bool).unwrap_or(false)
                        && entry.get("toAgent").is_some());
                Some(SearchableTranscriptEntry {
                    id: id.to_string(),
                    kind,
                    timestamp_ms: entry.get("timestampMs").and_then(Value::as_f64),
                    hidden_outbound_agent_peer_message,
                })
            })
            .collect::<Vec<_>>();

        let matches = find_agent_content_matches(&entries, &normalized_query, limit)
            .into_iter()
            .map(|item| {
                json!({
                    "entryId":item.entry_id,
                    "role":match item.role {
                        AgentContentRole::User => "user",
                        AgentContentRole::Assistant => "assistant",
                    },
                    "timestampMs":item.timestamp_ms,
                    "snippet":item.snippet,
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({"matches":matches}))
    }

    fn transcript_page(&self, params: &Value) -> Result<Value, String> {
        let agent_id = required_string(params, "agentId")?;
        let limit = params
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(200);
        let limit = usize::try_from(limit)
            .map_err(|_| "transcript page limit is out of range".to_string())?;
        let before_seq = params
            .get("beforeSeq")
            .filter(|value| !value.is_null())
            .and_then(Value::as_u64)
            .map(usize::try_from)
            .transpose()
            .map_err(|_| "transcript page cursor is out of range".to_string())?;
        let (entries, next_before_seq) = self
            .transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?
            .page_for_agent(agent_id, before_seq, limit)
            .map_err(|error| error.to_string())?;
        Ok(json!({
            "entries": entries,
            "nextBeforeSeq": next_before_seq,
        }))
    }

    fn content_search_messages(&self, params: &Value) -> Result<Value, String> {
        let _account_fence = self.current_turn_account_fence()?;
        let query = required_string(params, "query")?;
        let limit = params
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(50);
        let limit = usize::try_from(limit)
            .map_err(|_| "content search limit is out of range".to_string())?;
        let matches = self
            .transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?
            .search_content(query, limit)
            .map_err(|error| error.to_string())?;
        serde_json::to_value(matches)
            .map_err(|error| format!("failed to serialize content search result: {error}"))
    }

    fn content_search_media(&self, params: &Value) -> Result<Value, String> {
        let _account_fence = self.current_turn_account_fence()?;
        let query = params
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let limit = params
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(50);
        let limit = usize::try_from(limit)
            .map_err(|_| "media search limit is out of range".to_string())?;
        let matches = self
            .transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?
            .search_media(query, limit)
            .map_err(|error| error.to_string())?;
        serde_json::to_value(matches)
            .map_err(|error| format!("failed to serialize media search result: {error}"))
    }

    fn agent_async_tasks(&self, params: &Value) -> Result<Value, String> {
        let parent_agent_id = required_string(params, "id")?;
        let account_fence = self.current_turn_account_fence()?;
        let records = self
            .subagent_owner
            .lock()
            .map_err(|_| "durable subagent owner lock poisoned".to_string())?
            .list_running_for_parent(parent_agent_id);
        let live_tasks = records
            .into_iter()
            .filter(|record| record.account_fence == account_fence)
            .map(|record| AsyncTask {
                kind: "subagent".into(),
                id: record.subagent_id,
                label: record.title,
                status: "running".into(),
                started_at_ms: record.started_at_ms,
                detail: Some(record.box_id),
                subagent_type: Some(record.subagent_type),
            })
            .collect::<Vec<_>>();
        self.pending_wake_store.prune_stale_for(
            &account_fence,
            parent_agent_id,
            PENDING_WAKE_STALE_MAX_AGE_MS,
            now_ms(),
        );
        let durable = self
            .pending_wake_store
            .list_pending_for(&account_fence, parent_agent_id);
        serde_json::to_value(merge_async_tasks(&live_tasks, &durable))
            .map_err(|error| format!("failed to serialize async task projection: {error}"))
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
        let lifecycle = self
            .turn_lifecycle
            .lock()
            .map_err(|_| "turn lifecycle lock poisoned".to_string())?;

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
                    let waiting = account_fence.as_deref().and_then(|fence| {
                        lifecycle.awaiting_user_projection(fence, &agent.id)
                    });
                    match waiting {
                        Some(waiting) => {
                            object.insert(
                                "updatedAt".into(),
                                Value::from(agent.updated_at.max(waiting.updated_at_ms)),
                            );
                            object.insert(
                                "awaitingUserResponse".into(),
                                json!({
                                    "reason":waiting.reason,
                                    "requestId":waiting.request_id,
                                    "operationId":waiting.operation_id,
                                    "turnGeneration":waiting.turn_generation,
                                    "recoveryRequired":waiting.recovery_required,
                                }),
                            );
                        }
                        None => {
                            object.insert("awaitingUserResponse".into(), Value::Null);
                        }
                    }
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
        let raw_scopes = params
            .get("scopes")
            .and_then(Value::as_array)
            .ok_or("messaging scopes array is required")?;
        let supported_scopes = BTreeSet::from([
            "messaging",
            "calls",
            "blobsRead",
            "blobsWrite",
            "payments",
            "miniApps",
        ]);
        let scopes = raw_scopes
            .iter()
            .map(|scope| {
                scope
                    .as_str()
                    .filter(|scope| supported_scopes.contains(scope))
                    .map(str::to_string)
                    .ok_or_else(|| "messaging access request contains an unsupported scope".to_string())
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        if !scopes.contains("messaging") {
            return Err("messaging scope is required".into());
        }
        let (actor_id, mutation) = self.current_messaging_identity()?;
        let account_fence = self.current_turn_account_fence()?;
        self.messaging_access = Some(AndroidMessagingAccessGrant {
            account_fence,
            actor_id: actor_id.clone(),
            device_id: requested_device.clone(),
            session_id: requested_session.clone(),
            scopes: scopes.clone(),
        });
        Ok(with_account_session_mutation(
            json!({
                "status":"available",
                "actorId":actor_id,
                "sessionId":requested_session,
                "deviceId":requested_device,
                "scopes":scopes.into_iter().collect::<Vec<_>>(),
                "protocolVersion":2
            }),
            mutation,
        ))
    }

    fn messaging_execute(&mut self, params: &Value) -> Result<Value, String> {
        let (actor_id, mutation) = self.current_messaging_identity()?;
        let account_fence = self.current_turn_account_fence()?;
        let envelope = params
            .get("envelope")
            .and_then(Value::as_object)
            .ok_or("messaging envelope object is required")?;
        let context = envelope
            .get("context")
            .and_then(Value::as_object)
            .ok_or("messaging envelope context object is required")?;
        let context_actor = context
            .get("actorId")
            .and_then(Value::as_str)
            .ok_or("messaging actorId is required")?;
        let context_device = context
            .get("deviceId")
            .and_then(Value::as_str)
            .ok_or("messaging deviceId is required")?;
        let context_session = context
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or("messaging sessionId is required")?;
        if context_actor != actor_id {
            return Err("messaging actor does not match the authenticated account".into());
        }
        let required_scope = required_android_messaging_scope(envelope.get("command"));
        self.messaging_access
            .as_ref()
            .ok_or("messaging access grant has not been issued")?
            .authorize(
                &account_fence,
                &actor_id,
                Some(context_device),
                Some(context_session),
                required_scope,
            )?;

        let command = envelope.get("command").and_then(Value::as_object);
        // Acknowledgement telemetry is owned at the canonical Host mutation boundary. Compose may
        // optimistically project UI state, but it cannot settle or retry messaging side effects.
        let reaction_telemetry = command
            .filter(|command| command.get("type").and_then(Value::as_str) == Some("setReaction"))
            .and_then(|command| {
                Some((
                    command.get("conversationId")?.as_str()?.to_string(),
                    command
                        .get("reaction")?
                        .get("reaction")?
                        .as_str()?
                        .to_string(),
                ))
            });
        let send_telemetry = command
            .filter(|command| command.get("type").and_then(Value::as_str) == Some("sendMessage"))
            .and_then(|command| {
                let conversation_id = command.get("conversationId")?.as_str()?.to_string();
                let client_nonce = command
                    .get("clientMessageId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let content_type = command
                    .get("content")
                    .and_then(Value::as_object)
                    .and_then(|content| content.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("text");
                let attachment_count = if matches!(
                    content_type,
                    "photo" | "video" | "animation" | "audio" | "voice" | "document"
                ) {
                    1.0
                } else {
                    0.0
                };
                Some((
                    conversation_id,
                    client_nonce,
                    attachment_count,
                    context
                        .get("sentAtMs")
                        .and_then(Value::as_f64)
                        .unwrap_or_else(|| now_ms() as f64),
                ))
            });

        let started_at = Instant::now();
        let execute_result = {
            let mut messaging = self
                .messaging
                .lock()
                .map_err(|_| "canonical Android messaging owner lock poisoned".to_string())?;
            messaging.execute(params, &actor_id, i64::try_from(now_ms()).unwrap_or(i64::MAX))
        };
        let elapsed_ms = started_at.elapsed().as_secs_f64() * 1_000.0;

        if let Some((conversation_id, client_nonce, attachment_count, enter_epoch_ms)) =
            send_telemetry.as_ref()
        {
            let outcome = if execute_result.is_ok() { "acked" } else { "failed" };
            let ack_result = send_ack_event(
                elapsed_ms,
                outcome,
                *enter_epoch_ms,
                false,
                *attachment_count,
                None,
                None,
                client_nonce.as_deref(),
                None,
                Some(conversation_id.as_str()),
                None,
            )
            .and_then(|event| {
                self.record_product_telemetry("sand.send_ack", event)
                    .map(|_| ())
            });
            if ack_result.is_err() {
                self.edge_failures.report(
                    "structured-log-spill",
                    "send-ack-record",
                    "TelemetryError",
                );
            }
            if execute_result.is_ok() {
                let latency_result = send_latency_event(
                    elapsed_ms,
                    None,
                    *attachment_count,
                    false,
                    None,
                    None,
                    Some(conversation_id.as_str()),
                )
                .and_then(|event| {
                    self.record_product_telemetry("sand.send", event)
                        .map(|_| ())
                });
                if latency_result.is_err() {
                    self.edge_failures.report(
                        "structured-log-spill",
                        "send-latency-record",
                        "TelemetryError",
                    );
                }
            }
        }

        let result = execute_result?;
        if let Some((conversation_id, emoji)) = reaction_telemetry {
            let telemetry_result = reaction_ack_event(
                elapsed_ms,
                "landed",
                &conversation_id,
                &emoji,
            )
            .and_then(|event| {
                self.record_product_telemetry("sand.reaction.ack", event)
                    .map(|_| ())
            });
            if telemetry_result.is_err() {
                self.edge_failures.report(
                    "structured-log-spill",
                    "reaction-ack-record",
                    "TelemetryError",
                );
            }
        }

        self.report_signin_poll_result(&result);
        Ok(with_account_session_mutation(result, mutation))
    }
    fn messaging_blob_read(&mut self, params: &Value) -> Result<Value, String> {
        let (actor_id, mutation) = self.current_messaging_identity()?;
        let account_fence = self.current_turn_account_fence()?;
        self.messaging_access
            .as_ref()
            .ok_or("messaging access grant has not been issued")?
            .authorize(&account_fence, &actor_id, None, None, "blobsRead")?;
        let result = self
            .messaging
            .lock()
            .map_err(|_| "canonical Android messaging owner lock poisoned".to_string())?
            .read_blob_range(params, &actor_id)?;
        Ok(with_account_session_mutation(result, mutation))
    }

    fn current_sidebar_account_scope(&self) -> Result<String, String> {
        if self.mode == AndroidHostMode::Test {
            return Ok("account-device:test:android".to_string());
        }
        #[cfg(feature = "ci-account-session-import")]
        if let Some(identity) = self.ci_session_identity.as_ref() {
            if !self.logged_in {
                return Err("Sign in to Fabushi to use account-scoped capabilities.".into());
            }
            let material = format!("{}\n{}", identity.session_id, identity.device_id);
            return Ok(format!(
                "account-device:ci:{}",
                crate::sha256::sha256_hex(material.as_bytes())
            ));
        }
        self.account
            .account_storage_scope()
            .ok_or_else(|| "Sign in to Fabushi to use account-scoped capabilities.".into())
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
        if let Some(message) = self.account_binding_refusal.clone() {
            return Ok(with_account_session_mutation(
                json!({"loggedIn":false,"user":Value::Null,"errorMessage":message}),
                Some(AccountSessionMutation::Clear),
            ));
        }
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
        let logged_in = status.get("loggedIn").and_then(Value::as_bool) == Some(true);
        if !logged_in {
            return Ok(project_fabushi_sand_access(false));
        }
        match self.mode {
            AndroidHostMode::Test => Ok(project_fabushi_sand_access(true)),
            AndroidHostMode::Production => Ok(self
                .mcp_dashboard_backend
                .as_ref()
                .and_then(|backend| backend.resolve_sand_access_status().ok())
                .unwrap_or_else(|| project_fabushi_sand_access(false))),
        }
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

    fn account_pr_review_preferences(&mut self) -> Result<Value, String> {
        let status = self.account_status()?;
        let session_mutation = status.get("_accountSessionMutation").cloned();
        let mut preferences = if status.get("loggedIn").and_then(Value::as_bool) == Some(true) {
            match self.mode {
                AndroidHostMode::Test => json!({"user":Value::Null,"team":Value::Null}),
                AndroidHostMode::Production => self
                    .mcp_dashboard_backend
                    .as_ref()
                    .ok_or("PR review preference backend is unavailable")?
                    .resolve_sand_pr_review_preferences()?,
            }
        } else {
            json!({"user":Value::Null,"team":Value::Null})
        };
        if let (Some(mutation), Some(object)) = (session_mutation, preferences.as_object_mut()) {
            object.insert("_accountSessionMutation".into(), mutation);
        }
        Ok(preferences)
    }

    fn set_detected_time_zone(&mut self, params: &Value) -> Result<Value, String> {
        let detected = required_string(params, "detectedTimeZone")?.trim();
        if detected.is_empty()
            || detected.len() > 128
            || !detected
                .chars()
                .all(|value| value.is_ascii_alphanumeric() || matches!(value, '/' | '_' | '-' | '+'))
        {
            return Err("detectedTimeZone is invalid".into());
        }
        *self
            .detected_time_zone
            .lock()
            .map_err(|_| "Android detected time zone lock poisoned".to_string())? =
            Some(detected.to_string());
        Ok(json!({"detectedTimeZone": detected}))
    }

    fn effective_time_zone(&self) -> Result<Option<String>, String> {
        let override_zone = self
            .user_settings
            .lock()
            .map_err(|_| "Android user settings lock poisoned".to_string())?
            .time_zone_override()
            .map(str::to_string);
        if override_zone.is_some() {
            return Ok(override_zone);
        }
        self.detected_time_zone
            .lock()
            .map_err(|_| "Android detected time zone lock poisoned".to_string())
            .map(|value| value.clone())
    }

    fn time_zone_settings(&self) -> Result<Value, String> {
        let settings = self
            .user_settings
            .lock()
            .map_err(|_| "Android user settings lock poisoned".to_string())?;
        Ok(json!({"overrideTimeZone": settings.time_zone_override()}))
    }

    fn set_time_zone_settings(&mut self, params: &Value) -> Result<Value, String> {
        let value = match params.get("overrideTimeZone") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) => Some(value.as_str()),
            Some(_) => return Err("overrideTimeZone must be a string or null".into()),
        };
        let mut settings = self
            .user_settings
            .lock()
            .map_err(|_| "Android user settings lock poisoned".to_string())?;
        let saved = settings
            .set_time_zone_override(value)
            .map_err(|error| format!("failed to persist Android time zone override: {error}"))?;
        Ok(json!({"overrideTimeZone": saved}))
    }

    fn onboarding_seen(&self) -> Result<Value, String> {
        let account_scope = self.current_sidebar_account_scope()?;
        let has_seen = self
            .user_settings
            .lock()
            .map_err(|_| "canonical Android user settings lock poisoned".to_string())?
            .onboarding_seen(&account_scope);
        Ok(json!({
            "accountScope": account_scope,
            "hasSeenOnboarding": has_seen,
        }))
    }

    fn set_onboarding_seen(&mut self, params: &Value) -> Result<Value, String> {
        let value = params
            .get("hasSeenOnboarding")
            .and_then(Value::as_bool)
            .ok_or("hasSeenOnboarding boolean is required")?;
        let expected_scope = required_string(params, "expectedAccountScope")?;
        let account_scope = self.current_sidebar_account_scope()?;
        if expected_scope != account_scope {
            return Err("onboarding account scope changed before mutation".into());
        }
        let saved = self
            .user_settings
            .lock()
            .map_err(|_| "canonical Android user settings lock poisoned".to_string())?
            .set_onboarding_seen(&account_scope, value)
            .map_err(|error| format!("failed to persist Android onboarding state: {error}"))?;
        Ok(json!({
            "accountScope": account_scope,
            "hasSeenOnboarding": saved,
        }))
    }

    fn sync_local_tool_permission_ceiling(&self) -> Result<(), String> {
        let ceiling = match self.mode {
            AndroidHostMode::Test => None,
            AndroidHostMode::Production => self
                .mcp_dashboard_backend
                .as_ref()
                .and_then(|backend| backend.resolve_local_tool_permission_ceiling()),
        };
        self.user_settings
            .lock()
            .map_err(|_| "Android user settings lock poisoned".to_string())?
            .set_local_tool_permission_ceiling(ceiling)
            .map_err(|error| format!("failed to persist Android local tool permission ceiling: {error}"))?;
        Ok(())
    }

    fn local_tool_permission(&self) -> Result<Value, String> {
        self.sync_local_tool_permission_ceiling()?;
        let settings = self
            .user_settings
            .lock()
            .map_err(|_| "Android user settings lock poisoned".to_string())?;
        Ok(json!({
            "permission": settings.local_tool_permission().as_str(),
            "preference": settings.local_tool_permission_preference().as_str(),
            "ceiling": settings.local_tool_permission_ceiling().map(|value| value.as_str()),
        }))
    }

    fn set_local_tool_permission(&mut self, params: &Value) -> Result<Value, String> {
        self.sync_local_tool_permission_ceiling()?;
        let requested = required_string(params, "permission")?;
        let mut settings = self
            .user_settings
            .lock()
            .map_err(|_| "Android user settings lock poisoned".to_string())?;
        settings
            .set_local_tool_permission(requested)
            .map_err(|error| format!("failed to persist Android local tool permission: {error}"))?;
        Ok(json!({
            "permission": settings.local_tool_permission().as_str(),
            "preference": settings.local_tool_permission_preference().as_str(),
            "ceiling": settings.local_tool_permission_ceiling().map(|value| value.as_str()),
        }))
    }

    fn process_metrics_settings(&self) -> Result<Value, String> {
        let enabled = self
            .user_settings
            .lock()
            .map_err(|_| "Android user settings lock poisoned".to_string())?
            .process_metrics_local_enabled();
        Ok(json!({
            "localEnabled": enabled,
            "backendReportingEnabled": false,
        }))
    }

    fn set_process_metrics_settings(&mut self, params: &Value) -> Result<Value, String> {
        let local_enabled = params
            .get("localEnabled")
            .and_then(Value::as_bool)
            .ok_or("process metrics localEnabled boolean is required")?;
        let enabled = self
            .user_settings
            .lock()
            .map_err(|_| "Android user settings lock poisoned".to_string())?
            .set_process_metrics_local_enabled(local_enabled)
            .map_err(|error| format!("failed to persist Android process metrics setting: {error}"))?;
        if !enabled {
            self.process_metrics.reset();
            self.process_metrics_recent.clear();
        }
        Ok(json!({
            "localEnabled": enabled,
            "backendReportingEnabled": false,
        }))
    }

    fn auto_review_instructions(&mut self) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let snapshot = self
            .auto_review_settings
            .lock()
            .map_err(|_| "auto-review settings lock poisoned".to_string())?
            .snapshot(&account_fence)?;
        serde_json::to_value(snapshot)
            .map_err(|error| format!("failed to serialize auto-review settings: {error}"))
    }

    fn set_auto_review_instructions(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let expected_revision = required_u64(params, "expectedRevision")?;
        let instructions = params
            .get("instructions")
            .cloned()
            .ok_or_else(|| "auto-review instructions are required".to_string())
            .and_then(|value| {
                serde_json::from_value::<AutoReviewInstructions>(value)
                    .map_err(|error| format!("auto-review instructions are invalid: {error}"))
            })?;
        let snapshot = self
            .auto_review_settings
            .lock()
            .map_err(|_| "auto-review settings lock poisoned".to_string())?
            .set(&account_fence, expected_revision, instructions)?;
        serde_json::to_value(snapshot)
            .map_err(|error| format!("failed to serialize auto-review settings: {error}"))
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

    fn human_call_create(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let (creator_id, _) = self.current_messaging_identity()?;
        let participants = params
            .get("participantIds")
            .and_then(Value::as_array)
            .ok_or("participantIds array is required")?
            .iter()
            .map(|value| value.as_str().ok_or("participantIds must contain strings").map(str::to_string))
            .collect::<Result<Vec<_>, _>>()?;
        let session = self.human_calls.create(
            required_string(params, "callId")?,
            required_string(params, "requestId")?,
            required_string(params, "scopeId")?,
            &creator_id,
            participants,
            &account_fence,
            now_ms(),
        )?;
        serde_json::to_value(session).map_err(|error| error.to_string())
    }

    fn human_call_list(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
        serde_json::to_value(self.human_calls.list(
            required_string(params, "scopeId")?,
            &account_fence,
            limit,
        ))
        .map_err(|error| error.to_string())
    }

    fn human_call_lease(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let (user_id, _) = self.current_messaging_identity()?;
        let lease = self.human_calls.lease(
            required_string(params, "callId")?,
            &account_fence,
            required_u64(params, "generation")?,
            &user_id,
            &self.device_id,
            params.get("allowClaim").and_then(Value::as_bool).unwrap_or(false),
        )?;
        serde_json::to_value(lease).map_err(|error| error.to_string())
    }

    fn human_call_transition(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let session = self.human_calls.transition(
            required_string(params, "callId")?,
            &account_fence,
            required_u64(params, "generation")?,
            required_string(params, "action")?,
            params.get("terminalReason").and_then(Value::as_str).map(str::to_string),
            now_ms(),
        )?;
        serde_json::to_value(session).map_err(|error| error.to_string())
    }

    fn human_call_media_update(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let session = self.human_calls.update_media(
            required_string(params, "callId")?,
            &account_fence,
            required_u64(params, "generation")?,
            params.get("mediaCapabilities").cloned().unwrap_or_else(|| json!({})),
            params.get("deviceSelection").cloned().unwrap_or_else(|| json!({})),
            now_ms(),
        )?;
        serde_json::to_value(session).map_err(|error| error.to_string())
    }

    fn human_call_signal_send(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let signal = self.human_calls.send_signal(
            required_string(params, "callId")?,
            &account_fence,
            required_u64(params, "generation")?,
            &self.device_id,
            required_u64(params, "seq")?,
            required_string(params, "kind")?,
            params.get("payload").cloned().ok_or("signal payload is required")?,
            now_ms(),
        )?;
        serde_json::to_value(signal).map_err(|error| error.to_string())
    }

    fn human_call_signal_list(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let signals = self.human_calls.signals_since(
            required_string(params, "callId")?,
            &account_fence,
            required_u64(params, "generation")?,
            params.get("afterSeq").and_then(Value::as_u64).unwrap_or(0),
        )?;
        serde_json::to_value(signals).map_err(|error| error.to_string())
    }

    fn human_call_reconcile(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let session = self.human_calls.reconcile_unknown(
            required_string(params, "callId")?,
            &account_fence,
            required_u64(params, "generation")?,
            required_string(params, "terminalState")?,
            params.get("reason").and_then(Value::as_str).map(str::to_string),
            now_ms(),
        )?;
        serde_json::to_value(session).map_err(|error| error.to_string())
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
            next_run_at_ms: None,
        };
        let id = spec.id.clone();
        let time_zone = self.effective_time_zone()?;
        self.automation_runtime.upsert_spec(spec, time_zone.as_deref())?;
        Ok(json!({"id":id,"localId":local_id,"stored":true}))
    }

    fn automation_list(&mut self) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let time_zone = self.effective_time_zone()?;
        self.automation_runtime.refresh_next_runs(time_zone.as_deref())?;
        Ok(Value::Array(self.automation_runtime.list_for_account(&account_fence)))
    }

    fn automation_list_for_agent(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let agent_id = required_string(params, "agentId")?;
        if agent_id != "mahayana-assistant" {
            let agents = self
                .agents
                .lock()
                .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?;
            if agents.get(agent_id).is_none() {
                return Err("automation Agent owner not found".into());
            }
        }
        let time_zone = self.effective_time_zone()?;
        self.automation_runtime.refresh_next_runs(time_zone.as_deref())?;
        Ok(Value::Array(
            self.automation_runtime
                .list_for_agent(&account_fence, agent_id),
        ))
    }

    fn automation_set_agent_enabled(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let agent_id = required_string(params, "agentId")?;
        if agent_id != "mahayana-assistant" {
            let agents = self
                .agents
                .lock()
                .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?;
            if agents.get(agent_id).is_none() {
                return Err("automation Agent owner not found".into());
            }
        }
        let enabled = params
            .get("enabled")
            .and_then(Value::as_bool)
            .ok_or("enabled is required")?;
        let time_zone = self.effective_time_zone()?;
        let spec = self.automation_runtime.set_agent_workflow_enabled(
            &account_fence,
            agent_id,
            required_string(params, "automationId")?,
            enabled,
            time_zone.as_deref(),
        )?;
        serde_json::to_value(spec).map_err(|error| error.to_string())
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
        let time_zone = self.effective_time_zone()?;
        let run = self.automation_runtime.settle(
            required_string(params, "runId")?,
            &account_fence,
            required_u64(params, "generation")?,
            succeeded,
            params.get("reason").and_then(Value::as_str).map(str::to_string),
            now_ms(),
            time_zone.as_deref(),
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

    fn prepare_account_transition_cleanup(
        &mut self,
        previous_fence: &str,
        reason: &str,
    ) -> Result<(), String> {
        self.fence_turns_for_account_change(previous_fence, reason)?;
        self.human_calls
            .fence_account(previous_fence, reason, now_ms())?;

        let teach_identity = self
            .teach_recording
            .lock()
            .map_err(|_| "Teach Recording owner lock poisoned".to_string())?
            .snapshot()
            .clone();
        if teach_identity.phase != TeachRecordingPhase::Idle
            && teach_identity.account_fence.as_deref() == Some(previous_fence)
        {
            self.teach_recording
                .lock()
                .map_err(|_| "Teach Recording owner lock poisoned".to_string())?
                .fence_account_switch(previous_fence, teach_identity.account_epoch)?;
        }

        let provisioning_identity = self
            .onepassword_provisioning
            .lock()
            .map_err(|_| "1Password provisioning owner lock poisoned".to_string())?
            .snapshot()
            .clone();
        if matches!(
            provisioning_identity.phase,
            DurableProvisioningPhase::Prepared
                | DurableProvisioningPhase::Delivering
                | DurableProvisioningPhase::OutcomeUnknown
        ) && provisioning_identity.account_fence.as_deref() == Some(previous_fence)
        {
            self.onepassword_provisioning
                .lock()
                .map_err(|_| "1Password provisioning owner lock poisoned".to_string())?
                .fence_account_switch(previous_fence, provisioning_identity.account_epoch)?;
        }

        // Prepared variable writes may contain secret values. Even though commit is account-fenced,
        // the old account's uncommitted secret material must not survive the transition in memory.
        self.pending_plugin_variable_writes.clear();
        self.cancel_mcp_auth_watches_for_account_change(reason)?;
        self.user_settings
            .lock()
            .map_err(|_| "Android user settings lock poisoned".to_string())?
            .set_local_tool_permission_ceiling(None)
            .map_err(|error| format!("failed to clear stale local tool permission ceiling: {error}"))?;
        Ok(())
    }


    fn agent_avatar_generate(&self, params: &Value) -> Result<Value, String> {
        let description = required_string(params, "description")?.trim();
        if description.is_empty() {
            return Err("Agent avatar generation requires a description".into());
        }
        if description.len() > 8_000 {
            return Err("Agent avatar generation description is too large".into());
        }

        // Generated-image results are account-scoped backend projections. Capture the fence before
        // the network request and reject the result if an account transition happened in-flight.
        let account_fence = self.current_turn_account_fence()?;
        // Cursor/Sand credentials and machine identity remain owned by the Rust backend.
        // Presentation receives only bounded generated image bytes, never a credential.
        let generated = self
            .mcp_dashboard_backend
            .as_ref()
            .ok_or("Agent avatar generation backend is unavailable")?
            .generate_image(description, &[])
            .map_err(|error| error.to_string())?;
        if self.current_turn_account_fence()? != account_fence {
            return Err("Agent avatar generation result is stale after account switch".into());
        }

        // Desktop's cursor-generate-image adapter treats the backend MIME value as an image
        // subtype and constructs data:image/<mime>;base64,... . Accept either that wire shape
        // ("png", "webp") or a fully-qualified image MIME and normalize exactly once.
        let mime = generated.mime_type.trim();
        if mime.is_empty() || mime.chars().any(char::is_control) {
            return Err("Agent avatar generation returned an invalid MIME type".into());
        }
        let mime_type = if mime.starts_with("image/") {
            mime.to_string()
        } else if mime.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')) {
            format!("image/{mime}")
        } else {
            return Err("Agent avatar generation returned a non-image MIME type".into());
        };
        let bytes = BASE64_STANDARD
            .decode(generated.image_data.as_bytes())
            .map_err(|_| "Agent avatar generation returned invalid base64")?;
        if bytes.is_empty() {
            return Err("Agent avatar generation returned an empty image".into());
        }
        if bytes.len() > 25 * 1024 * 1024 {
            return Err("Agent avatar generation returned an image larger than 25 MB".into());
        }
        let data_url = format!("data:{mime_type};base64,{}", generated.image_data);
        Ok(json!({
            "accountFence": account_fence,
            "imageData": generated.image_data,
            "mimeType": mime_type,
            "dataUrl": data_url,
        }))
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
            let base_sections = mutation
                .get("baseSections")
                .and_then(Value::as_array)
                .ok_or("sidebar base sections array is required")?
                .iter()
                .cloned()
                .map(serde_json::from_value::<AndroidSidebarSection>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("invalid sidebar base section payload: {error}"))?;
            let requested_account_fence = required_string(mutation, "accountFence")?;
            let expected_revision = mutation
                .get("expectedRevision")
                .and_then(Value::as_u64)
                .ok_or("sidebar expected revision is required")?;
            let known_agent_ids = self
                .agents
                .lock()
                .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
                .list()
                .into_iter()
                .map(|agent| agent.id)
                .collect::<BTreeSet<_>>();
            let account_scope = self.current_sidebar_account_scope()?;
            if requested_account_fence != account_scope {
                return Err("stale sidebar section mutation account fence".into());
            }
            return self
                .sidebar_sections
                .lock()
                .map_err(|_| "canonical Android sidebar sections lock poisoned".to_string())?
                .set(
                    &account_scope,
                    operation_id,
                    expected_revision,
                    &base_sections,
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
        let account_scope = self.current_sidebar_account_scope()?;
        let known_agent_ids = self
            .agents
            .lock()
            .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
            .list()
            .into_iter()
            .map(|agent| agent.id)
            .collect::<BTreeSet<_>>();
        let snapshot = self
            .sidebar_sections
            .lock()
            .map_err(|_| "canonical Android sidebar sections lock poisoned".to_string())?
            .get_snapshot(&account_scope, &known_agent_ids);
        Ok(json!({"accountFence":snapshot.account_fence,"revision":snapshot.revision,"sections":snapshot.sections}))
    }

    fn process_crash_record(&mut self, params: &Value) -> Result<Value, String> {
        let kind = required_string(params, "kind")?;
        if kind != "uncaught_exception" {
            return Err("unsupported Android process crash kind".into());
        }
        let error_class = required_string(params, "errorClass")?;
        if error_class.is_empty()
            || error_class.chars().count() > 96
            || !error_class
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '$'))
        {
            return Err("invalid Android process crash error class".into());
        }
        let occurred_at_ms = required_u64(params, "occurredAtMs")?;
        let uptime_ms = required_u64(params, "uptimeMs")?;

        // The Android platform crash handoff is replayed only after process restart. Attribute
        // that recovered crash to the prior unclean session instead of tainting the newly armed
        // current session marker, which would otherwise create a false crash on the next launch.
        if let Some(previous_unclean) = self.process_crash_recent.iter_mut().rev().find(|entry| {
            entry.get("kind").and_then(Value::as_str) == Some("unclean_exit")
        }) {
            if let Some(record) = previous_unclean.as_object_mut() {
                record.insert("crashSeen".to_string(), Value::Bool(true));
            }
        }

        let bounded_uptime_ms = uptime_ms.min(30 * 24 * 60 * 60 * 1_000_u64);
        let record = json!({
            "kind": kind,
            "errorClass": error_class,
            "occurredAtMs": occurred_at_ms,
            "uptimeMs": bounded_uptime_ms,
            "errorCode": "SAND-E0603",
            "errorDomain": "android-process",
            "errorRetryable": false,
        });
        if self.process_crash_recent.len() >= PROCESS_CRASH_EVENT_CAP {
            self.process_crash_recent.pop_front();
        }
        self.process_crash_recent.push_back(record.clone());

        // Preserve only bounded source-neutral crash metadata. The Android platform handoff never
        // forwards Throwable message/stack/request content, and Host does not read such fields.
        self.sync_structured_log_account_slot();
        let metadata = BTreeMap::from([
            ("kind".to_string(), kind.to_string()),
            ("error_class".to_string(), error_class.to_string()),
            ("error_code".to_string(), "SAND-E0603".to_string()),
            ("error_domain".to_string(), "android-process".to_string()),
            ("uptime_ms".to_string(), bounded_uptime_ms.to_string()),
        ]);
        if self
            .structured_log_spool
            .record(
                "error",
                "sand.desktop.process_crash",
                metadata,
                occurred_at_ms,
            )
            .is_err()
        {
            self.edge_failures
                .report("structured-log-spill", "process-crash-record", "StorageError");
        }
        Ok(record)
    }

    fn process_metrics_observe(&mut self, params: &Value) -> Result<Value, String> {
        if !self
            .user_settings
            .lock()
            .map_err(|_| "Android user settings lock poisoned".to_string())?
            .process_metrics_local_enabled()
        {
            return Err("process metrics collection is disabled".into());
        }
        let observed_at_ms = params.get("observedAtMs").and_then(Value::as_u64).ok_or("observedAtMs is required")?;
        let cpu_time_ms_cumulative = params.get("cpuTimeMsCumulative").and_then(Value::as_u64).ok_or("cpuTimeMsCumulative is required")?;
        let memory_bytes = params.get("memoryBytes").and_then(Value::as_u64).ok_or("memoryBytes is required")?;
        let sample = self.process_metrics.observe(observed_at_ms, cpu_time_ms_cumulative, memory_bytes)?;
        let Some(sample) = sample else {
            return Ok(Value::Null);
        };
        let projected = serde_json::to_value(sample)
            .map_err(|error| format!("failed to project Android process metrics: {error}"))?;
        if self.process_metrics_recent.len() >= 32 {
            self.process_metrics_recent.pop_front();
        }
        self.process_metrics_recent.push_back(projected.clone());
        Ok(projected)
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
        let mut lifecycle = self
            .turn_lifecycle
            .lock()
            .map_err(|_| "turn lifecycle lock poisoned".to_string())?;
        let disk_reconciled = lifecycle.reconcile_disk_pressure_claim(
            &account_fence,
            &reconciled.operation_id,
            reconciled.state == DurableTurnState::Completed,
            now_ms(),
        )?;
        let awaiting_user_reconciled = lifecycle.reconcile_awaiting_user(
            &account_fence,
            &reconciled.operation_id,
            reconciled.generation,
        )?;

        Ok(json!({
            "requestId":request_id,
            "operationId":reconciled.operation_id,
            "state":format!("{:?}", reconciled.state).to_ascii_lowercase(),
            "diskPressureReconciled":disk_reconciled,
            "awaitingUserReconciled":awaiting_user_reconciled,
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

    fn account_profile_update(&mut self, params: &Value) -> Result<Value, String> {
        let display_name = required_string(params, "displayName")?;
        if self.mode == AndroidHostMode::Test {
            return Ok(json!({
                "ok":true,
                "auth":{
                    "loggedIn":self.logged_in,
                    "user":{
                        "nickname":display_name,
                        "username":"fabushi",
                        "email":"fabushi@example.invalid"
                    }
                }
            }));
        }
        #[cfg(feature = "ci-account-session-import")]
        if self.ci_session_identity.is_some() {
            return Err("Profile mutation is unavailable for imported CI sessions.".into());
        }
        let (result, mutation) = self.account.update_display_name(display_name)?;
        Ok(with_account_session_mutation(result, mutation))
    }

    fn account_logout(&mut self) -> Result<Value, String> {
        let previous_fence = self.current_turn_account_fence().ok();
        if let Some(previous_fence) = previous_fence.as_deref() {
            self.prepare_account_transition_cleanup(previous_fence, "account-logout")?;
        } else {
            self.pending_plugin_variable_writes.clear();
            self.cancel_mcp_auth_watches_for_account_change("account-logout")?;
        }
        *self
            .live_account_fence
            .lock()
            .map_err(|_| "live account fence lock poisoned".to_string())? = None;
        self.account_binding_refusal = None;
        if self.mode == AndroidHostMode::Test {
            self.logged_in = false;
            self.account_telemetry.report_signin(SigninLoginReport::SignedOut(
                SigninSignoutCause::UserLogout,
            ));
            return Ok(self.auth_status());
        }
        #[cfg(feature = "ci-account-session-import")]
        if self.ci_session_identity.is_some() {
            self.logged_in = false;
            self.ci_session_identity = None;
            if let Some(path) = self.ci_session_path.take() {
                let _ = std::fs::remove_file(path);
            }
            self.account_telemetry.report_signin(SigninLoginReport::SignedOut(
                SigninSignoutCause::UserLogout,
            ));
            return Ok(self.auth_status());
        }
        let (status, mutation) = self.account.logout()?;
        self.account_telemetry.report_signin(SigninLoginReport::SignedOut(
            SigninSignoutCause::UserLogout,
        ));
        Ok(with_account_session_mutation(status, Some(mutation)))
    }

    fn authorize_current_account_binding(&mut self) -> Result<(), String> {
        let account_slot = self
            .account
            .account_storage_scope()
            .ok_or("account environment binding requires a settled account scope")?;
        let descriptor = self.account.environment_descriptor_url();
        match self.account_binding.authorize(
            &account_slot,
            &descriptor,
            false,
            self.preexisting_account_durable_data,
        ) {
            Ok(true) => {
                self.account_binding_refusal = None;
                Ok(())
            }
            Ok(false) => {
                self.account.clear_local_session();
                let message =
                    "Existing Android durable data belongs to another account environment."
                        .to_string();
                self.account_binding_refusal = Some(message.clone());
                Err(message)
            }
            Err(error) => {
                self.account.clear_local_session();
                let message = format!("Android account environment binding failed: {error}");
                self.account_binding_refusal = Some(message.clone());
                Err(message)
            }
        }
    }

    fn report_signin_poll_result(&mut self, result: &Value) {
        match result.get("status").and_then(Value::as_str) {
            Some("completed") => self
                .account_telemetry
                .report_signin(SigninLoginReport::LoginCompleted),
            Some("failed") => self
                .account_telemetry
                .report_signin(SigninLoginReport::LoginFailed("error")),
            Some("expired") => self
                .account_telemetry
                .report_signin(SigninLoginReport::LoginFailed("timeout")),
            Some("cancelled") => self.account_telemetry.report_signin(
                SigninLoginReport::SignedOut(SigninSignoutCause::Forced),
            ),
            _ => {}
        }
    }

    fn auth_browser_start(&mut self) -> Result<Value, String> {
        let result = if self.mode == AndroidHostMode::Test {
            self.browser_start()?
        } else {
            self.account.browser_start()?
        };
        self.account_telemetry.report_signin(SigninLoginReport::LoginStarted);
        Ok(result)
    }

    fn auth_browser_reopen(&mut self, params: &Value) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Test {
            return self.browser_reopen(params);
        }
        self.account.browser_reopen(required_string(params, "attemptId")?)
    }

    fn auth_browser_cancel(&mut self, params: &Value) -> Result<Value, String> {
        let result = if self.mode == AndroidHostMode::Test {
            self.browser_cancel(params)?
        } else {
            self.account.browser_cancel(required_string(params, "attemptId")?)?
        };
        self.account_telemetry.report_signin(SigninLoginReport::SignedOut(
            SigninSignoutCause::Forced,
        ));
        Ok(result)
    }

    fn auth_browser_poll(&mut self, params: &Value) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Test {
            let result = self.browser_poll(params)?;
            self.report_signin_poll_result(&result);
            return Ok(result);
        }
        let previous_fence = self.account.session_fence();
        let (result, mutation) = self
            .account
            .browser_poll(required_string(params, "attemptId")?)?;
        let mut current_fence = self.account.session_fence();
        if current_fence.is_some() && previous_fence != current_fence {
            if let Err(error) = self.authorize_current_account_binding() {
                current_fence = None;
                *self
                    .live_account_fence
                    .lock()
                    .map_err(|_| "live account fence lock poisoned".to_string())? = None;
                self.account_telemetry
                    .report_signin(SigninLoginReport::LoginFailed("policy_refused"));
                return Ok(with_account_session_mutation(
                    json!({
                        "attemptId":required_string(params, "attemptId")?,
                        "status":"failed",
                        "errorMessage":error,
                    }),
                    Some(AccountSessionMutation::Clear),
                ));
            }
        }
        *self
            .live_account_fence
            .lock()
            .map_err(|_| "live account fence lock poisoned".to_string())? = current_fence.clone();
        if previous_fence.is_some()
            && current_fence.is_some()
            && previous_fence != current_fence
        {
            if let Some(previous_fence) = previous_fence.as_deref() {
                self.prepare_account_transition_cleanup(previous_fence, "account-switch")?;
            }
        }
        self.report_signin_poll_result(&result);
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

    fn mcp_server_accounts(&self, params: &Value) -> Result<Value, String> {
        let server_id = required_string(params, "serverId")?;
        let _account_fence = self.current_turn_account_fence()?;
        let backend = self
            .mcp_dashboard_backend
            .as_ref()
            .ok_or("MCP Dashboard backend is available only in production")?;
        let accounts = backend
            .list_server_accounts(server_id)?
            .into_iter()
            .map(|account| {
                json!({
                    "accountKey": account.account_key,
                    "rowServerIdentifier": account.row_server_identifier,
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({"serverId":server_id,"accounts":accounts}))
    }

    fn mcp_account_logout(&mut self, params: &Value) -> Result<Value, String> {
        let server_id = required_string(params, "serverId")?.to_string();
        let account_key = required_string(params, "accountKey")?.to_string();
        let _account_fence = self.current_turn_account_fence()?;
        let backend = self
            .mcp_dashboard_backend
            .as_ref()
            .ok_or("MCP Dashboard backend is available only in production")?;
        let snapshot = backend
            .fresh_server_snapshot(&server_id)?
            .ok_or("MCP server is not available")?;
        let server_url = snapshot
            .server_url
            .as_deref()
            .ok_or("MCP account logout requires an HTTP/SSE server URL")?;
        backend.logout_mcp_account(server_url, &account_key)?;
        let cancelled = self
            .mcp_auth_watches
            .lock()
            .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?
            .cancel_watch(&server_id, &account_key)?;
        if let Some(completion) = cancelled.as_ref() {
            self.emit_mcp_auth_watch_completion(completion, Some("account-logout"));
        }
        self.events.push_back(json!({
            "type":"mcp.account.changed",
            "action":"logout",
            "serverId":server_id,
            "serverName":snapshot.server_name,
            "accountKey":account_key,
        }));
        Ok(json!({"status":"logged-out","serverId":server_id,"accountKey":account_key}))
    }

    fn mcp_account_rename(&mut self, params: &Value) -> Result<Value, String> {
        let server_id = required_string(params, "serverId")?.to_string();
        let account_key = required_string(params, "accountKey")?.to_string();
        let new_account_key = required_string(params, "newAccountKey")?.to_string();
        let _account_fence = self.current_turn_account_fence()?;
        let backend = self
            .mcp_dashboard_backend
            .as_ref()
            .ok_or("MCP Dashboard backend is available only in production")?;
        backend.rename_mcp_account(&server_id, &account_key, &new_account_key)?;
        let cancelled = self
            .mcp_auth_watches
            .lock()
            .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?
            .cancel_watch(&server_id, &account_key)?;
        if let Some(completion) = cancelled.as_ref() {
            self.emit_mcp_auth_watch_completion(completion, Some("account-renamed"));
        }
        self.events.push_back(json!({
            "type":"mcp.account.changed",
            "action":"rename",
            "serverId":server_id,
            "accountKey":account_key,
            "newAccountKey":new_account_key,
        }));
        Ok(json!({
            "status":"renamed",
            "serverId":server_id,
            "accountKey":account_key,
            "newAccountKey":new_account_key,
        }))
    }

    fn mcp_account_remove(&mut self, params: &Value) -> Result<Value, String> {
        let server_id = required_string(params, "serverId")?.to_string();
        let account_key = required_string(params, "accountKey")?.to_string();
        let _account_fence = self.current_turn_account_fence()?;
        let backend = self
            .mcp_dashboard_backend
            .as_ref()
            .ok_or("MCP Dashboard backend is available only in production")?;
        backend.delete_mcp_account(&server_id, &account_key)?;
        let cancelled = self
            .mcp_auth_watches
            .lock()
            .map_err(|_| "MCP auth watch manager lock poisoned".to_string())?
            .cancel_watch(&server_id, &account_key)?;
        if let Some(completion) = cancelled.as_ref() {
            self.emit_mcp_auth_watch_completion(completion, Some("account-removed"));
        }
        self.events.push_back(json!({
            "type":"mcp.account.changed",
            "action":"remove",
            "serverId":server_id,
            "accountKey":account_key,
        }));
        Ok(json!({"status":"removed","serverId":server_id,"accountKey":account_key}))
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
        let projected = if self.mode == AndroidHostMode::Test {
            let attempt_id = self.next_attempt_id("oauth");
            self.oauth_attempts.insert(attempt_id.clone());
            json!({
                "attemptId":attempt_id,
                "provider":provider,
                "url":format!("https://api.ombhrum.com/sign-in?attempt={attempt_id}")
            })
        } else {
            let result = self.account.browser_start_for_provider(Some(provider))?;
            json!({
                "attemptId":result["attemptId"],
                "provider":provider,
                "url":result["loginUrl"],
                "status":result["status"],
                "pollAfterMs":result["pollAfterMs"],
                "expiresAt":result["expiresAt"],
            })
        };
        self.account_telemetry.report_signin(SigninLoginReport::LoginStarted);
        Ok(projected)
    }

    fn oauth_poll(&mut self, params: &Value) -> Result<Value, String> {
        let attempt_id = required_string(params, "attemptId")?;
        if self.mode == AndroidHostMode::Test {
            if !self.oauth_attempts.remove(attempt_id) {
                return Err("OAuth attempt is unknown or already consumed".into());
            }
            self.logged_in = true;
            let result = json!({"attemptId":attempt_id,"status":"completed","auth":self.auth_status()});
            self.report_signin_poll_result(&result);
            return Ok(result);
        }
        let previous_fence = self.account.session_fence();
        let (result, mutation) = self.account.browser_poll(attempt_id)?;
        let mut current_fence = self.account.session_fence();
        if current_fence.is_some() && previous_fence != current_fence {
            if let Err(error) = self.authorize_current_account_binding() {
                current_fence = None;
                *self
                    .live_account_fence
                    .lock()
                    .map_err(|_| "live account fence lock poisoned".to_string())? = None;
                self.account_telemetry
                    .report_signin(SigninLoginReport::LoginFailed("policy_refused"));
                return Ok(with_account_session_mutation(
                    json!({
                        "attemptId":required_string(params, "attemptId")?,
                        "status":"failed",
                        "errorMessage":error,
                    }),
                    Some(AccountSessionMutation::Clear),
                ));
            }
        }
        *self
            .live_account_fence
            .lock()
            .map_err(|_| "live account fence lock poisoned".to_string())? = current_fence.clone();
        if previous_fence.is_some()
            && current_fence.is_some()
            && previous_fence != current_fence
        {
            if let Some(previous_fence) = previous_fence.as_deref() {
                self.prepare_account_transition_cleanup(previous_fence, "account-switch")?;
            }
        }
        self.report_signin_poll_result(&result);
        Ok(with_account_session_mutation(result, mutation))
    }

    fn oauth_cancel(&mut self, params: &Value) -> Result<Value, String> {
        let attempt_id = required_string(params, "attemptId")?;
        let result = if self.mode == AndroidHostMode::Test {
            if !self.oauth_attempts.remove(attempt_id) {
                return Err("OAuth attempt is unknown or already consumed".into());
            }
            json!({"attemptId":attempt_id,"status":"cancelled"})
        } else {
            self.account.browser_cancel(attempt_id)?
        };
        self.account_telemetry.report_signin(SigninLoginReport::SignedOut(
            SigninSignoutCause::Forced,
        ));
        Ok(result)
    }

    fn send_acknowledgement_scope(
        &self,
        params: &Value,
    ) -> Result<(Option<String>, String, String), String> {
        let account_slot = params
            .get("accountSlot")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        let account_fence = self.current_turn_account_fence()?;
        let agent_id = required_string(params, "agentId")?.to_string();
        Ok((account_slot, account_fence, agent_id))
    }

    fn composer_draft_read(&mut self, params: &Value) -> Result<Value, String> {
        let (account_slot, account_fence, agent_id) = self.send_acknowledgement_scope(params)?;
        let draft = self
            .composer_drafts
            .lock()
            .map_err(|_| "composer draft owner lock poisoned".to_string())?
            .read(account_slot.as_deref(), &account_fence, &agent_id)?;
        Ok(json!({"draft": draft}))
    }

    fn composer_draft_write(&mut self, params: &Value) -> Result<Value, String> {
        let (account_slot, account_fence, agent_id) = self.send_acknowledgement_scope(params)?;
        let prompt = params
            .get("prompt")
            .and_then(Value::as_str)
            .ok_or("composer draft prompt is required")?;
        let draft = self
            .composer_drafts
            .lock()
            .map_err(|_| "composer draft owner lock poisoned".to_string())?
            .write(account_slot.as_deref(), &account_fence, &agent_id, prompt, now_ms())?;
        serde_json::to_value(draft)
            .map_err(|error| format!("failed to serialize composer draft: {error}"))
    }

    fn composer_draft_clear_if_current(&mut self, params: &Value) -> Result<Value, String> {
        let (account_slot, account_fence, agent_id) = self.send_acknowledgement_scope(params)?;
        let draft_id = required_string(params, "draftId")?;
        let cleared = self
            .composer_drafts
            .lock()
            .map_err(|_| "composer draft owner lock poisoned".to_string())?
            .clear_if_current(account_slot.as_deref(), &account_fence, &agent_id, draft_id)?;
        Ok(json!({"cleared": cleared}))
    }


    fn composer_draft_attachment_begin(&mut self, params: &Value) -> Result<Value, String> {
        let (account_slot, account_fence, agent_id) = self.send_acknowledgement_scope(params)?;
        let name = required_string(params, "name")?;
        let mime_type = params
            .get("mimeType")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let declared_size_bytes = params
            .get("sizeBytes")
            .and_then(Value::as_u64);
        let current_count = self
            .composer_drafts
            .lock()
            .map_err(|_| "composer draft owner lock poisoned".to_string())?
            .read(account_slot.as_deref(), &account_fence, &agent_id)?
            .map(|draft| draft.attachments.len())
            .unwrap_or(0);
        if current_count >= COMPOSER_ATTACHMENT_LIMIT {
            return Err("composer attachment limit reached".into());
        }
        let attachment_id = self
            .agent_attachments
            .lock()
            .map_err(|_| "Agent attachment owner lock poisoned".to_string())?
            .begin(
                account_slot.as_deref(),
                &account_fence,
                &agent_id,
                name,
                mime_type,
                declared_size_bytes,
            )?;
        Ok(json!({"attachmentId":attachment_id}))
    }

    fn composer_draft_attachment_append(&mut self, params: &Value) -> Result<Value, String> {
        let (account_slot, account_fence, agent_id) = self.send_acknowledgement_scope(params)?;
        let attachment_id = required_string(params, "attachmentId")?;
        let offset = params
            .get("offset")
            .and_then(Value::as_u64)
            .ok_or("Agent attachment offset is required")?;
        let encoded = required_string(params, "dataBase64")?;
        let bytes = BASE64_STANDARD
            .decode(encoded)
            .map_err(|_| "Agent attachment chunk is not valid base64".to_string())?;
        if bytes.len() > 1024 * 1024 {
            return Err("Agent attachment chunk exceeds the one MiB bridge limit".into());
        }
        let next_offset = self
            .agent_attachments
            .lock()
            .map_err(|_| "Agent attachment owner lock poisoned".to_string())?
            .append(
                account_slot.as_deref(),
                &account_fence,
                &agent_id,
                attachment_id,
                offset,
                &bytes,
            )?;
        Ok(json!({"nextOffset":next_offset}))
    }

    fn composer_draft_attachment_finish(&mut self, params: &Value) -> Result<Value, String> {
        let (account_slot, account_fence, agent_id) = self.send_acknowledgement_scope(params)?;
        let attachment_id = required_string(params, "attachmentId")?;
        let attachment = self
            .agent_attachments
            .lock()
            .map_err(|_| "Agent attachment owner lock poisoned".to_string())?
            .finish(
                account_slot.as_deref(),
                &account_fence,
                &agent_id,
                attachment_id,
            )?;
        let draft = self
            .composer_drafts
            .lock()
            .map_err(|_| "composer draft owner lock poisoned".to_string())?
            .add_attachment(
                account_slot.as_deref(),
                &account_fence,
                &agent_id,
                attachment.clone(),
                now_ms(),
            );
        match draft {
            Ok(draft) => Ok(json!({"attachment":attachment,"draft":draft})),
            Err(error) => {
                let _ = self
                    .agent_attachments
                    .lock()
                    .map_err(|_| "Agent attachment owner lock poisoned".to_string())?
                    .remove_completed(
                        account_slot.as_deref(),
                        &account_fence,
                        &agent_id,
                        attachment_id,
                    );
                Err(error)
            }
        }
    }

    fn composer_draft_attachment_read_range(&mut self, params: &Value) -> Result<Value, String> {
        let (account_slot, account_fence, agent_id) = self.send_acknowledgement_scope(params)?;
        let attachment_id = required_string(params, "attachmentId")?;
        let offset = params.get("offset").and_then(Value::as_u64).unwrap_or(0);
        let length = params.get("length").and_then(Value::as_u64).unwrap_or(4 * 1024 * 1024);
        let (attachment, bytes) = self
            .agent_attachments
            .lock()
            .map_err(|_| "Agent attachment owner lock poisoned".to_string())?
            .read_completed_media_chunk(
                account_slot.as_deref(),
                &account_fence,
                &agent_id,
                attachment_id,
                offset,
                length,
            )?;
        let next_offset = offset.saturating_add(bytes.len() as u64);
        Ok(json!({
            "attachmentId": attachment.id,
            "name": attachment.name,
            "mimeType": attachment.mime_type,
            "offset": offset,
            "nextOffset": next_offset,
            "size": attachment.size_bytes,
            "eof": next_offset >= attachment.size_bytes,
            "dataBase64": BASE64_STANDARD.encode(bytes),
        }))
    }

    fn composer_draft_attachment_abort(&mut self, params: &Value) -> Result<Value, String> {
        let (account_slot, account_fence, agent_id) = self.send_acknowledgement_scope(params)?;
        let attachment_id = required_string(params, "attachmentId")?;
        let aborted = self
            .agent_attachments
            .lock()
            .map_err(|_| "Agent attachment owner lock poisoned".to_string())?
            .abort(
                account_slot.as_deref(),
                &account_fence,
                &agent_id,
                attachment_id,
            )?;
        Ok(json!({"aborted":aborted}))
    }

    fn composer_draft_attachment_remove(&mut self, params: &Value) -> Result<Value, String> {
        let (account_slot, account_fence, agent_id) = self.send_acknowledgement_scope(params)?;
        let attachment_id = required_string(params, "attachmentId")?;
        let current = self
            .composer_drafts
            .lock()
            .map_err(|_| "composer draft owner lock poisoned".to_string())?
            .read(account_slot.as_deref(), &account_fence, &agent_id)?;
        let is_current = current
            .as_ref()
            .is_some_and(|draft| draft.attachments.iter().any(|row| row.id == attachment_id));
        if !is_current {
            return Err("Agent attachment is not present in the current composer draft".into());
        }
        let draft = self
            .composer_drafts
            .lock()
            .map_err(|_| "composer draft owner lock poisoned".to_string())?
            .remove_attachment(
                account_slot.as_deref(),
                &account_fence,
                &agent_id,
                attachment_id,
                now_ms(),
            )?;
        self.agent_attachments
            .lock()
            .map_err(|_| "Agent attachment owner lock poisoned".to_string())?
            .remove_completed(
                account_slot.as_deref(),
                &account_fence,
                &agent_id,
                attachment_id,
            )?;
        Ok(json!({"removed":true,"draft":draft}))
    }

    fn send_acknowledgement_begin(&mut self, params: &Value) -> Result<Value, String> {
        let (account_slot, account_fence, agent_id) = self.send_acknowledgement_scope(params)?;
        let nonce = required_string(params, "nonce")?;
        let text = params.get("text").and_then(Value::as_str).unwrap_or_default();
        let rich_text = params
            .get("richText")
            .filter(|value| !value.is_null())
            .cloned();
        let selected_model = self.validate_available_model_selection(
            params.get("model").and_then(Value::as_str),
        )?;
        let attachments = AgentAttachment::list_from_json(params.get("attachments"))?;
        self.agent_attachments
            .lock()
            .map_err(|_| "Agent attachment owner lock poisoned".to_string())?
            .validate_completed(
                account_slot.as_deref(),
                &account_fence,
                &agent_id,
                &attachments,
            )?;
        let entries = SendAcknowledgementEntry::list_from_json(
            params
                .get("entries")
                .ok_or("send acknowledgement entries are required")?,
        )?;
        let record = self
            .send_acknowledgements
            .lock()
            .map_err(|_| "send acknowledgement owner lock poisoned".to_string())?
            .begin_with_attachments_rich_text_and_model(
                account_slot.as_deref(),
                &account_fence,
                &agent_id,
                nonce,
                text,
                rich_text,
                selected_model,
                attachments,
                entries,
            )?;
        serde_json::to_value(record)
            .map_err(|error| format!("failed to serialize send acknowledgement: {error}"))
    }

    fn send_acknowledgement_queue(&mut self, params: &Value) -> Result<Value, String> {
        let (account_slot, account_fence, agent_id) = self.send_acknowledgement_scope(params)?;
        let nonce = required_string(params, "nonce")?;
        let generation = params
            .get("generation")
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .ok_or("send acknowledgement generation is required")?;
        let record = self
            .send_acknowledgements
            .lock()
            .map_err(|_| "send acknowledgement owner lock poisoned".to_string())?
            .mark_queued(
                account_slot.as_deref(),
                &account_fence,
                &agent_id,
                nonce,
                generation,
            )?;
        serde_json::to_value(record)
            .map_err(|error| format!("failed to serialize queued send acknowledgement: {error}"))
    }

    fn send_acknowledgement_cancel_queued(&mut self, params: &Value) -> Result<Value, String> {
        let (account_slot, account_fence, agent_id) = self.send_acknowledgement_scope(params)?;
        let nonce = required_string(params, "nonce")?;
        let generation = params
            .get("generation")
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .ok_or("send acknowledgement generation is required")?;
        let cancelled = self
            .send_acknowledgements
            .lock()
            .map_err(|_| "send acknowledgement owner lock poisoned".to_string())?
            .cancel_queued(
                account_slot.as_deref(),
                &account_fence,
                &agent_id,
                nonce,
                generation,
            )?;
        Ok(json!({"cancelled":true,"acknowledgement":cancelled}))
    }

    fn send_acknowledgement_dispatch(&mut self, params: &Value) -> Result<Value, String> {
        let (account_slot, account_fence, agent_id) = self.send_acknowledgement_scope(params)?;
        let nonce = required_string(params, "nonce")?;
        let generation = params
            .get("generation")
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .ok_or("send acknowledgement generation is required")?;

        let dispatching = {
            let mut owner = self
                .send_acknowledgements
                .lock()
                .map_err(|_| "send acknowledgement owner lock poisoned".to_string())?;
            owner.mark_queued(
                account_slot.as_deref(),
                &account_fence,
                &agent_id,
                nonce,
                generation,
            )?;
            owner.mark_dispatching(
                account_slot.as_deref(),
                &account_fence,
                &agent_id,
                nonce,
                generation,
            )?
        };

        let command = json!({
            "type":"chat.send",
            "requestId":dispatching.nonce,
            "clientNonce":dispatching.nonce,
            "text":dispatching.message_text,
            "richText":dispatching.message_rich_text,
            "attachments":dispatching.attachments,
            "model":dispatching.selected_model,
            "accountSlot":dispatching.account_slot,
            "agentId":dispatching.agent_id,
            "mode":"agent",
        });
        match self.feature_execute(&json!({"command":command})) {
            Ok(accepted) => {
                let record = self
                    .send_acknowledgements
                    .lock()
                    .map_err(|_| "send acknowledgement owner lock poisoned".to_string())?
                    .mark_accepted(
                        account_slot.as_deref(),
                        &account_fence,
                        &agent_id,
                        nonce,
                        generation,
                    )?;
                self.record_send_settlement_telemetry(&record, "acked", now_ms());
                Ok(json!({
                    "acknowledgement":record,
                    "accepted":accepted,
                }))
            }
            Err(error) => {
                let transcript = self
                    .transcript
                    .lock()
                    .map_err(|_| "transcript lock poisoned".to_string())?
                    .get_transcript();
                let (settled, before_reconcile, failed_record) = {
                    let mut owner = self
                        .send_acknowledgements
                        .lock()
                        .map_err(|_| "send acknowledgement owner lock poisoned".to_string())?;
                    let before_reconcile = owner.record(
                        account_slot.as_deref(),
                        &account_fence,
                        &agent_id,
                        nonce,
                    );
                    let settled = owner.reconcile_transcript(
                        account_slot.as_deref(),
                        &account_fence,
                        &agent_id,
                        &transcript,
                    )?;
                    let failed_record = if settled == 0 {
                        Some(owner.mark_failed(
                            account_slot.as_deref(),
                            &account_fence,
                            &agent_id,
                            nonce,
                            generation,
                            now_ms(),
                            error.clone(),
                        )?)
                    } else {
                        None
                    };
                    (settled, before_reconcile, failed_record)
                };
                if settled > 0 {
                    if let Some(record) = before_reconcile.as_ref() {
                        self.record_send_settlement_telemetry(record, "acked", now_ms());
                    }
                    return Err(format!(
                        "send dispatch returned an error after canonical echo; acknowledgement reconciled without retry: {error}"
                    ));
                }
                if let Some(record) = failed_record.as_ref() {
                    self.record_send_settlement_telemetry(record, "failed", now_ms());
                }
                Err(error)
            }
        }
    }

    fn send_acknowledgement_retry_prepare(&mut self, params: &Value) -> Result<Value, String> {
        let (account_slot, account_fence, agent_id) = self.send_acknowledgement_scope(params)?;
        let nonce = required_string(params, "nonce")?;
        let fresh_nonce = required_string(params, "freshNonce")?;
        let generation = params
            .get("generation")
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .ok_or("send acknowledgement generation is required")?;
        let entries = SendAcknowledgementEntry::list_from_json(
            params
                .get("entries")
                .ok_or("send acknowledgement entries are required")?,
        )?;
        let retried = self
            .send_acknowledgements
            .lock()
            .map_err(|_| "send acknowledgement owner lock poisoned".to_string())?
            .retry_failed(
                account_slot.as_deref(),
                &account_fence,
                &agent_id,
                nonce,
                generation,
                fresh_nonce,
                entries,
            )?;
        serde_json::to_value(retried)
            .map_err(|error| format!("failed to serialize prepared send retry: {error}"))
    }

    fn send_acknowledgement_retry(&mut self, params: &Value) -> Result<Value, String> {
        let retried = self.send_acknowledgement_retry_prepare(params)?;
        self.send_acknowledgement_dispatch(&json!({
            "accountSlot":retried.get("accountSlot").cloned().unwrap_or(Value::Null),
            "agentId":retried.get("agentId").cloned().unwrap_or(Value::Null),
            "nonce":retried.get("nonce").cloned().unwrap_or(Value::Null),
            "generation":retried.get("generation").cloned().unwrap_or(Value::Null),
        }))
    }


    fn send_acknowledgement_clear_approvals(&mut self, params: &Value) -> Result<Value, String> {
        let (account_slot, account_fence, agent_id) = self.send_acknowledgement_scope(params)?;
        let nonce = required_string(params, "nonce")?;
        let generation = params
            .get("generation")
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .ok_or("send acknowledgement generation is required")?;

        let accepted = self
            .send_acknowledgements
            .lock()
            .map_err(|_| "send acknowledgement owner lock poisoned".to_string())?
            .record(account_slot.as_deref(), &account_fence, &agent_id, nonce)
            .ok_or("post-journal approval cleanup requires a durable send acknowledgement")?;
        if accepted.generation != generation {
            return Err("post-journal approval cleanup was fenced by send generation".into());
        }
        if accepted.phase != SendAcknowledgementPhase::AcceptedAwaitingEcho {
            return Err("post-journal approval cleanup requires an accepted send".into());
        }

        let approval_id = params
            .get("approvalId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let approval_operation_id = params
            .get("approvalOperationId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let (Some(approval_id), Some(approval_operation_id)) =
            (approval_id, approval_operation_id)
        else {
            if approval_id.is_some() || approval_operation_id.is_some() {
                return Err("approvalId and approvalOperationId must be provided together".into());
            }
            return Ok(json!({
                "status":"idle",
                "cleared":false,
                "generation":generation,
                "approvalId":Value::Null,
            }));
        };

        let reason = "superseded by an accepted journaled user send";
        let cancelled = self.capability_broker.cancel_approval(
            approval_id,
            approval_operation_id,
            &account_fence,
            reason,
            now_ms(),
        )?;
        if !cancelled {
            return Ok(json!({
                "status":"settled",
                "cleared":false,
                "generation":generation,
                "approvalId":approval_id,
            }));
        }

        let registry = if self.subagent_review_approvals.contains(approval_id) {
            self.subagent_review_approvals.resolve(approval_id, false)?;
            "subagent-review"
        } else if self
            .remote_approvals
            .clear_from_ui(approval_id, approval_operation_id, &account_fence)?
            .is_some()
        {
            "remote"
        } else if let Some(pending_operation) = self.pending_approvals.get(approval_id).cloned() {
            if pending_operation != approval_operation_id {
                return Err("feature approval cleanup operation identity disagrees with registry".into());
            }
            self.pending_approvals.remove(approval_id);
            self.active_operations.remove(approval_operation_id);
            self.events.push_back(json!({
                "type":"operation.interrupted",
                "operationId":approval_operation_id,
                "reason":"superseded-by-journaled-send",
            }));
            "feature"
        } else {
            "broker"
        };

        self.events.push_back(json!({
            "type":"approval.cleared",
            "approvalId":approval_id,
            "operationId":approval_operation_id,
            "agentId":agent_id,
            "accountSlot":account_slot,
            "reason":"superseded-by-journaled-send",
            "registry":registry,
        }));
        Ok(json!({
            "status":"cleared",
            "cleared":true,
            "generation":generation,
            "approvalId":approval_id,
            "registry":registry,
        }))
    }

    fn send_acknowledgement_snapshot(&mut self, params: &Value) -> Result<Value, String> {
        let (account_slot, account_fence, agent_id) = self.send_acknowledgement_scope(params)?;
        let transcript = self
            .transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?
            .get_transcript();
        let mut owner = self
            .send_acknowledgements
            .lock()
            .map_err(|_| "send acknowledgement owner lock poisoned".to_string())?;
        owner.reconcile_transcript(
            account_slot.as_deref(),
            &account_fence,
            &agent_id,
            &transcript,
        )?;
        Ok(owner.projection(account_slot.as_deref(), &account_fence, &agent_id))
    }
    fn send_acknowledgement_restored_snapshot(&self) -> Result<Value, String> {
        let account_slot = self.account.account_storage_scope();
        let account_fence = self.current_turn_account_fence()?;
        let owner = self
            .send_acknowledgements
            .lock()
            .map_err(|_| "send acknowledgement owner lock poisoned".to_string())?;
        let records = owner
            .restored_on_open()
            .into_iter()
            .filter(|record| {
                record.account_slot == account_slot && record.account_fence == account_fence
            })
            .collect::<Vec<_>>();
        Ok(json!({ "records": records }))
    }


    fn widget_respond(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let account_slot = params
            .get("accountSlot")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let entry_id = required_string(params, "entryId")?;
        let agent_id = required_string(params, "agentId")?;
        let generation = params.get("generation").and_then(Value::as_u64).unwrap_or(0);
        let value = required_string(params, "value")?.trim();
        if value.is_empty() || value.len() > 16 * 1024 {
            return Err("widget response value is invalid".into());
        }

        let mutation = self.mutate_widget_entry(
            entry_id,
            agent_id,
            account_slot,
            &account_fence,
            Some(value),
        )?;
        if mutation.get("accepted").and_then(Value::as_bool) != Some(true) {
            let mut rejected = mutation;
            rejected["generation"] = Value::from(generation);
            return Ok(rejected);
        }

        let current = mutation
            .get("entry")
            .cloned()
            .ok_or("accepted widget response is missing canonical entry")?;
        let request_id = format!("widget-response:{agent_id}:{entry_id}:{generation}");
        let mut command = json!({
            "type":"chat.send",
            "requestId":request_id,
            "agentId":agent_id,
            "text":value,
            "hidden":true,
            "appendUserMessage":false,
            "awaitTurn":false,
            "requestSource":"widget-response",
        });
        if let Some(reply_to) = current
            .get("replyTo")
            .or_else(|| current.get("replyToId"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            command["replyToId"] = Value::String(reply_to.to_string());
        }

        let continuation = match self.feature_execute(&json!({"command":command})) {
            Ok(accepted) => accepted,
            Err(error) => {
                self.rollback_widget_response(entry_id, agent_id)?;
                return Err(error);
            }
        };
        let operation_id = continuation
            .get("operationId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                let _ = self.rollback_widget_response(entry_id, agent_id);
                "widget response continuation did not return an operation id".to_string()
            })?;
        Ok(json!({
            "accepted":true,
            "entry":current,
            "generation":generation,
            "operationId":operation_id,
        }))
    }

    fn widget_dismiss(&mut self, params: &Value) -> Result<Value, String> {
        let account_fence = self.current_turn_account_fence()?;
        let account_slot = params
            .get("accountSlot")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let entry_id = required_string(params, "entryId")?;
        let agent_id = required_string(params, "agentId")?;
        let generation = params.get("generation").and_then(Value::as_u64).unwrap_or(0);
        let mut result = self.mutate_widget_entry(
            entry_id,
            agent_id,
            account_slot,
            &account_fence,
            None,
        )?;
        result["generation"] = Value::from(generation);
        Ok(result)
    }

    fn mutate_widget_entry(
        &mut self,
        entry_id: &str,
        agent_id: &str,
        account_slot: Option<&str>,
        account_fence: &str,
        response: Option<&str>,
    ) -> Result<Value, String> {
        let transcript = self
            .transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?
            .get_transcript();
        let entry_index = transcript
            .iter()
            .position(|entry| entry.get("id").and_then(Value::as_str) == Some(entry_id))
            .ok_or("widget transcript entry was not found")?;
        let current = transcript[entry_index].clone();
        if current.get("agentId").and_then(Value::as_str) != Some(agent_id) {
            return Err("widget transcript entry is fenced to another Agent".into());
        }
        if let Some(entry_slot) = current.get("accountSlot").and_then(Value::as_str) {
            if Some(entry_slot) != account_slot {
                return Err("widget transcript entry is fenced to another account slot".into());
            }
        }
        if let Some(entry_fence) = current.get("accountFence").and_then(Value::as_str) {
            if entry_fence != account_fence {
                return Err("widget transcript entry is fenced to another account generation".into());
            }
        }
        if current.get("kind").and_then(Value::as_str) != Some("send-message")
            || current
                .get("message")
                .and_then(Value::as_object)
                .and_then(|message| message.get("type"))
                .and_then(Value::as_str)
                != Some("widget")
        {
            return Err("transcript entry is not a widget".into());
        }
        if current.get("respondedValue").and_then(Value::as_str).is_some()
            || current
                .get("widgetDismissed")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            return Ok(json!({"accepted":false,"reason":"settled"}));
        }

        if let Some(value) = response {
            let widget = current
                .get("message")
                .and_then(Value::as_object)
                .and_then(|message| message.get("widget"))
                .and_then(Value::as_object)
                .ok_or("widget payload is malformed")?;
            let options = widget
                .get("options")
                .and_then(Value::as_array)
                .ok_or("widget options are malformed")?;
            let option_match = options.iter().any(|option| {
                let Some(option) = option.as_object() else { return false; };
                let Some(label) = option.get("label").and_then(Value::as_str) else { return false; };
                option
                    .get("value")
                    .and_then(Value::as_str)
                    .unwrap_or(label)
                    == value
            });
            let allow_custom = widget
                .get("allowCustom")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if !option_match && !allow_custom {
                return Ok(json!({"accepted":false,"reason":"value-not-allowed"}));
            }
            if widget
                .get("dismissOnMoveOn")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && transcript.iter().skip(entry_index + 1).any(|entry| {
                    entry.get("agentId").and_then(Value::as_str) == Some(agent_id)
                        && entry.get("kind").and_then(Value::as_str) == Some("message")
                        && entry.get("role").and_then(Value::as_str) == Some("user")
                })
            {
                return Ok(json!({"accepted":false,"reason":"stale-move-on"}));
            }
        }

        let updated = self
            .transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?
            .update_entry(entry_id, |entry| {
                let mut next = entry.clone();
                if let Some(value) = response {
                    next["respondedValue"] = Value::String(value.to_string());
                    if let Some(object) = next.as_object_mut() {
                        object.remove("widgetDismissed");
                    }
                } else if let Some(object) = next.as_object_mut() {
                    object.remove("widgetSkipped");
                    object.insert("widgetDismissed".into(), Value::Bool(true));
                }
                next
            })
            .map_err(|error| format!("failed to persist widget transcript action: {error}"))?
            .ok_or("widget transcript entry disappeared before mutation")?;
        Ok(json!({"accepted":true,"entry":updated}))
    }

    fn rollback_widget_response(&mut self, entry_id: &str, agent_id: &str) -> Result<(), String> {
        let current = self
            .transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?
            .entry(entry_id)
            .ok_or("widget transcript entry disappeared before rollback")?;
        if current.get("agentId").and_then(Value::as_str) != Some(agent_id) {
            return Err("widget rollback is fenced to another Agent".into());
        }
        self.transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?
            .update_entry(entry_id, |entry| {
                let mut next = entry.clone();
                if let Some(object) = next.as_object_mut() {
                    object.remove("respondedValue");
                }
                next
            })
            .map_err(|error| format!("failed to rollback widget response: {error}"))?
            .ok_or("widget transcript entry disappeared during rollback")?;
        Ok(())
    }

    fn mcp_routed_execute(
        &mut self,
        command: &Value,
        operation_id: &str,
        feature_request_id: &str,
    ) -> Result<(), String> {
        let server_id = required_string(command, "serverId")?.to_string();
        let tool_name = required_string(command, "toolName")?.to_string();
        let tool_call_id = required_string(command, "toolCallId")?.to_string();
        let approval_id = required_string(command, "approvalId")?;
        let approval_operation_id = required_string(command, "approvalOperationId")?;
        let approval_request_id = required_string(command, "approvalRequestId")?;
        let arguments = command
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !arguments.is_object() {
            return Err("mcp.routed.execute arguments must be a JSON object".into());
        }
        let account_fence = self.current_turn_account_fence()?;
        let disabled = self
            .mcp_server_tool_settings
            .lock()
            .map_err(|_| "MCP server tool settings lock poisoned".to_string())?
            .disabled_for(&account_fence, &server_id)?;
        if disabled.contains(&tool_name) {
            return Err("mcp.routed.execute tool is disabled for the current account".into());
        }

        self.capability_broker.consume_approval_for_dispatch(
            approval_id,
            approval_operation_id,
            approval_request_id,
            "mcp.execute",
            &account_fence,
            now_ms(),
        )?;

        let broker_request_id = format!("mcp-exec:{tool_call_id}");
        let plugin_id = format!("mcp:{server_id}");
        let generation = mcp_account_fence_generation(&account_fence);
        let started = now_ms();
        self.capability_broker.begin(PendingCapabilityCall {
            request_id: broker_request_id.clone(),
            plugin_id: plugin_id.clone(),
            capability: "mcp.execute".into(),
            tool: tool_name.clone(),
            arguments: arguments.clone(),
            required_permissions: BTreeSet::new(),
            account_fence: account_fence.clone(),
            runtime_generation: generation,
            started_at_ms: started,
            deadline_at_ms: started.saturating_add(EXECUTE_TOOL_TIMEOUT_MS),
            state: "pending".into(),
        })?;
        self.capability_broker.assert_current(
            &broker_request_id,
            &plugin_id,
            &account_fence,
            generation,
            now_ms(),
        )?;

        let backend = self
            .mcp_dashboard_backend
            .as_ref()
            .ok_or("MCP Dashboard backend is available only in production")?;
        let execution = backend.execute_server_tool(
            &server_id,
            &tool_name,
            &arguments,
            &tool_call_id,
            command.get("agentId").and_then(Value::as_str),
        );
        let finished = now_ms();

        if self.current_turn_account_fence().as_deref() != Ok(account_fence.as_str()) {
            if self.capability_broker.assert_current(
                &broker_request_id,
                &plugin_id,
                &account_fence,
                generation,
                finished,
            ).is_ok() {
                self.capability_broker.settle(
                    &broker_request_id,
                    "outcome_unknown",
                    Some("account fence changed while MCP tool execution was in flight".into()),
                    finished,
                )?;
            }
            return Err(
                "mcp.routed.execute result fenced by account switch; reconciliation required"
                    .into(),
            );
        }

        match execution {
            Ok(McpToolExecutionBackendResult::Settled(result)) => {
                self.capability_broker.assert_current(
                    &broker_request_id,
                    &plugin_id,
                    &account_fence,
                    generation,
                    finished,
                )?;
                self.capability_broker
                    .settle(&broker_request_id, "completed", None, finished)?;
                self.events.push_back(json!({
                    "type":"mcp.tool.completed",
                    "operationId":operation_id,
                    "requestId":feature_request_id,
                    "capabilityRequestId":broker_request_id,
                    "serverId":server_id,
                    "toolName":tool_name,
                    "toolCallId":tool_call_id,
                    "result":result,
                }));
                self.finish_operation(operation_id);
                Ok(())
            }
            Ok(McpToolExecutionBackendResult::OutcomeUnknown { message }) => {
                self.capability_broker.settle(
                    &broker_request_id,
                    "outcome_unknown",
                    Some(message.clone()),
                    finished,
                )?;
                self.events.push_back(json!({
                    "type":"mcp.tool.outcome-unknown",
                    "operationId":operation_id,
                    "requestId":feature_request_id,
                    "capabilityRequestId":broker_request_id,
                    "serverId":server_id,
                    "toolName":tool_name,
                    "toolCallId":tool_call_id,
                    "message":message,
                    "retrySafe":false,
                }));
                self.finish_operation(operation_id);
                Ok(())
            }
            Err(error) => {
                self.capability_broker.settle(
                    &broker_request_id,
                    "failed",
                    Some(format!(
                        "MCP execution failed before a backend tool result was accepted: {error}"
                    )),
                    finished,
                )?;
                Err(error)
            }
        }
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
            "mcp.routed.execute" => {
                if let Err(error) =
                    self.mcp_routed_execute(&command, &operation_id, request_id)
                {
                    self.active_operations.remove(&operation_id);
                    return Err(error);
                }
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
        let message_text = command
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let attachments = AgentAttachment::list_from_json(command.get("attachments"))?;
        if message_text.trim().is_empty() && attachments.is_empty() {
            return Err("chat.send requires text or an attachment".into());
        }
        if !text_size_allowed(&message_text) {
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
        let account_slot = command
            .get("accountSlot")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let account_fence_for_attachment = self.current_turn_account_fence()?;
        self.agent_attachments
            .lock()
            .map_err(|_| "Agent attachment owner lock poisoned".to_string())?
            .validate_completed(
                account_slot,
                &account_fence_for_attachment,
                &agent_id,
                &attachments,
            )?;
        let mut prompt = prompt_with_agent_attachments(&message_text, &attachments);
        if let Some(time_zone) = self.effective_time_zone()? {
            prompt.push_str("\n\n<system_reminder>\nUser time zone: ");
            prompt.push_str(&time_zone);
            prompt.push_str("\n</system_reminder>");
        }
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
                    "content":message_text.clone(),
                    "richText":command.get("richText").cloned().unwrap_or(Value::Null),
                    "attachments":attachments.clone(),
                    "operationId":operation_id,
                    "agentId":agent_id.clone(),
                    "clientNonce":command
                        .get("clientNonce")
                        .and_then(Value::as_str)
                        .unwrap_or(request_id),
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
        let turn_attachments = attachments.clone();
        let agent_attachment_root = self
            .agent_attachments
            .lock()
            .map_err(|_| "Agent attachment owner lock poisoned".to_string())?
            .completed_root();
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
        let remote_user_settings = Arc::clone(&self.user_settings);
        let remote_local_tool_permission_ceiling = match self.mode {
            AndroidHostMode::Test => None,
            AndroidHostMode::Production => match self.mcp_dashboard_backend.as_ref() {
                Some(backend) => match backend.try_resolve_local_tool_permission_ceiling() {
                    Ok(Some(value)) => LocalToolPermission::parse(value)
                        .ok()
                        .or(Some(LocalToolPermission::Ask)),
                    Ok(None) => None,
                    Err(_) => Some(LocalToolPermission::Ask),
                },
                None => Some(LocalToolPermission::Ask),
            },
        };
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
                let turn_run_shell = shared_turn_run_shell();
                let turn_lifecycle_process_epoch = match turn_lifecycle.lock() {
                    Ok(store) => store.process_epoch(),
                    Err(_) => {
                        let _ = turn_journal
                            .lock()
                            .map_err(|_| "turn journal lock poisoned".to_string())
                            .and_then(|mut journal| journal.settle(
                                &request_id_owned,
                                &operation_id_owned,
                                &account_fence_owned,
                                turn_generation,
                                DurableTurnState::Failed,
                                Some("turn lifecycle lock poisoned".into()),
                                now_ms(),
                            ));
                        return;
                    }
                };
                let task_review_token = bearer_token.clone();
                let task_review_cancelled = Arc::clone(&cancelled);
                let task_review_broker = subagent_review_broker.clone();
                let task_review_approvals = Arc::clone(&subagent_review_approvals);
                let task_review_events = Arc::clone(&subagent_review_events);
                let task_review_parent_agent = conversation_id_owned.clone();
                let task_review_parent_request = request_id_owned.clone();
                let task_review_account_fence = account_fence_owned.clone();
                let task_review_parent_operation = operation_id_owned.clone();
                let task_review_shell = Arc::clone(&turn_run_shell);
                let task_review_lifecycle = Arc::clone(&turn_lifecycle);
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
                                mark_turn_awaiting_user(
                                    &task_review_shell,
                                    &task_review_lifecycle,
                                    &task_review_account_fence,
                                    &task_review_parent_agent,
                                    &task_review_parent_request,
                                    &task_review_parent_operation,
                                    turn_generation,
                                    turn_lifecycle_process_epoch,
                                    &reason,
                                )?;
                                let approval = wait_for_subagent_review_approval(
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
                                );
                                let cleared = clear_turn_awaiting_user(
                                    &task_review_shell,
                                    &task_review_lifecycle,
                                    &task_review_account_fence,
                                    &task_review_parent_agent,
                                    &task_review_parent_request,
                                    &task_review_parent_operation,
                                    turn_generation,
                                    turn_lifecycle_process_epoch,
                                );
                                let approved = approval?;
                                cleared?;
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
                let steer_review_parent_operation = operation_id_owned.clone();
                let steer_review_shell = Arc::clone(&turn_run_shell);
                let steer_review_lifecycle = Arc::clone(&turn_lifecycle);
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
                                mark_turn_awaiting_user(
                                    &steer_review_shell,
                                    &steer_review_lifecycle,
                                    &steer_review_account_fence,
                                    &steer_review_parent_agent,
                                    &steer_review_parent_request,
                                    &steer_review_parent_operation,
                                    turn_generation,
                                    turn_lifecycle_process_epoch,
                                    &reason,
                                )?;
                                let approval = wait_for_subagent_review_approval(
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
                                );
                                let cleared = clear_turn_awaiting_user(
                                    &steer_review_shell,
                                    &steer_review_lifecycle,
                                    &steer_review_account_fence,
                                    &steer_review_parent_agent,
                                    &steer_review_parent_request,
                                    &steer_review_parent_operation,
                                    turn_generation,
                                    turn_lifecycle_process_epoch,
                                );
                                let approved = approval?;
                                cleared?;
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
                        Arc::clone(&remote_user_settings),
                        remote_local_tool_permission_ceiling,
                        Arc::clone(&live_account_fence),
                        Arc::clone(&turn_events),
                        &operation_id_owned,
                        &request_id_owned,
                        Arc::clone(&cancelled),
                    )
                } else {
                    routed_subagent_tools
                };
                let routed_subagent_tools = if turn_attachments.is_empty() {
                    routed_subagent_tools
                } else {
                    with_agent_attachment_tools(
                        routed_subagent_tools,
                        agent_attachment_root.clone(),
                        turn_attachments.clone(),
                    )
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

                let checkpoint_store = Arc::clone(&turn_journal);
                let checkpoint_request = request_id_owned.clone();
                let checkpoint_operation = operation_id_owned.clone();
                let checkpoint_account = account_fence_owned.clone();
                let checkpoint_persist: ProductionTurnCheckpointPersist = Arc::new(
                    move |attempt, emitted_chunks| {
                        checkpoint_store
                            .lock()
                            .map_err(|_| "turn journal lock poisoned".to_string())?
                            .persist_checkpoint_metadata(
                                &checkpoint_request,
                                &checkpoint_operation,
                                &checkpoint_account,
                                turn_generation,
                                attempt,
                                emitted_chunks,
                                now_ms(),
                            )
                    },
                );

                let mut lifecycle_bindings = ProductionTurnAgentLifecycleBindings::new(
                    account_fence_owned.clone(),
                    conversation_id_owned.clone(),
                    operation_id_owned.clone(),
                )
                .with_checkpoint_persist(checkpoint_persist)
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
                    .with_turn_run_shell(Arc::clone(&turn_run_shell))
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
                        let checkpoint_outcome_unknown = error
                            .message
                            .starts_with("durable turn checkpoint persistence failed:");
                        let settled = turn_journal
                            .lock()
                            .map_err(|_| "turn journal lock poisoned".to_string())
                            .and_then(|mut journal| {
                                if checkpoint_outcome_unknown {
                                    journal.mark_outcome_unknown(
                                        &request_id_owned,
                                        &operation_id_owned,
                                        &account_fence_owned,
                                        turn_generation,
                                        error.message.clone(),
                                        now_ms(),
                                    )
                                } else {
                                    journal.settle(
                                        &request_id_owned,
                                        &operation_id_owned,
                                        &account_fence_owned,
                                        turn_generation,
                                        DurableTurnState::Failed,
                                        Some(error.message.clone()),
                                        now_ms(),
                                    )
                                }
                            })
                            .is_ok();
                        if settled {
                            push_turn_event(
                                &turn_events,
                                if checkpoint_outcome_unknown {
                                    json!({
                                        "type":"operation.interrupted",
                                        "operationId":operation_id_owned,
                                        "requestId":request_id_owned,
                                        "reason":"outcome-unknown",
                                        "message":error.message,
                                    })
                                } else {
                                    json!({
                                        "type":"operation.failed",
                                        "operationId":operation_id_owned,
                                        "requestId":request_id_owned,
                                        "message":error.message,
                                    })
                                },
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
            .tray_events
            .lock()
            .map_err(|_| "tray event queue lock poisoned".to_string())?
            .pop_front()
        {
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


    fn quiesce_plugin_for_release_mutation(
        &mut self,
        plugin_id: &str,
        reason: &str,
    ) -> Result<bool, String> {
        self.runtime_call_cancellations.signal_plugin(plugin_id);
        self.capability_broker.cancel_plugin(plugin_id, reason, now_ms())?;
        let generation = self.runtime_generations.entry(plugin_id.to_string()).or_insert(0);
        *generation = generation.saturating_add(1);
        let runtime_started = self
            .js_runtime
            .as_ref()
            .and_then(|runtime| runtime.plugin_state(plugin_id))
            .is_some();
        if runtime_started {
            self.js_runtime
                .as_mut()
                .expect("plugin runtime disappeared during release mutation")
                .disable_plugin(plugin_id)
                .map_err(|error| format!("failed to quiesce plugin runtime: {error}"))?;
            self.sync_js_runtime_events()?;
        }
        self.runtime_tools.remove(plugin_id);
        Ok(runtime_started)
    }

    fn plugin_update(&mut self, params: &Value) -> Result<Value, String> {
        let release = params.get("release").ok_or("feature.plugin.update requires release")?;
        let plugin_id = release
            .get("pluginId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("plugin release is missing pluginId")?
            .to_string();
        if self.mode == AndroidHostMode::Test {
            if !self.installed_plugins.contains(&plugin_id) {
                return Err("plugin is not installed".into());
            }
            return Ok(json!({
                "pluginId":plugin_id,
                "updated":true,
                "requiresRuntimeRestart":false,
                "fixture":true
            }));
        }

        self.plugin_installer
            .active(&plugin_id)
            .map_err(|error| format!("failed to inspect installed plugin: {error}"))?
            .ok_or("plugin is not installed")?;
        let manifest: ExternalReleaseManifest = serde_json::from_value(release.clone())
            .map_err(|error| format!("plugin release manifest is invalid: {error}"))?;
        let runtime_was_started = self.quiesce_plugin_for_release_mutation(
            &plugin_id,
            "plugin release update",
        )?;
        let pointer = self
            .plugin_installer
            .install(
                &manifest,
                "android",
                &["deepseek-js", "javascript", "cordis-js", "mcp", "wasm"],
            )
            .map_err(|error| format!("verified plugin update failed: {error}"))?;
        self.plugin_permissions
            .retain_requested(&pointer.plugin_id, &pointer.requested_permissions)
            .map_err(|error| format!("failed to reconcile plugin permissions after update: {error}"))?;
        Ok(json!({
            "pluginId":pointer.plugin_id,
            "version":pointer.version,
            "artifactId":pointer.artifact_id,
            "artifactSha256":pointer.artifact_sha256,
            "runtime":pointer.runtime,
            "entry":pointer.entry,
            "requestedPermissions":pointer.requested_permissions,
            "installedPath":pointer.installed_path,
            "updated":true,
            "requiresRuntimeRestart":runtime_was_started,
            "fixture":false
        }))
    }

    fn plugin_uninstall(&mut self, params: &Value) -> Result<Value, String> {
        let plugin_id = required_string(params, "pluginId")?.to_string();
        if self.mode == AndroidHostMode::Test {
            let removed = self.installed_plugins.remove(&plugin_id);
            return Ok(json!({"pluginId":plugin_id,"removed":removed,"fixture":true}));
        }
        if self
            .plugin_installer
            .active(&plugin_id)
            .map_err(|error| format!("failed to inspect installed plugin: {error}"))?
            .is_none()
        {
            return Ok(json!({"pluginId":plugin_id,"removed":false,"fixture":false}));
        }

        let runtime_was_started = self.quiesce_plugin_for_release_mutation(
            &plugin_id,
            "plugin uninstalled",
        )?;
        let removed = self
            .plugin_installer
            .uninstall(&plugin_id)
            .map_err(|error| format!("plugin uninstall failed: {error}"))?;
        self.plugin_permissions
            .remove_plugin(&plugin_id)
            .map_err(|error| format!("failed to remove plugin grants: {error}"))?;
        Ok(json!({
            "pluginId":plugin_id,
            "removed":removed,
            "runtimeWasStarted":runtime_was_started,
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

fn has_existing_account_durable_data(app_data_dir: &std::path::Path) -> bool {
    [
        "messaging-repository.json",
        "transcript.json",
        "agents.json",
        "composer-drafts.json",
        "agent-turn-journal.json",
        "automation-runtime.json",
        "plugin-variables.json",
    ]
    .iter()
    .any(|name| app_data_dir.join(name).metadata().is_ok())
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

fn required_android_messaging_scope(command: Option<&Value>) -> &'static str {
    match command
        .and_then(|command| command.get("type"))
        .and_then(Value::as_str)
    {
        Some("beginBlobUpload" | "appendBlobChunk" | "finishBlobUpload" | "deleteBlob") => {
            "blobsWrite"
        }
        Some("createInvoice" | "checkoutInvoice" | "refundOrder" | "walletStatus") => "payments",
        Some("installMiniApp" | "grantMiniApp" | "openMiniApp" | "miniAppCall") => "miniApps",
        _ => "messaging",
    }
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
    fn global_content_search_is_persistent_account_fenced_and_reconciles_changes() {
        let root=tempfile::tempdir().unwrap();
        {
            let mut host=AndroidJsonHost::new(root.path(),AndroidHostMode::Test);
            host.logged_in=true;
            let fence=host.current_turn_account_fence().unwrap();
            let mut transcript=host.transcript.lock().unwrap();
            transcript.append_entry(json!({"id":"m1","agentId":"a","kind":"message","role":"user","content":"needle old","timestampMs":1,"accountFence":fence})).unwrap();
            transcript.append_entry(json!({"id":"m2","agentId":"b","kind":"message","role":"assistant","content":"needle new","timestampMs":2,"accountFence":fence})).unwrap();
            transcript.append_entry(json!({"id":"foreign","agentId":"x","kind":"message","role":"assistant","content":"needle foreign","timestampMs":3,"accountFence":"foreign:epoch"})).unwrap();
        }
        let mut reopened=AndroidJsonHost::new(root.path(),AndroidHostMode::Test); reopened.logged_in=true;
        let first=reopened.dispatch("feature.search.global",&json!({"query":"needle"})).unwrap();
        assert_eq!(first["messages"].as_array().unwrap().iter().map(|v|v["entryId"].as_str().unwrap()).collect::<Vec<_>>(),vec!["m2","m1"]);
        reopened.transcript.lock().unwrap().update_entry("m1",|entry|{let mut next=entry.clone();next["content"]=Value::String("changed".into());next}).unwrap();
        assert_eq!(reopened.dispatch("feature.search.global",&json!({"query":"needle"})).unwrap()["messages"].as_array().unwrap().len(),1);
        assert_eq!(reopened.dispatch("feature.search.global",&json!({"query":"changed"})).unwrap()["messages"].as_array().unwrap().len(),1);
    }

#[test]
    fn agent_content_search_reads_canonical_transcript_and_survives_reopen() {
        let root = tempfile::tempdir().unwrap();
        {
            let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
            let mut transcript = host.transcript.lock().unwrap();
            transcript.append_entry(json!({"id":"old","agentId":"agent-a","kind":"message","role":"user","content":"Alpha   BETA older","timestampMs":1})).unwrap();
            transcript.append_entry(json!({"id":"new","agentId":"agent-a","kind":"send-message","message":{"type":"text","content":"alpha beta newest"},"timestampMs":2})).unwrap();
            transcript.append_entry(json!({"id":"hidden","agentId":"agent-a","kind":"message","role":"assistant","content":"alpha beta hidden","hiddenOutboundAgentPeerMessage":true,"timestampMs":3})).unwrap();
            transcript.append_entry(json!({"id":"other","agentId":"agent-b","kind":"message","role":"assistant","content":"alpha beta other agent","timestampMs":4})).unwrap();
        }
        let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let result = reopened.dispatch("feature.search.agentContent", &json!({"agentId":"agent-a","query":"ALPHA beta","limit":5})).unwrap();
        let matches = result["matches"].as_array().unwrap();
        assert_eq!(matches.iter().map(|v| v["entryId"].as_str().unwrap()).collect::<Vec<_>>(), vec!["new","old"]);
        assert_eq!(matches[0]["role"], "assistant");
        assert_eq!(matches[1]["role"], "user");
        assert_eq!(matches[1]["snippet"], "Alpha BETA older");
        assert!(reopened.dispatch("feature.search.agentContent", &json!({"agentId":"agent-b","query":"alpha beta"})).unwrap()["matches"].as_array().unwrap().iter().all(|v| v["entryId"] == "other"));
    }

    use crate::runner::SubagentLineage;

    #[test]
    fn platform_injected_machine_id_does_not_create_plaintext_host_file() {
        let root = tempfile::tempdir().unwrap();
        let injected = "android-keystore-device-123";
        let host = AndroidJsonHost::new_with_account_session_and_device_id(
            root.path(),
            AndroidHostMode::Test,
            None,
            injected,
        );
        assert!(
            !root.path().join("machine-id").exists(),
            "platform-injected identity must not create the legacy plaintext machine-id file",
        );
        drop(host);

        let reopened = AndroidJsonHost::new_with_account_session_and_device_id(
            root.path(),
            AndroidHostMode::Test,
            None,
            injected,
        );
        assert!(
            !root.path().join("machine-id").exists(),
            "reopen with the protected platform identity must remain plaintext-free",
        );
        drop(reopened);
    }


    #[test]
    fn in_process_messaging_grant_binds_account_context_and_scopes() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        host.logged_in = true;

        let access = host
            .dispatch(
                "feature.messaging.access.issue",
                &json!({
                    "deviceId":"android:test-device",
                    "sessionId":"android:test-session",
                    "scopes":["messaging"]
                }),
            )
            .unwrap();
        assert_eq!(access["actorId"], "human:android-test");
        assert_eq!(access["deviceId"], "android:test-device");
        assert_eq!(access["sessionId"], "android:test-session");

        let valid = json!({
            "requestId":"messaging-grant-valid",
            "envelope":{
                "protocolVersion":2,
                "context":{
                    "requestId":"messaging-grant-valid",
                    "deviceId":"android:test-device",
                    "actorId":"human:android-test",
                    "sessionId":"android:test-session",
                    "sentAtMs":1
                },
                "command":{
                    "type":"upsertProfile",
                    "actor":{
                        "id":"human:android-test",
                        "kind":"human",
                        "displayName":"Android Test"
                    }
                }
            }
        });
        assert!(host.dispatch("feature.messaging.execute", &valid).is_ok());

        let mut forged = valid.clone();
        forged["requestId"] = json!("messaging-grant-forged");
        forged["envelope"]["context"]["requestId"] = json!("messaging-grant-forged");
        forged["envelope"]["context"]["deviceId"] = json!("android:other-device");
        let forged_error = host
            .dispatch("feature.messaging.execute", &forged)
            .unwrap_err();
        assert!(forged_error.contains("device/session context"));

        let blob_write = json!({
            "requestId":"messaging-grant-blob-write",
            "envelope":{
                "protocolVersion":2,
                "context":{
                    "requestId":"messaging-grant-blob-write",
                    "deviceId":"android:test-device",
                    "actorId":"human:android-test",
                    "sessionId":"android:test-session",
                    "sentAtMs":2
                },
                "command":{"type":"beginBlobUpload"}
            }
        });
        let scope_error = host
            .dispatch("feature.messaging.execute", &blob_write)
            .unwrap_err();
        assert!(scope_error.contains("required scope blobsWrite"));

        host.logged_in = false;
        assert!(host.dispatch("feature.messaging.execute", &valid).is_err());
    }

    #[test]
    fn content_search_rejects_reads_without_current_account_fence() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        host.logged_in = false;
        assert!(host
            .dispatch(
                "feature.contentSearch.messages",
                &json!({"query":"alpha","limit":50}),
            )
            .is_err());
        assert!(host
            .dispatch(
                "feature.search.agentContent",
                &json!({"agentId":"agent-a","query":"alpha","limit":5}),
            )
            .is_err());
        assert!(host
            .dispatch(
                "feature.contentSearch.media",
                &json!({"query":"alpha","limit":50}),
            )
            .is_err());
        assert!(host
            .dispatch(
                "feature.search.global",
                &json!({"query":"alpha"}),
            )
            .is_err());
    }

    #[test]
    fn account_transition_cleanup_drops_old_account_secrets_and_fences_teach_recording() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let fence = host.current_turn_account_fence().unwrap();

        let prepared = host
            .plugin_variable_prepare(&json!({
                "pluginId":"plugin-a",
                "schema":{"fields":[{"key":"token","type":"string","secret":true}]},
                "values":{"token":"old-account-secret"},
                "teamConfigured":false
            }))
            .unwrap();
        let write_id = prepared["writeId"].as_str().unwrap().to_string();
        assert_eq!(host.pending_plugin_variable_writes.len(), 1);

        {
            let mut owner = host.teach_recording.lock().unwrap();
            owner
                .begin_start(&fence, 7, "agent-a", "request-a", "operation-a")
                .unwrap();
            owner
                .mark_recording(&fence, 7, "operation-a", 11)
                .unwrap();
        }

        host.prepare_account_transition_cleanup(&fence, "account-switch")
            .unwrap();

        assert!(host.pending_plugin_variable_writes.is_empty());
        assert!(host
            .plugin_variable_commit(&json!({"writeId":write_id}))
            .unwrap_err()
            .contains("stale, cancelled, or already committed"));
        let teach = host.teach_recording.lock().unwrap().snapshot().clone();
        assert_eq!(teach.phase, TeachRecordingPhase::OutcomeUnknown);
        assert_eq!(teach.account_fence.as_deref(), Some(fence.as_str()));
    }

    #[test]
    fn teach_recording_status_is_durable_and_never_promotes_generic_remote_shell() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let initial = host.dispatch("feature.teachRecording.status", &json!({})).unwrap();
        assert_eq!(initial["phase"], "idle");
        assert_eq!(initial["maxDurationMs"], 600_000);
        assert_eq!(initial["requiresReconciliation"], false);
        assert_eq!(initial["remoteRecordingReady"], false);
        assert_eq!(
            initial["unavailableReason"],
            "authorized_remote_runner_teach_recording_executor_unavailable"
        );

        {
            let mut owner = host.teach_recording.lock().unwrap();
            owner
                .begin_start("session:account-a", 7, "agent-a", "request-a", "operation-a")
                .unwrap();
            owner
                .mark_recording("session:account-a", 7, "operation-a", 11)
                .unwrap();
        }
        drop(host);

        let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let recovered = reopened
            .dispatch("feature.teachRecording.status", &json!({}))
            .unwrap();
        assert_eq!(recovered["phase"], "outcome-unknown");
        assert_eq!(recovered["requiresReconciliation"], true);
        assert_eq!(recovered["operationId"], "operation-a");
    }

    #[test]
    fn sidebar_sections_dispatch_is_account_fenced_durable_and_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let mut first = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let first_agent = first
            .dispatch("createAgent", &json!({"name":"One","description":""}))
            .unwrap();
        let second_agent = first
            .dispatch("createAgent", &json!({"name":"Two","description":""}))
            .unwrap();
        let first_id = first_agent["agent"]["id"].as_str().unwrap().to_string();
        let second_id = second_agent["agent"]["id"].as_str().unwrap().to_string();
        let initial = first
            .dispatch("feature.agent.sidebarSections", &json!({}))
            .unwrap();
        let account_fence = initial["accountFence"].as_str().unwrap().to_string();

        let payload = json!({
            "operationId":"sidebar-op-1",
            "accountFence":"session:test:android",
            "mutation":{
                "kind":"sidebar-sections",
                "accountFence":account_fence.clone(),
                "expectedRevision":0,
                "baseSections":[],
                "sections":[{
                    "id":"section-a",
                    "name":"Team",
                    "agentIds":[first_id],
                    "isCollapsed":false
                }]
            }
        });
        let committed = first
            .dispatch("feature.agent.rosterMutation", &payload)
            .unwrap();
        assert_eq!(committed["status"], "completed");
        let projected = first
            .dispatch("feature.agent.sidebarSections", &json!({}))
            .unwrap();
        assert_eq!(projected["sections"][0]["id"], "section-a");
        assert_eq!(projected["sections"][1]["id"], "__agents__");
        assert_eq!(projected["sections"][1]["agentIds"][0], second_id);

        let replayed = first
            .dispatch("feature.agent.rosterMutation", &payload)
            .unwrap();
        assert_eq!(replayed, committed);

        let mismatch = first.dispatch(
            "feature.agent.rosterMutation",
            &json!({
                "operationId":"sidebar-op-1",
                "accountFence":"session:test:android",
                "mutation":{
                    "kind":"sidebar-sections",
                    "accountFence":account_fence.clone(),
                    "expectedRevision":0,
                    "baseSections":[],
                    "sections":[{
                        "id":"section-a",
                        "name":"Changed",
                        "agentIds":[],
                        "isCollapsed":false
                    }]
                }
            }),
        );
        assert!(mismatch.is_err());

        let stale_presentation = first.dispatch(
            "feature.agent.rosterMutation",
            &json!({
                "operationId":"sidebar-op-stale-presentation",
                "accountFence":"session:stale",
                "mutation":{
                    "kind":"sidebar-sections",
                    "accountFence":account_fence.clone(),
                    "expectedRevision":1,
                    "baseSections":projected["sections"].clone(),
                    "sections":[]
                }
            }),
        );
        assert!(stale_presentation.unwrap_err().contains("stale account"));

        let stale_sidebar_scope = first.dispatch(
            "feature.agent.rosterMutation",
            &json!({
                "operationId":"sidebar-op-stale-scope",
                "accountFence":"session:test:android",
                "mutation":{
                    "kind":"sidebar-sections",
                    "accountFence":"account-device:stale:android",
                    "expectedRevision":1,
                    "baseSections":projected["sections"].clone(),
                    "sections":[]
                }
            }),
        );
        assert!(stale_sidebar_scope
            .unwrap_err()
            .contains("stale sidebar section mutation account fence"));

        drop(first);
        let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let restored = reopened
            .dispatch("feature.agent.sidebarSections", &json!({}))
            .unwrap();
        assert_eq!(restored, projected);
        assert_eq!(
            reopened
                .dispatch("feature.agent.rosterMutation", &payload)
                .unwrap(),
            committed,
        );
    }

    #[test]
    fn test_mode_sand_access_fixture_does_not_define_production_policy() {
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
    fn async_tasks_pending_wakes_are_durable_and_account_fenced() {
        use crate::extensions::transcript::sand_pending_wake_store::{
            DurablePendingWakeMarker, PendingWakeKind,
        };

        let root = std::env::temp_dir().join(format!(
            "fabushi-agent-async-pending-wake-{}",
            now_ms()
        ));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        host.logged_in = true;
        let account_fence = host.current_turn_account_fence().unwrap();
        let fresh_marked_at = now_ms();
        assert!(host.pending_wake_store.mark_pending(DurablePendingWakeMarker {
            account_fence: account_fence.clone(),
            agent_id: "agent-a".into(),
            kind: PendingWakeKind::Shell,
            work_id: "shell-a".into(),
            marked_at_ms: fresh_marked_at,
            quiet_origin: None,
            title: Some("Long build".into()),
            subagent_type: None,
            interrupted_by_recreate: true,
        }));
        assert!(host.pending_wake_store.mark_pending(DurablePendingWakeMarker {
            account_fence: "other-account:epoch-9".into(),
            agent_id: "agent-a".into(),
            kind: PendingWakeKind::CloudAgent,
            work_id: "cloud-secret".into(),
            marked_at_ms: fresh_marked_at,
            quiet_origin: None,
            title: None,
            subagent_type: None,
            interrupted_by_recreate: false,
        }));
        assert!(host.pending_wake_store.mark_pending(DurablePendingWakeMarker {
            account_fence: account_fence.clone(),
            agent_id: "agent-a".into(),
            kind: PendingWakeKind::Shell,
            work_id: "stale-shell".into(),
            marked_at_ms: fresh_marked_at.saturating_sub(PENDING_WAKE_STALE_MAX_AGE_MS + 1),
            quiet_origin: None,
            title: Some("Stale build".into()),
            subagent_type: None,
            interrupted_by_recreate: false,
        }));

        let value = host
            .agent_async_tasks(&json!({"id":"agent-a"}))
            .expect("same-account async task projection");
        let tasks = value.as_array().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["kind"], "shell");
        assert_eq!(tasks[0]["id"], "shell-a");
        assert_eq!(tasks[0]["label"], "Long build");
        assert_eq!(tasks[0]["detail"], "reattached after a host restart");

        drop(host);
        let mut reopened = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        reopened.logged_in = true;
        assert_eq!(
            reopened
                .pending_wake_store
                .list_pending_for(&account_fence, "agent-a")
                .len(),
            1
        );
        assert_eq!(
            reopened
                .pending_wake_store
                .list_pending_for("other-account:epoch-9", "agent-a")
                .len(),
            1,
            "projection pruning must not delete another account's durable marker",
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn async_tasks_projection_uses_durable_parent_and_account_fenced_subagent_truth() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let frozen = SubagentFrozenTurnConfig {
            provider_id: "test-provider".into(),
            model_id: "test-model".into(),
            tool_names: vec![],
            allowed_subagent_types: vec!["executor".into()],
            privacy_mode: "default".into(),
            summarization_binding_id: "test-summary".into(),
        };
        let launch = host
            .subagent_owner
            .lock()
            .unwrap()
            .launch(
                "agent-parent",
                SubagentLineage::default(),
                "box-1",
                "executor",
                "tool-call-1",
                "Inspect repository state",
                "session:test:android",
                None,
                frozen,
                123,
            )
            .unwrap();

        let tasks = host
            .dispatch("feature.agent.asyncTasks", &json!({"id":"agent-parent"}))
            .unwrap();
        assert_eq!(tasks.as_array().unwrap().len(), 1);
        assert_eq!(tasks[0]["kind"], "subagent");
        assert_eq!(tasks[0]["id"], launch.record.subagent_id);
        assert_eq!(tasks[0]["label"], "Inspect repository state");
        assert_eq!(tasks[0]["status"], "running");
        assert_eq!(tasks[0]["startedAtMs"], 123);
        assert_eq!(tasks[0]["subagentType"], "executor");

        drop(host);
        let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let after_process_death = reopened
            .dispatch("feature.agent.asyncTasks", &json!({"id":"agent-parent"}))
            .unwrap();
        assert!(
            after_process_death.as_array().unwrap().is_empty(),
            "process reopen converts unresolved running child work to outcome-unknown instead of presenting it as running",
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
    fn hidden_roster_mutation_replays_same_operation_after_host_reopen_and_rejects_stale_account() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-hidden-roster-mutation-{}",
            now_ms()
        ));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let created = host
            .dispatch(
                "createAgent",
                &json!({"name":"Hidden lifecycle","description":"profile"}),
            )
            .unwrap();
        let agent_id = created["agent"]["id"].as_str().unwrap().to_string();
        let params = json!({
            "operationId":"hidden-operation-1",
            "accountFence":"session:test:android",
            "mutation":{"kind":"hidden","id":agent_id,"value":true},
        });

        let first = host
            .dispatch("feature.agent.rosterMutation", &params)
            .unwrap();
        assert_eq!(first["status"], "completed");
        assert_eq!(first["result"]["isHiddenFromSidebar"], true);
        assert_eq!(
            host.dispatch("listAgents", &json!({})).unwrap()[0]["isHiddenFromSidebar"],
            true,
        );

        let duplicate_settlement = host
            .dispatch("feature.agent.rosterMutation", &params)
            .unwrap();
        assert_eq!(
            duplicate_settlement, first,
            "same operation id must return the durable Host settlement",
        );

        let stale = host.dispatch(
            "feature.agent.rosterMutation",
            &json!({
                "operationId":"hidden-operation-old-account",
                "accountFence":"session:old-account",
                "mutation":{"kind":"hidden","id":agent_id,"value":false},
            }),
        );
        assert!(stale.unwrap_err().contains("stale account"));
        assert_eq!(
            host.dispatch("listAgents", &json!({})).unwrap()[0]["isHiddenFromSidebar"],
            true,
            "old-account callback must not mutate canonical hidden truth",
        );

        drop(host);
        let mut reopened = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let replay_after_process_death = reopened
            .dispatch("feature.agent.rosterMutation", &params)
            .unwrap();
        assert_eq!(replay_after_process_death, first);
        assert_eq!(
            reopened.dispatch("listAgents", &json!({})).unwrap()[0]["isHiddenFromSidebar"],
            true,
        );

        let rejected_params = json!({
            "operationId":"hidden-operation-rejected",
            "accountFence":"session:test:android",
            "mutation":{"kind":"hidden","id":"missing-agent","value":false},
        });
        let rejected = reopened
            .dispatch("feature.agent.rosterMutation", &rejected_params)
            .unwrap();
        assert_eq!(rejected["status"], "rejected");
        let rejected_replay = reopened
            .dispatch("feature.agent.rosterMutation", &rejected_params)
            .unwrap();
        assert_eq!(
            rejected_replay, rejected,
            "deterministic rejection must itself be a durable settlement",
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
        let lifecycle_epoch = host.turn_lifecycle.lock().unwrap().process_epoch();
        host.turn_lifecycle
            .lock()
            .unwrap()
            .mark_awaiting_user(
                &account_fence,
                &agent.id,
                "projection-request",
                "projection-op",
                9,
                lifecycle_epoch,
                "Waiting for approval",
                201,
            )
            .unwrap();

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
        assert_eq!(row["awaitingUserResponse"]["reason"], "Waiting for approval");
        assert_eq!(row["awaitingUserResponse"]["recoveryRequired"], false);
        assert!(row["updatedAt"].as_u64().unwrap() >= 201);

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
        assert_eq!(
            reopened_row["awaitingUserResponse"]["recoveryRequired"],
            true,
            "process restart must preserve waiting truth as recovery-required instead of fabricating an active run"
        );
        reopened
            .turn_lifecycle
            .lock()
            .unwrap()
            .reconcile_awaiting_user(&account_fence, "projection-op", 9)
            .unwrap();
        let reconciled_list = reopened.dispatch("listAgents", &json!({})).unwrap();
        let reconciled_row = reconciled_list
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == agent.id)
            .unwrap();
        assert!(reconciled_row["awaitingUserResponse"].is_null());

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
        // This test intentionally reopens the same durable Host state. Own a pre-created,
        // unique directory for the whole test instead of relying on one of the stores to
        // create a process/time-derived parent as an incidental side effect.
        let root_owner = tempfile::tempdir().unwrap();
        let root = root_owner.path();
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
        assert_eq!(frozen.model_id, "grok-4.5");
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


#[cfg(test)]
mod plugin_release_lifecycle_tests {
    use super::*;

    #[test]
    fn update_requires_install_and_uninstall_is_idempotent() {
        let root = std::env::temp_dir().join(format!("fabushi-plugin-lifecycle-{}", now_ms()));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let release = json!({"pluginId":"lifecycle-plugin"});
        assert!(host
            .dispatch("feature.plugin.update", &json!({"release":release.clone()}))
            .unwrap_err()
            .contains("plugin is not installed"));
        host.dispatch("feature.plugin.install", &json!({"release":release.clone()}))
            .unwrap();
        assert_eq!(
            host.dispatch("feature.plugin.update", &json!({"release":release}))
                .unwrap()["updated"],
            true
        );
        assert_eq!(
            host.dispatch(
                "feature.plugin.uninstall",
                &json!({"pluginId":"lifecycle-plugin"}),
            )
            .unwrap()["removed"],
            true
        );
        assert!(host
            .dispatch("plugin.compatibility", &json!({"pluginId":"lifecycle-plugin"}))
            .unwrap_err()
            .contains("plugin is not installed"));
        assert_eq!(
            host.dispatch(
                "feature.plugin.uninstall",
                &json!({"pluginId":"lifecycle-plugin"}),
            )
            .unwrap()["removed"],
            false
        );
        let _ = std::fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod widget_action_tests {
    use super::*;

    fn widget_entry(id: &str, agent_id: &str, allow_custom: bool) -> Value {
        json!({
            "id":id,
            "kind":"send-message",
            "agentId":agent_id,
            "message":{
                "type":"widget",
                "widget":{
                    "prompt":"Choose",
                    "options":[
                        {"label":"Alpha","value":"a"},
                        {"label":"Beta"}
                    ],
                    "allowCustom":allow_custom
                }
            }
        })
    }

    #[test]
    fn widget_response_and_dismiss_persist_canonically_and_reject_stale_or_settled_actions() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        host.transcript
            .lock()
            .unwrap()
            .append_entry(widget_entry("widget-a", "agent-a", false))
            .unwrap();
        host.transcript
            .lock()
            .unwrap()
            .append_entry(widget_entry("widget-b", "agent-a", true))
            .unwrap();

        assert!(host
            .dispatch(
                "feature.widget.respond",
                &json!({"agentId":"agent-b","entryId":"widget-a","value":"a"})
            )
            .is_err());
        let invalid = host
            .dispatch(
                "feature.widget.respond",
                &json!({"agentId":"agent-a","entryId":"widget-a","value":"custom"})
            )
            .unwrap();
        assert_eq!(invalid["accepted"], false);
        let accepted = host
            .dispatch(
                "feature.widget.respond",
                &json!({"agentId":"agent-a","entryId":"widget-a","value":"a"})
            )
            .unwrap();
        assert_eq!(accepted["accepted"], true);
        let duplicate = host
            .dispatch(
                "feature.widget.respond",
                &json!({"agentId":"agent-a","entryId":"widget-a","value":"a"})
            )
            .unwrap();
        assert_eq!(duplicate["accepted"], false);

        let dismissed = host
            .dispatch(
                "feature.widget.dismiss",
                &json!({"agentId":"agent-a","entryId":"widget-b"})
            )
            .unwrap();
        assert_eq!(dismissed["accepted"], true);

        drop(host);
        let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let transcript = reopened
            .dispatch("feature.transcript.snapshot", &json!({}))
            .unwrap();
        let rows = transcript.as_array().unwrap();
        let answered = rows.iter().find(|row| row["id"] == "widget-a").unwrap();
        let dismissed = rows.iter().find(|row| row["id"] == "widget-b").unwrap();
        assert_eq!(answered["respondedValue"], "a");
        assert_eq!(dismissed["widgetDismissed"], true);
    }

    #[test]
    fn post_journal_approval_cleanup_is_canonical_and_failure_isolated() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);

        let pending = host
            .dispatch(
                "feature.execute",
                &json!({"command":{
                    "type":"capability.request",
                    "requestId":"approval-cleanup-old",
                    "capability":"camera"
                }}),
            )
            .unwrap();
        let old_operation = pending["operationId"].as_str().unwrap().to_string();
        let approval = (0..16)
            .find_map(|_| {
                let event = host.dispatch("feature.receive", &json!({})).ok()?;
                (event["type"] == "approval.requested"
                    && event["operationId"] == old_operation)
                    .then_some(event)
            })
            .expect("matching approval requested");
        let approval_id = approval["approvalId"].as_str().unwrap().to_string();

        let accepted = {
            let mut owner = host.send_acknowledgements.lock().unwrap();
            let record = owner
                .begin(
                    None,
                    "session:test:android",
                    "mahayana-assistant",
                    "cleanup-send-a",
                    "hello",
                    vec![SendAcknowledgementEntry {
                        id: "cleanup-send-a".into(),
                        kind: crate::send_acknowledgement::SendAcknowledgementEntryKind::Message,
                        client_nonce: "cleanup-send-a".into(),
                    }],
                )
                .unwrap();
            owner
                .mark_queued(
                    None,
                    "session:test:android",
                    "mahayana-assistant",
                    &record.nonce,
                    record.generation,
                )
                .unwrap();
            owner
                .mark_dispatching(
                    None,
                    "session:test:android",
                    "mahayana-assistant",
                    &record.nonce,
                    record.generation,
                )
                .unwrap();
            owner
                .mark_accepted(
                    None,
                    "session:test:android",
                    "mahayana-assistant",
                    &record.nonce,
                    record.generation,
                )
                .unwrap()
        };

        let cleared = host
            .dispatch(
                "feature.sendAcknowledgement.clearApprovals",
                &json!({
                    "agentId":"mahayana-assistant",
                    "nonce":accepted.nonce,
                    "generation":accepted.generation,
                    "approvalId":approval_id,
                    "approvalOperationId":old_operation,
                }),
            )
            .unwrap();
        assert_eq!(cleared["status"], "cleared");
        assert_eq!(cleared["cleared"], true);
        assert!(host
            .dispatch(
                "feature.approval.resolve",
                &json!({"approvalId":approval_id,"approved":true}),
            )
            .is_err());

        let next = host
            .dispatch(
                "feature.execute",
                &json!({"command":{
                    "type":"capability.request",
                    "requestId":"approval-cleanup-failure",
                    "capability":"microphone"
                }}),
            )
            .unwrap();
        let next_operation = next["operationId"].as_str().unwrap().to_string();
        let next_approval = (0..16)
            .find_map(|_| {
                let event = host.dispatch("feature.receive", &json!({})).ok()?;
                (event["type"] == "approval.requested"
                    && event["operationId"] == next_operation)
                    .then_some(event)
            })
            .expect("next matching approval requested");
        let next_approval_id = next_approval["approvalId"].as_str().unwrap().to_string();

        let failed_cleanup = host.dispatch(
            "feature.sendAcknowledgement.clearApprovals",
            &json!({
                "agentId":"mahayana-assistant",
                "nonce":accepted.nonce,
                "generation":accepted.generation,
                "approvalId":next_approval_id,
                "approvalOperationId":"wrong-operation",
            }),
        );
        assert!(failed_cleanup.is_err());
        assert_eq!(
            host.send_acknowledgements
                .lock()
                .unwrap()
                .record(
                    None,
                    "session:test:android",
                    "mahayana-assistant",
                    &accepted.nonce,
                )
                .unwrap()
                .phase,
            SendAcknowledgementPhase::AcceptedAwaitingEcho,
        );
        let denied = host
            .dispatch(
                "feature.approval.resolve",
                &json!({"approvalId":next_approval_id,"approved":false}),
            )
            .unwrap();
        assert_eq!(denied["operationId"], next_operation);
        assert_eq!(denied["execution"], "denied");

        let unaccepted = {
            let mut owner = host.send_acknowledgements.lock().unwrap();
            owner
                .begin(
                    None,
                    "session:test:android",
                    "mahayana-assistant",
                    "cleanup-send-pending",
                    "still pending",
                    vec![SendAcknowledgementEntry {
                        id: "cleanup-send-pending".into(),
                        kind: crate::send_acknowledgement::SendAcknowledgementEntryKind::Message,
                        client_nonce: "cleanup-send-pending".into(),
                    }],
                )
                .unwrap()
        };
        assert!(host
            .dispatch(
                "feature.sendAcknowledgement.clearApprovals",
                &json!({
                    "agentId":"mahayana-assistant",
                    "nonce":unaccepted.nonce,
                    "generation":unaccepted.generation,
                }),
            )
            .is_err());
    }

}

#[cfg(test)]
mod transcript_page_contract_tests {
    use super::*;

    #[test]
    fn account_telemetry_outbox_cap_matches_desktop_preinstall_buffer() {
        assert_eq!(ACCOUNT_TELEMETRY_OUTBOX_CAP, 16);
    }

    #[test]
    fn protected_store_unavailable_is_recorded_by_host_session_telemetry_owner() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let recorded = host
            .dispatch(
                "feature.account.telemetry.protectedStoreUnavailable",
                &json!({}),
            )
            .unwrap();
        assert_eq!(recorded["recorded"], true);

        let snapshot = host
            .dispatch("feature.account.telemetry.snapshot", &json!({}))
            .unwrap();
        let entry = snapshot
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["metadata"]["phase"] == "keychain_unavailable")
            .expect("protected store failure telemetry");
        assert_eq!(entry["stream"], "session");
        assert_eq!(entry["level"], "warn");
        assert_eq!(entry["metadata"]["error_code"], "SAND-E0219");
        assert_eq!(entry["metadata"]["error_retryable"], "false");
    }

    #[test]
    fn host_content_search_reads_canonical_transcript_and_reopens() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        {
            let mut transcript = host.transcript.lock().unwrap();
            transcript
                .append_entry(json!({
                    "id":"search-a",
                    "agentId":"agent-a",
                    "kind":"message",
                    "role":"user",
                    "content":"Find the launch checklist",
                    "timestampMs":10,
                }))
                .unwrap();
            transcript
                .append_entry(json!({
                    "id":"search-peer",
                    "agentId":"agent-b",
                    "kind":"message",
                    "role":"assistant",
                    "content":"launch checklist internal copy",
                    "toAgent":{"kind":"agent","id":"agent-a"},
                    "timestampMs":11,
                }))
                .unwrap();
        }
        let first = host
            .dispatch(
                "feature.contentSearch.messages",
                &json!({"query":"launch checklist","limit":50}),
            )
            .unwrap();
        assert_eq!(first.as_array().unwrap().len(), 1);
        assert_eq!(first[0]["entryId"], "search-a");
        drop(host);

        let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let second = reopened
            .dispatch(
                "feature.contentSearch.messages",
                &json!({"query":"launch checklist","limit":50}),
            )
            .unwrap();
        assert_eq!(second.as_array().unwrap().len(), 1);
        assert!(reopened
            .dispatch(
                "feature.contentSearch.messages",
                &json!({"query":"launch","limit":0}),
            )
            .is_err());
    }

    #[test]
    fn host_media_search_reads_canonical_transcript_and_reopens() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        host.transcript
            .lock()
            .unwrap()
            .append_entry(json!({
                "id":"media-a",
                "agentId":"agent-a",
                "kind":"user-attachment",
                "filePath":"/tmp/Launch-Board.png",
                "timestampMs":9,
                "width":320,
                "height":200
            }))
            .unwrap();
        let first = host
            .dispatch(
                "feature.contentSearch.media",
                &json!({"query":"launch bo","limit":50}),
            )
            .unwrap();
        assert_eq!(first.as_array().unwrap().len(), 1);
        assert_eq!(first[0]["entryId"], "media-a");
        assert_eq!(first[0]["kind"], "image");
        drop(host);

        let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let second = reopened
            .dispatch(
                "feature.contentSearch.media",
                &json!({"query":"launch","limit":50}),
            )
            .unwrap();
        assert_eq!(second.as_array().unwrap().len(), 1);
        assert_eq!(second[0]["fileName"], "Launch-Board.png");
    }

    #[test]
    fn host_transcript_page_is_agent_scoped_exclusive_and_reopens() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        {
            let mut transcript = host.transcript.lock().unwrap();
            for (id, agent_id) in [
                ("a-1", "agent-a"),
                ("b-1", "agent-b"),
                ("a-2", "agent-a"),
                ("a-3", "agent-a"),
            ] {
                transcript
                    .append_entry(json!({
                        "id":id,
                        "kind":"message",
                        "role":"user",
                        "agentId":agent_id,
                        "content":id,
                    }))
                    .unwrap();
            }
        }

        let latest = host
            .dispatch(
                "feature.transcript.page",
                &json!({"agentId":"agent-a","limit":2}),
            )
            .unwrap();
        let latest_entries = latest["entries"].as_array().unwrap();
        assert_eq!(latest_entries[0]["id"], "a-2");
        assert_eq!(latest_entries[1]["id"], "a-3");
        assert_eq!(latest["nextBeforeSeq"], 1);

        drop(host);
        let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let older = reopened
            .dispatch(
                "feature.transcript.page",
                &json!({"agentId":"agent-a","limit":2,"beforeSeq":1}),
            )
            .unwrap();
        assert_eq!(older["entries"].as_array().unwrap().len(), 1);
        assert_eq!(older["entries"][0]["id"], "a-1");
        assert!(older["nextBeforeSeq"].is_null());

        assert!(reopened
            .dispatch(
                "feature.transcript.page",
                &json!({"agentId":"agent-a","limit":0}),
            )
            .is_err());
    }
}



#[cfg(test)]
mod time_zone_settings_dispatch_tests {
    use super::*;

    #[test]
    fn time_zone_override_is_durable_clearable_and_rejects_invalid_types() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        assert!(host
            .dispatch("feature.settings.timeZone", &json!({}))
            .unwrap()["overrideTimeZone"]
            .is_null());

        let saved = host
            .dispatch(
                "feature.settings.timeZone.set",
                &json!({"overrideTimeZone":"America/Los_Angeles"}),
            )
            .unwrap();
        assert_eq!(saved["overrideTimeZone"], "America/Los_Angeles");
        assert!(host
            .dispatch(
                "feature.settings.timeZone.set",
                &json!({"overrideTimeZone":42}),
            )
            .is_err());
        assert_eq!(
            host.dispatch(
                "feature.settings.timeZone.detected",
                &json!({"detectedTimeZone":"America/New_York"}),
            )
            .unwrap()["detectedTimeZone"],
            "America/New_York",
        );
        assert_eq!(host.effective_time_zone().unwrap().as_deref(), Some("America/Los_Angeles"));
        assert!(host
            .dispatch(
                "feature.settings.timeZone.detected",
                &json!({"detectedTimeZone":"bad zone"}),
            )
            .is_err());

        drop(host);
        let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        assert_eq!(
            reopened
                .dispatch("feature.settings.timeZone", &json!({}))
                .unwrap()["overrideTimeZone"],
            "America/Los_Angeles",
        );
        assert!(reopened
            .dispatch(
                "feature.settings.timeZone.set",
                &json!({"overrideTimeZone":null}),
            )
            .unwrap()["overrideTimeZone"]
            .is_null());
        assert_eq!(
            reopened.effective_time_zone().unwrap().as_deref(),
            None,
            "detected time zone is process-owned and must be re-synchronized after process death",
        );
    }
}


#[cfg(test)]
mod process_metrics_recent_dispatch_tests {
    use super::*;

    #[test]
    fn process_metrics_recent_sink_is_bounded_ordered_and_process_local() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        for index in 0..40_u64 {
            host.dispatch(
                "feature.telemetry.processMetrics.observe",
                &json!({
                    "observedAtMs": 1_000 + index * 300_000,
                    "cpuTimeMsCumulative": 10_000 + index * 100,
                    "memoryBytes": (64 + index) * 1024 * 1024,
                }),
            )
            .unwrap();
        }
        let recent = host
            .dispatch("feature.telemetry.processMetrics.recent", &json!({}))
            .unwrap()
            .as_array()
            .cloned()
            .unwrap();
        assert_eq!(recent.len(), 32);
        assert_eq!(recent.first().unwrap()["sampleSeqno"], 7);
        assert_eq!(recent.last().unwrap()["sampleSeqno"], 38);
        assert!(recent.iter().all(|sample| sample.get("sessionId").is_some()));
        assert!(recent.iter().all(|sample| sample.get("memoryMb").is_some()));
        assert!(recent.iter().all(|sample| sample.get("cpuTimeMsSample").is_some()));

        drop(host);
        let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        assert_eq!(
            reopened
                .dispatch("feature.telemetry.processMetrics.recent", &json!({}))
                .unwrap(),
            json!([]),
            "diagnostic samples are process-local and must not replay after process death",
        );
    }
}

#[cfg(test)]
mod event_loop_pressure_dispatch_tests {
    use super::*;

    #[test]
    fn event_loop_pressure_records_only_elevated_windows_and_rejects_invalid_values() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);

        let quiet = host
            .dispatch(
                "feature.telemetry.eventLoopPressure",
                &json!({"p50Ms":10,"p95Ms":99,"maxMs":120,"windowMs":60_000}),
            )
            .unwrap();
        assert_eq!(quiet["recorded"], false);

        let elevated = host
            .dispatch(
                "feature.telemetry.eventLoopPressure",
                &json!({"p50Ms":25,"p95Ms":150,"maxMs":500,"windowMs":60_000}),
            )
            .unwrap();
        assert_eq!(elevated["recorded"], true);

        let durable = std::fs::read_to_string(
            root.path().join("android-structured-log-spill.v1.json"),
        )
        .unwrap();
        assert!(durable.contains("sand.desktop.event_loop"));
        assert!(durable.contains("elevated"));

        assert!(host
            .dispatch(
                "feature.telemetry.eventLoopPressure",
                &json!({"p50Ms":0,"p95Ms":-1,"maxMs":0,"windowMs":60_000}),
            )
            .is_err());
    }
}

#[cfg(test)]
mod process_crash_telemetry_dispatch_tests {
    use super::*;

    #[test]
    fn recovered_crash_marks_prior_session_without_contaminating_current_marker() {
        let root = tempfile::tempdir().unwrap();
        let marker_path = root.path().join("host-session-marker.json");
        let (mut prior, previous) =
            crate::process_crash_guard::AndroidUncleanExitMarker::open(&marker_path, 1_000)
                .unwrap();
        assert!(previous.is_none());
        prior.heartbeat(1_250).unwrap();
        // Simulate process death: Drop would be a clean shutdown and intentionally remove marker.
        std::mem::forget(prior);

        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        host.dispatch(
            "feature.telemetry.processCrash.record",
            &json!({
                "kind":"uncaught_exception",
                "errorClass":"IllegalStateException",
                "occurredAtMs":1_250,
                "uptimeMs":250
            }),
        )
        .unwrap();

        let recent = host
            .dispatch("feature.telemetry.processCrash.recent", &json!({}))
            .unwrap();
        let previous_unclean = recent
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["kind"] == "unclean_exit")
            .expect("prior unclean session must remain projected");
        assert_eq!(previous_unclean["crashSeen"], true);

        let current_marker = std::fs::read_to_string(&marker_path).unwrap();
        assert!(
            current_marker.contains("\"crashSeen\":false"),
            "replayed previous-process crash must not taint current marker: {current_marker}"
        );
    }

    #[test]
    fn send_settlement_telemetry_uses_durable_host_timing_without_persisting_message_body() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let record = {
            let mut owner = host.send_acknowledgements.lock().unwrap();
            let begun = owner
                .begin(
                    None,
                    "session:test:android",
                    "mahayana-assistant",
                    "telemetry-send-a",
                    "secret-send-body",
                    vec![SendAcknowledgementEntry {
                        id: "telemetry-send-a".into(),
                        kind: crate::send_acknowledgement::SendAcknowledgementEntryKind::Message,
                        client_nonce: "telemetry-send-a".into(),
                    }],
                )
                .unwrap();
            owner
                .mark_queued(
                    None,
                    "session:test:android",
                    "mahayana-assistant",
                    &begun.nonce,
                    begun.generation,
                )
                .unwrap();
            owner
                .mark_dispatching(
                    None,
                    "session:test:android",
                    "mahayana-assistant",
                    &begun.nonce,
                    begun.generation,
                )
                .unwrap()
        };
        let observed_at_ms = record
            .dispatch_started_at_ms
            .expect("dispatch timing is durable")
            .saturating_add(25);
        host.record_send_settlement_telemetry(&record, "acked", observed_at_ms);

        let durable =
            std::fs::read_to_string(root.path().join("android-structured-log-spill.v1.json"))
                .unwrap();
        assert!(durable.contains("sand.send_ack"));
        assert!(durable.contains("sand.send"));
        assert!(durable.contains("telemetry-send-a"));
        assert!(!durable.contains("secret-send-body"));
    }

    #[test]
    fn cold_start_unclean_session_is_persisted_as_structured_telemetry() {
        let root = tempfile::tempdir().unwrap();
        let marker_path = root.path().join("host-session-marker.json");
        let (mut prior, previous) =
            crate::process_crash_guard::AndroidUncleanExitMarker::open_with_version(
                &marker_path,
                1_000,
                "1.2.3",
            )
            .unwrap();
        assert!(previous.is_none());
        prior.heartbeat(1_250).unwrap();
        std::mem::forget(prior);

        let host = AndroidJsonHost::new_with_account_session_and_device_id_and_app_version(
            root.path(),
            AndroidHostMode::Test,
            None,
            "test-device",
            "1.2.4",
        );
        let spill_path = root.path().join("android-structured-log-spill.v1.json");
        let durable = std::fs::read_to_string(&spill_path).unwrap();
        assert!(durable.contains("sand.desktop.unclean_exit"));
        assert!(durable.contains("prev_uptime_ms"));
        assert!(durable.contains("prev_version"));
        assert!(durable.contains("1.2.3"));
        assert!(durable.contains("same_version"));
        assert!(durable.contains("SAND-E0610"));
        drop(host);
    }

    #[test]
    fn feedback_submit_rejects_stale_account_and_invalid_payload_before_backend() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let stale = host
            .dispatch(
                "feature.feedback.submit",
                &json!({
                    "accountFence":"stale-account",
                    "message":"hello",
                    "submissionId":"123e4567-e89b-12d3-a456-426614174000"
                }),
            )
            .unwrap();
        assert_eq!(stale, json!({"ok":false,"code":"not-signed-in"}));

        // Test Host has no authenticated account; invalid feedback must never escape as a
        // successful side effect or be auto-retried by presentation.
        let invalid = host
            .dispatch(
                "feature.feedback.submit",
                &json!({
                    "accountFence":"stale-account",
                    "message":"",
                    "submissionId":"not-a-uuid"
                }),
            )
            .unwrap();
        assert_eq!(invalid, json!({"ok":false,"code":"not-signed-in"}));
    }

    fn process_crash_projection_is_bounded_and_excludes_error_content() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        for index in 0..(PROCESS_CRASH_EVENT_CAP + 5) {
            host.dispatch(
                "feature.telemetry.processCrash.record",
                &json!({
                    "kind":"uncaught_exception",
                    "errorClass":"IllegalStateException",
                    "occurredAtMs":1_000 + index as u64,
                    "uptimeMs":u64::MAX,
                    "message":"secret-token-must-not-be-accepted",
                    "stack":"sensitive-stack"
                }),
            )
            .unwrap();
        }
        let recent = host
            .dispatch("feature.telemetry.processCrash.recent", &json!({}))
            .unwrap();
        let rows = recent.as_array().unwrap();
        assert_eq!(rows.len(), PROCESS_CRASH_EVENT_CAP);
        assert_eq!(rows.first().unwrap()["occurredAtMs"], 1_005);
        assert_eq!(rows.last().unwrap()["errorCode"], "SAND-E0603");
        assert_eq!(rows.last().unwrap()["errorClass"], "IllegalStateException");
        let serialized = recent.to_string();
        assert!(!serialized.contains("secret-token"));
        assert!(!serialized.contains("sensitive-stack"));

        let spill_path = root.path().join("android-structured-log-spill.v1.json");
        let durable = std::fs::read_to_string(&spill_path).unwrap();
        assert!(durable.contains("sand.desktop.process_crash"));
        assert!(!durable.contains("secret-token"));
        assert!(!durable.contains("sensitive-stack"));
        drop(host);
        let loaded = crate::android_structured_log_spill::FileAndroidStructuredLogSpill::new(
            &spill_path,
        )
        .load("logged-out", now_ms())
        .unwrap();
        let state = match loaded {
            crate::android_structured_log_spill::SpillLoad::Loaded(state) => state,
            other => panic!("expected durable process-crash spill, got {other:?}"),
        };
        assert_eq!(
            state
                .checkpoint
                .records
                .iter()
                .filter(|record| record.message == "sand.desktop.process_crash")
                .count(),
            PROCESS_CRASH_EVENT_CAP,
        );
        assert_eq!(
            state.checkpoint.records.last().unwrap().metadata["error_code"],
            "SAND-E0603",
        );
        assert_eq!(
            state.checkpoint.records.last().unwrap().metadata["error_class"],
            "IllegalStateException",
        );

        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        assert!(host
            .dispatch(
                "feature.telemetry.processCrash.record",
                &json!({"kind":"signal","errorClass":"Signal","occurredAtMs":1,"uptimeMs":1}),
            )
            .is_err());
        assert!(host
            .dispatch(
                "feature.telemetry.processCrash.record",
                &json!({
                    "kind":"uncaught_exception",
                    "errorClass":"IllegalStateException: secret",
                    "occurredAtMs":1,
                    "uptimeMs":1
                }),
            )
            .unwrap_err()
            .contains("error class"));
    }
}

#[cfg(test)]
mod process_metrics_settings_dispatch_tests {
    use super::*;

    #[test]
    fn process_metrics_settings_are_durable_and_disable_resets_process_local_state() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let initial = host.dispatch("feature.settings.processMetrics", &json!({})).unwrap();
        assert_eq!(initial["localEnabled"], false);
        assert_eq!(initial["backendReportingEnabled"], false);
        assert!(host
            .dispatch(
                "feature.telemetry.processMetrics.observe",
                &json!({"observedAtMs":1,"cpuTimeMsCumulative":1,"memoryBytes":1024}),
            )
            .unwrap_err()
            .contains("disabled"));

        host.dispatch(
            "feature.settings.processMetrics.set",
            &json!({"localEnabled":true}),
        )
        .unwrap();
        assert!(host
            .dispatch(
                "feature.telemetry.processMetrics.observe",
                &json!({"observedAtMs":1_000,"cpuTimeMsCumulative":1_000,"memoryBytes":64 * 1024 * 1024}),
            )
            .unwrap()
            .is_null());
        let first = host
            .dispatch(
                "feature.telemetry.processMetrics.observe",
                &json!({"observedAtMs":301_000,"cpuTimeMsCumulative":2_000,"memoryBytes":96 * 1024 * 1024}),
            )
            .unwrap();
        assert_eq!(first["sampleSeqno"], 0);
        assert_eq!(first["cpuTimeMsSample"], 1_000);
        assert_eq!(
            host.dispatch("feature.telemetry.processMetrics.recent", &json!({}))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1,
        );

        host.dispatch(
            "feature.settings.processMetrics.set",
            &json!({"localEnabled":false}),
        )
        .unwrap();
        assert_eq!(
            host.dispatch("feature.telemetry.processMetrics.recent", &json!({}))
                .unwrap(),
            json!([]),
        );
        assert!(host
            .dispatch(
                "feature.telemetry.processMetrics.observe",
                &json!({"observedAtMs":600_000,"cpuTimeMsCumulative":8_000,"memoryBytes":128 * 1024 * 1024}),
            )
            .unwrap_err()
            .contains("disabled"));

        host.dispatch(
            "feature.settings.processMetrics.set",
            &json!({"localEnabled":true}),
        )
        .unwrap();
        assert!(host
            .dispatch(
                "feature.telemetry.processMetrics.observe",
                &json!({"observedAtMs":900_000,"cpuTimeMsCumulative":9_000,"memoryBytes":32 * 1024 * 1024}),
            )
            .unwrap()
            .is_null());
        let restarted = host
            .dispatch(
                "feature.telemetry.processMetrics.observe",
                &json!({"observedAtMs":1_200_000,"cpuTimeMsCumulative":9_250,"memoryBytes":48 * 1024 * 1024}),
            )
            .unwrap();
        assert_eq!(restarted["sampleSeqno"], 0);
        assert_eq!(restarted["cpuTimeMsSample"], 250);
        assert_eq!(restarted["sessionPeakMemoryMb"], 48.0);

        drop(host);
        let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        assert_eq!(
            reopened.dispatch("feature.settings.processMetrics", &json!({}))
                .unwrap()["localEnabled"],
            true,
        );
        assert!(reopened
            .dispatch(
                "feature.settings.processMetrics.set",
                &json!({"localEnabled":"yes"}),
            )
            .is_err());
    }
}

#[cfg(test)]
mod local_tool_permission_dispatch_tests {
    use super::*;

    #[test]
    fn local_tool_permission_is_durable_and_invalid_values_do_not_mutate() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        assert_eq!(
            host.dispatch("feature.settings.localToolPermission", &json!({}))
                .unwrap()["permission"],
            "ask",
        );
        let saved = host.dispatch(
            "feature.settings.localToolPermission.set",
            &json!({"permission":"always"}),
        ).unwrap();
        assert_eq!(saved["permission"], "always");
        assert_eq!(saved["preference"], "always");
        assert!(saved["ceiling"].is_null());
        assert!(host.dispatch(
            "feature.settings.localToolPermission.set",
            &json!({"permission":"sometimes"}),
        ).is_err());
        assert_eq!(
            host.dispatch("feature.settings.localToolPermission", &json!({}))
                .unwrap()["permission"],
            "always",
        );

        drop(host);
        let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        assert_eq!(
            reopened.dispatch("feature.settings.localToolPermission", &json!({}))
                .unwrap()["permission"],
            "always",
        );
    }
}

#[cfg(test)]
mod onboarding_settings_dispatch_tests {
    use super::*;

    #[test]
    fn onboarding_seen_is_host_owned_durable_and_bound_to_current_account_scope() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        assert_eq!(
            host.dispatch("feature.settings.onboardingSeen", &json!({}))
                .unwrap()["hasSeenOnboarding"],
            false,
        );
        assert_eq!(
            host.dispatch(
                "feature.settings.onboardingSeen.set",
                &json!({
                    "hasSeenOnboarding":true,
                    "expectedAccountScope":"account-device:test:android"
                }),
            )
            .unwrap()["hasSeenOnboarding"],
            true,
        );
        drop(host);

        let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let restored = reopened
            .dispatch("feature.settings.onboardingSeen", &json!({}))
            .unwrap();
        assert_eq!(restored["hasSeenOnboarding"], true);
        assert_eq!(restored["accountScope"], "account-device:test:android");
        assert!(reopened
            .dispatch(
                "feature.settings.onboardingSeen.set",
                &json!({
                    "hasSeenOnboarding":"yes",
                    "expectedAccountScope":"account-device:test:android"
                }),
            )
            .is_err());
        assert!(reopened
            .dispatch(
                "feature.settings.onboardingSeen.set",
                &json!({
                    "hasSeenOnboarding":false,
                    "expectedAccountScope":"account-device:stale-account"
                }),
            )
            .unwrap_err()
            .contains("account scope changed"));
        assert_eq!(
            reopened.dispatch("feature.settings.onboardingSeen", &json!({}))
                .unwrap()["hasSeenOnboarding"],
            true,
            "a stale account-scoped completion must not mutate the canonical setting",
        );
    }
}

#[cfg(test)]
mod auto_review_settings_dispatch_tests {
    use super::*;

    #[test]
    fn auto_review_settings_dispatch_is_account_scoped_revision_fenced_and_durable() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let initial = host
            .dispatch("feature.settings.autoReviewInstructions", &json!({}))
            .unwrap();
        assert_eq!(initial["revision"], 0);
        assert_eq!(initial["instructions"]["isEnabled"], true);

        let saved = host
            .dispatch(
                "feature.settings.autoReviewInstructions.set",
                &json!({
                    "expectedRevision":0,
                    "instructions":{
                        "isEnabled":false,
                        "allowInstructions":["  run tests  ","run tests"],
                        "blockInstructions":["delete files"]
                    }
                }),
            )
            .unwrap();
        assert_eq!(saved["revision"], 1);
        assert_eq!(saved["instructions"]["isEnabled"], false);
        assert_eq!(saved["instructions"]["allowInstructions"], json!(["run tests"]));

        assert!(host
            .dispatch(
                "feature.settings.autoReviewInstructions.set",
                &json!({
                    "expectedRevision":0,
                    "instructions":{"isEnabled":true,"allowInstructions":[],"blockInstructions":[]}
                }),
            )
            .unwrap_err()
            .contains("revision conflict"));

        drop(host);
        let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let restored = reopened
            .dispatch("feature.settings.autoReviewInstructions", &json!({}))
            .unwrap();
        assert_eq!(restored["revision"], 1);
        assert_eq!(restored["instructions"]["allowInstructions"], json!(["run tests"]));
        assert_eq!(restored["instructions"]["blockInstructions"], json!(["delete files"]));
    }
}


#[cfg(test)]
mod structured_log_spill_runtime_tests {
    use super::*;
    use crate::android_structured_log_spill::{
        FileAndroidStructuredLogSpill, SpillLoad,
    };

    #[test]
    fn sanitized_edge_failure_is_durable_without_raw_error_detail() {
        let root = tempfile::tempdir().unwrap();
        let spill_path = root.path().join("android-structured-log-spill.v1.json");
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        host.report_edge_failure(
            "android-host-jni",
            "feature.account.profileUpdate",
            "HostError secret-token-must-not-survive",
        );
        drop(host);

        let raw = std::fs::read_to_string(&spill_path).unwrap();
        assert!(!raw.contains("secret-token-must-not-survive"));

        let loaded = FileAndroidStructuredLogSpill::new(&spill_path)
            .load("logged-out", now_ms())
            .unwrap();
        let state = match loaded {
            SpillLoad::Loaded(state) => state,
            other => panic!("expected durable structured-log spill, got {other:?}"),
        };
        assert_eq!(state.checkpoint.records.len(), 1);
        let record = &state.checkpoint.records[0];
        assert_eq!(record.message, "sand.desktop.edge_failed");
        assert_eq!(record.metadata["area"], "android-host-jni");
        assert_eq!(record.metadata["leg"], "feature.account.profileUpdate");
        assert_eq!(record.metadata["error_class"], "Error");
    }
}

#[cfg(test)]
mod access_blocked_durable_dispatch_tests {
    use super::*;
    use crate::android_structured_log_spill::{FileAndroidStructuredLogSpill, SpillLoad};
    #[test]
    fn access_blocked_persists_bounded_product_event() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        host.dispatch("feature.account.telemetry.accessBlocked", &json!({"edge":"entered","cause":"seat_revoked","accessReason":"teamAccessRequired","msBlocked":u64::MAX})).unwrap();
        let state = match FileAndroidStructuredLogSpill::new(root.path().join("android-structured-log-spill.v1.json")).load("logged-out", now_ms()).unwrap() { SpillLoad::Loaded(state) => state, other => panic!("expected spill, got {other:?}") };
        let record = state.checkpoint.records.iter().find(|record| record.message == "sand.access_blocked").unwrap();
        assert_eq!(record.level, "warn");
        assert_eq!(record.metadata["ms_blocked"], (7_u64 * 24 * 60 * 60 * 1_000).to_string());
        assert!(host.dispatch("feature.account.telemetry.accessBlocked", &json!({"edge":"entered","cause":"secret free text","accessReason":"none"})).is_err());
    }
}


#[cfg(test)]
mod signin_gate_runtime_tests {
    use super::*;

    #[test]
    fn signin_gate_dispatch_accepts_only_bounded_desktop_taxonomy() {
        let root = tempfile::tempdir().unwrap();
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        host.dispatch("feature.account.telemetry.signinGate", &json!({"gate":"sign-in"})).unwrap();
        host.dispatch("feature.account.telemetry.signinGate", &json!({"consult":"fresh"})).unwrap();
        let snapshot = host.dispatch("feature.account.telemetry.snapshot", &json!({})).unwrap();
        let entries = snapshot.as_array().unwrap();
        assert!(entries.iter().any(|entry| entry["stream"] == "signin" && entry["metadata"]["phase"] == "boot_gate" && entry["metadata"]["gate"] == "sign-in"));
        assert!(entries.iter().any(|entry| entry["stream"] == "signin" && entry["metadata"]["phase"] == "account_consult" && entry["metadata"]["outcome"] == "fresh"));
        assert!(host.dispatch("feature.account.telemetry.signinGate", &json!({"gate":"secret-free-text"})).is_err());
        assert!(host.dispatch("feature.account.telemetry.signinGate", &json!({"gate":"sign-in","consult":"fresh"})).is_err());
    }
}


#[cfg(test)]
mod reaction_ack_runtime_tests {
    use super::*;

    fn messaging_envelope(request_id: &str, command: Value) -> Value {
        json!({
            "requestId":request_id,
            "envelope":{
                "protocolVersion":2,
                "context":{
                    "requestId":request_id,
                    "deviceId":"android:test-device",
                    "actorId":"human:android-test",
                    "sessionId":"android:test-session",
                    "sentAtMs":1
                },
                "command":command
            }
        })
    }

    #[test]
    fn canonical_reaction_ack_is_durable_after_host_mutation() {
        let root = tempfile::tempdir().unwrap();
        let spill = root.path().join("android-structured-log-spill.v1.json");
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        host.logged_in = true;

        host.dispatch(
            "feature.messaging.access.issue",
            &json!({
                "deviceId":"android:test-device",
                "sessionId":"android:test-session",
                "scopes":["messaging"]
            }),
        )
        .unwrap();

        host.dispatch(
            "feature.messaging.execute",
            &messaging_envelope(
                "profile",
                json!({"type":"upsertProfile","actor":{
                    "id":"human:android-test",
                    "kind":"human",
                    "displayName":"Android Test"
                }}),
            ),
        )
        .unwrap();
        host.dispatch(
            "feature.messaging.execute",
            &messaging_envelope(
                "conversation",
                json!({"type":"createConversation","conversation":{
                    "id":"conversation:telemetry",
                    "kind":"group",
                    "title":"Telemetry",
                    "ownerId":"human:android-test",
                    "participants":[{
                        "actorId":"human:android-test",
                        "role":"owner",
                        "joinedAtMs":1
                    }]
                }}),
            ),
        )
        .unwrap();
        let sent = host
            .dispatch(
                "feature.messaging.execute",
                &messaging_envelope(
                    "message",
                    json!({
                        "type":"sendMessage",
                        "conversationId":"conversation:telemetry",
                        "clientMessageId":"message:telemetry",
                        "content":{"type":"text","data":{"text":{"text":"hello","entities":[]}}}
                    }),
                ),
            )
            .unwrap();
        let message_id = sent["envelopes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|envelope| envelope["event"]["type"] == "messageAdded")
            .and_then(|envelope| envelope["event"]["message"]["id"].as_str())
            .unwrap()
            .to_string();

        host.dispatch(
            "feature.messaging.execute",
            &messaging_envelope(
                "reaction",
                json!({
                    "type":"setReaction",
                    "conversationId":"conversation:telemetry",
                    "messageId":message_id,
                    "reaction":{"reaction":"👍","chosenByMe":true}
                }),
            ),
        )
        .unwrap();

        let raw = std::fs::read_to_string(spill).unwrap();
        assert!(raw.contains("sand.reaction.ack"));
        assert!(raw.contains("conversation:telemetry"));
        assert!(raw.contains("thumbs_up"));
    }

    #[test]
    fn canonical_send_telemetry_records_success_and_failure() {
        let root = tempfile::tempdir().unwrap();
        let spill = root.path().join("android-structured-log-spill.v1.json");
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        host.logged_in = true;

        host.dispatch(
            "feature.messaging.access.issue",
            &json!({
                "deviceId":"android:test-device",
                "sessionId":"android:test-session",
                "scopes":["messaging"]
            }),
        )
        .unwrap();
        host.dispatch(
            "feature.messaging.execute",
            &messaging_envelope(
                "send-profile",
                json!({"type":"upsertProfile","actor":{
                    "id":"human:android-test",
                    "kind":"human",
                    "displayName":"Android Test"
                }}),
            ),
        )
        .unwrap();
        host.dispatch(
            "feature.messaging.execute",
            &messaging_envelope(
                "send-conversation",
                json!({"type":"createConversation","conversation":{
                    "id":"conversation:send-telemetry",
                    "kind":"group",
                    "title":"Send Telemetry",
                    "ownerId":"human:android-test",
                    "participants":[{
                        "actorId":"human:android-test",
                        "role":"owner",
                        "joinedAtMs":1
                    }]
                }}),
            ),
        )
        .unwrap();

        host.dispatch(
            "feature.messaging.execute",
            &messaging_envelope(
                "send-success",
                json!({
                    "type":"sendMessage",
                    "conversationId":"conversation:send-telemetry",
                    "clientMessageId":"client:send-success",
                    "content":{"type":"text","data":{"text":{"text":"hello","entities":[]}}}
                }),
            ),
        )
        .unwrap();

        let failed = host.dispatch(
            "feature.messaging.execute",
            &messaging_envelope(
                "send-failure",
                json!({
                    "type":"sendMessage",
                    "conversationId":"conversation:missing",
                    "clientMessageId":"client:send-failure",
                    "content":{"type":"text","data":{"text":{"text":"fail","entities":[]}}}
                }),
            ),
        );
        assert!(failed.is_err(), "telemetry must not convert the canonical failure to success");

        let raw = std::fs::read_to_string(spill).unwrap();
        assert!(raw.contains("sand.send_ack"));
        assert!(raw.contains("\"acked\""));
        assert!(raw.contains("\"failed\""));
        assert!(raw.contains("sand.send"));
        assert!(raw.contains("conversation:send-telemetry"));
        assert!(raw.contains("client:send-success"));
        assert!(raw.contains("client:send-failure"));
    }


    #[test]
    fn send_journal_restore_telemetry_groups_pre_reopen_phases_without_retrying() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("send-acknowledgements.json");
        {
            let mut owner = DurableSendAcknowledgementOwner::open(&path, 1).unwrap();
            let prepared = owner
                .begin(None, "session:a", "agent-a", "prepared-a", "prepared", vec![
                    SendAcknowledgementEntry {
                        id: "prepared-a".into(),
                        kind: crate::send_acknowledgement::SendAcknowledgementEntryKind::Message,
                        client_nonce: "prepared-a".into(),
                    },
                ])
                .unwrap();
            assert_eq!(prepared.phase, SendAcknowledgementPhase::Pending);

            let queued = owner
                .begin(None, "session:a", "agent-a", "queued-a", "queued", vec![
                    SendAcknowledgementEntry {
                        id: "queued-a".into(),
                        kind: crate::send_acknowledgement::SendAcknowledgementEntryKind::Message,
                        client_nonce: "queued-a".into(),
                    },
                ])
                .unwrap();
            owner
                .mark_queued(None, "session:a", "agent-a", "queued-a", queued.generation)
                .unwrap();

            let dispatching = owner
                .begin(None, "session:a", "agent-a", "dispatching-a", "dispatching", vec![
                    SendAcknowledgementEntry {
                        id: "dispatching-a".into(),
                        kind: crate::send_acknowledgement::SendAcknowledgementEntryKind::Message,
                        client_nonce: "dispatching-a".into(),
                    },
                ])
                .unwrap();
            owner
                .mark_dispatching(
                    None,
                    "session:a",
                    "agent-a",
                    "dispatching-a",
                    dispatching.generation,
                )
                .unwrap();
        }

        let observed_at_ms = now_ms().saturating_add(1_000);
        let reopened = DurableSendAcknowledgementOwner::open(&path, observed_at_ms).unwrap();
        let events = restored_send_journal_telemetry(
            &reopened.restored_on_open(),
            observed_at_ms,
        );
        assert_eq!(events.len(), 3);
        let phases = events
            .iter()
            .map(|event| event.metadata.get("phase").cloned().unwrap())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            phases,
            BTreeSet::from([
                "prepared".to_string(),
                "queued".to_string(),
                "dispatching".to_string(),
            ])
        );
        assert!(events.iter().all(|event| {
            event.metadata.get("outcome").map(String::as_str) == Some("parked")
                && event.metadata.get("record_count").map(String::as_str) == Some("1")
        }));
        assert_eq!(
            reopened
                .record(None, "session:a", "agent-a", "dispatching-a")
                .unwrap()
                .phase,
            SendAcknowledgementPhase::OutcomeUnknown,
        );
    }

}

