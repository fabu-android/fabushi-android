use std::collections::{HashMap, HashSet};
use std::sync::{atomic::{AtomicU64, Ordering}, Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use sha2::{Digest, Sha256};

pub const SAND_AUTO_REVIEW_APPROVAL_TTL_MS: u64 = 10 * 60 * 1_000;
pub const SAND_AUTO_REVIEW_MAX_PENDING_PER_AGENT: usize = 4;
pub const SAND_AUTO_REVIEW_CANCELLED_REASON: &str = "The action was cancelled.";
const SAND_AUTO_REVIEW_CANCEL_POLL_MS: u64 = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandAutoReviewMode {
    Off,
    Shadow,
    Enforce,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SandAutoReviewSurface {
    HostShell,
    BoxShell,
    Mcp,
    Computer,
    AutomationWrite,
    CloudAgent,
    SubagentLaunch,
    Other(String),
}

impl SandAutoReviewSurface {
    pub fn key(&self) -> &str {
        match self {
            Self::HostShell => "hostShell",
            Self::BoxShell => "boxShell",
            Self::Mcp => "mcp",
            Self::Computer => "computer",
            Self::AutomationWrite => "automationWrite",
            Self::CloudAgent => "cloudAgent",
            Self::SubagentLaunch => "subagentLaunch",
            Self::Other(value) => value.as_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandAutoReviewResolution {
    Approved,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandAutoReviewExpiryCause {
    Ttl,
    Cancelled,
    UserRedirect,
    SettingsChange,
    SessionEnd,
    Quiesce,
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandAutoReviewExpiryPolicy {
    Park,
    Ttl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandAutoReviewModes {
    pub host_shell: SandAutoReviewMode,
    pub box_shell: SandAutoReviewMode,
    pub mcp: SandAutoReviewMode,
    pub computer: SandAutoReviewMode,
    pub automation_write: SandAutoReviewMode,
    pub cloud_agent: SandAutoReviewMode,
    pub subagent_launch: SandAutoReviewMode,
}

impl SandAutoReviewModes {
    pub fn off() -> Self {
        Self::uniform(SandAutoReviewMode::Off)
    }

    pub fn shadow() -> Self {
        Self::uniform(SandAutoReviewMode::Shadow)
    }

    pub fn enforce() -> Self {
        Self::uniform(SandAutoReviewMode::Enforce)
    }

    fn uniform(mode: SandAutoReviewMode) -> Self {
        Self {
            host_shell: mode,
            box_shell: mode,
            mcp: mode,
            computer: mode,
            automation_write: SandAutoReviewMode::Off,
            cloud_agent: mode,
            subagent_launch: mode,
        }
    }
}

pub fn resolve_sand_auto_review_modes(
    settings_enabled: bool,
    enforce_enabled: bool,
    local_override: Option<SandAutoReviewMode>,
) -> SandAutoReviewModes {
    if !settings_enabled {
        return SandAutoReviewModes::off();
    }
    if let Some(mode) = local_override {
        return SandAutoReviewModes::uniform(mode);
    }
    if enforce_enabled {
        SandAutoReviewModes::enforce()
    } else {
        SandAutoReviewModes::shadow()
    }
}

pub fn sand_auto_review_approval_expiry_policy(source: &str) -> SandAutoReviewExpiryPolicy {
    if matches!(source, "turn" | "handoff-resume") {
        SandAutoReviewExpiryPolicy::Park
    } else {
        SandAutoReviewExpiryPolicy::Ttl
    }
}

pub fn format_sand_auto_review_denied_reason(classifier_reason: &str) -> String {
    format!(
        "Auto-review blocked this action: {classifier_reason}. Do not retry the same action, and do not switch to another anonymous public file host, pastebin, disposable transfer link, or similar courier — that is the same unauthorized data-exposure crossing. Ask the user what they want next. Use a safer alternative only when it is a genuinely authorized path."
    )
}

pub fn format_sand_auto_review_interrupted_for_update_reason(
    classifier_reason: &str,
) -> String {
    format!(
        "A host update interrupted this approval request before the user could answer — the user did NOT deny it. After you resume, re-run the action; if it is blocked again, use the tool's approval-retry parameter to raise a fresh approval card. The pending review reason was: {classifier_reason}"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandAutoReviewApprovalStatus {
    Pending,
    Approved,
    Denied,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandAutoReviewApproval {
    pub id: String,
    pub agent_id: String,
    pub surface: SandAutoReviewSurface,
    pub fingerprint: String,
    pub reason: String,
    pub summary: String,
    pub command: Option<String>,
    pub proposed_rule: Option<String>,
    pub user_message_epoch: u64,
    pub host_generation: String,
    pub created_at_ms: u64,
    pub expires_at_ms: Option<u64>,
    pub status: SandAutoReviewApprovalStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandAutoReviewEvent {
    Created(SandAutoReviewApproval),
    Resolved(SandAutoReviewApproval),
    Expired {
        approval: SandAutoReviewApproval,
        cause: SandAutoReviewExpiryCause,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandAutoReviewRequest {
    pub agent_id: Option<String>,
    pub surface: SandAutoReviewSurface,
    pub fingerprint: String,
    pub reason: String,
    pub summary: String,
    pub command: Option<String>,
    pub proposed_rule: Option<String>,
    pub expiry_policy: Option<SandAutoReviewExpiryPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandAutoReviewDecision {
    Approved,
    Denied { reason: String },
}

pub struct SandAutoReviewPending {
    pub approval: SandAutoReviewApproval,
    receiver: mpsc::Receiver<SandAutoReviewDecision>,
    controller: SandAutoReviewController,
}

impl SandAutoReviewPending {
    pub fn wait(self) -> Result<SandAutoReviewDecision, mpsc::RecvError> {
        if !self.controller.has_cancellation_probe() {
            return self.receiver.recv();
        }
        loop {
            if self.controller.is_cancelled() {
                self.controller.retire(
                    &self.approval.id,
                    SandAutoReviewExpiryCause::Cancelled,
                    SandAutoReviewDecision::Denied {
                        reason: SAND_AUTO_REVIEW_CANCELLED_REASON.into(),
                    },
                );
            }
            match self
                .receiver
                .recv_timeout(Duration::from_millis(SAND_AUTO_REVIEW_CANCEL_POLL_MS))
            {
                Ok(decision) => return Ok(decision),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return self.receiver.recv(),
            }
        }
    }

    pub fn try_recv(&self) -> Result<SandAutoReviewDecision, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
}

pub enum SandAutoReviewRequestOutcome {
    Immediate(SandAutoReviewDecision),
    Pending(SandAutoReviewPending),
}

type Listener = Arc<dyn Fn(&SandAutoReviewEvent) + Send + Sync>;
type CancellationProbe = Arc<dyn Fn() -> bool + Send + Sync>;

struct PendingRecord {
    approval: SandAutoReviewApproval,
    sender: mpsc::Sender<SandAutoReviewDecision>,
    generation: u64,
}

struct ControllerState {
    pending: HashMap<String, PendingRecord>,
    listeners: HashMap<u64, Listener>,
    next_listener_id: u64,
    user_message_epoch: u64,
    quiescing_for_host_wind_down: bool,
    next_pending_generation: u64,
}

pub struct SandAutoReviewController {
    agent_id: String,
    host_generation: String,
    approval_ttl_ms: u64,
    max_pending_per_agent: usize,
    approvals_resolvable: bool,
    now: Arc<dyn Fn() -> u64 + Send + Sync>,
    random_id: Arc<dyn Fn() -> String + Send + Sync>,
    on_display_recheck_failed: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    cancellation_probe: Arc<Mutex<Option<CancellationProbe>>>,
    state: Arc<Mutex<ControllerState>>,
}

impl Clone for SandAutoReviewController {
    fn clone(&self) -> Self {
        Self {
            agent_id: self.agent_id.clone(),
            host_generation: self.host_generation.clone(),
            approval_ttl_ms: self.approval_ttl_ms,
            max_pending_per_agent: self.max_pending_per_agent,
            approvals_resolvable: self.approvals_resolvable,
            now: Arc::clone(&self.now),
            random_id: Arc::clone(&self.random_id),
            on_display_recheck_failed: self.on_display_recheck_failed.clone(),
            cancellation_probe: Arc::clone(&self.cancellation_probe),
            state: Arc::clone(&self.state),
        }
    }
}

impl SandAutoReviewController {
    pub fn new(agent_id: impl Into<String>, host_generation: impl Into<String>) -> Self {
        Self::with_options(
            agent_id,
            host_generation,
            SAND_AUTO_REVIEW_APPROVAL_TTL_MS,
            SAND_AUTO_REVIEW_MAX_PENDING_PER_AGENT,
            true,
            Arc::new(now_ms),
            Arc::new(default_approval_id),
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_options(
        agent_id: impl Into<String>,
        host_generation: impl Into<String>,
        approval_ttl_ms: u64,
        max_pending_per_agent: usize,
        approvals_resolvable: bool,
        now: Arc<dyn Fn() -> u64 + Send + Sync>,
        random_id: Arc<dyn Fn() -> String + Send + Sync>,
        on_display_recheck_failed: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            host_generation: host_generation.into(),
            approval_ttl_ms,
            max_pending_per_agent,
            approvals_resolvable,
            now,
            random_id,
            on_display_recheck_failed,
            cancellation_probe: Arc::new(Mutex::new(None)),
            state: Arc::new(Mutex::new(ControllerState {
                pending: HashMap::new(),
                listeners: HashMap::new(),
                next_listener_id: 1,
                user_message_epoch: 0,
                quiescing_for_host_wind_down: false,
                next_pending_generation: 1,
            })),
        }
    }

    pub fn epoch(&self) -> u64 {
        self.state.lock().expect("auto-review state").user_message_epoch
    }

    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    pub fn host_generation(&self) -> &str {
        &self.host_generation
    }

    pub fn report_display_recheck_failed(&self, agent_id: Option<&str>) {
        if let Some(callback) = self.on_display_recheck_failed.as_ref() {
            callback(agent_id.unwrap_or(&self.agent_id));
        }
    }

    pub fn set_cancellation_probe(
        &self,
        probe: Arc<dyn Fn() -> bool + Send + Sync>,
    ) {
        *self
            .cancellation_probe
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(probe);
    }

    fn has_cancellation_probe(&self) -> bool {
        self.cancellation_probe
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }

    fn is_cancelled(&self) -> bool {
        self.cancellation_probe
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .is_some_and(|probe| probe())
    }

    pub fn request_approval(
        &self,
        request: SandAutoReviewRequest,
    ) -> SandAutoReviewRequestOutcome {
        let agent_id = request
            .agent_id
            .clone()
            .unwrap_or_else(|| self.agent_id.clone());

        {
            if self.is_cancelled() {
                return SandAutoReviewRequestOutcome::Immediate(
                    SandAutoReviewDecision::Denied {
                        reason: SAND_AUTO_REVIEW_CANCELLED_REASON.into(),
                    },
                );
            }
            let state = self.state.lock().expect("auto-review state");
            if state.quiescing_for_host_wind_down {
                return SandAutoReviewRequestOutcome::Immediate(
                    SandAutoReviewDecision::Denied {
                        reason: format_sand_auto_review_interrupted_for_update_reason(
                            &request.reason,
                        ),
                    },
                );
            }
            if !self.approvals_resolvable {
                return SandAutoReviewRequestOutcome::Immediate(
                    SandAutoReviewDecision::Denied {
                        reason: "This action needs Auto-review approval, which isn't available in this conversation. Run it from a direct chat with the assistant.".into(),
                    },
                );
            }
            let pending_for_agent = state
                .pending
                .values()
                .filter(|record| record.approval.agent_id == agent_id)
                .count();
            if pending_for_agent >= self.max_pending_per_agent {
                return SandAutoReviewRequestOutcome::Immediate(
                    SandAutoReviewDecision::Denied {
                        reason: "Too many actions are already waiting for Auto-review approval; resolve those first.".into(),
                    },
                );
            }
        }

        let created_at_ms = (self.now)();
        let expiry_policy = request.expiry_policy.unwrap_or(SandAutoReviewExpiryPolicy::Ttl);
        let approval = {
            let state = self.state.lock().expect("auto-review state");
            SandAutoReviewApproval {
                id: (self.random_id)(),
                agent_id,
                surface: request.surface,
                fingerprint: request.fingerprint,
                reason: sanitize(&request.reason, "This action requires your approval.", 500),
                summary: sanitize(&request.summary, "Sensitive action", 500),
                command: sanitize_optional(request.command.as_deref(), 4_000, false),
                proposed_rule: sanitize_optional(request.proposed_rule.as_deref(), 500, true),
                user_message_epoch: state.user_message_epoch,
                host_generation: self.host_generation.clone(),
                created_at_ms,
                expires_at_ms: matches!(expiry_policy, SandAutoReviewExpiryPolicy::Ttl)
                    .then_some(created_at_ms.saturating_add(self.approval_ttl_ms)),
                status: SandAutoReviewApprovalStatus::Pending,
            }
        };
        let (sender, receiver) = mpsc::channel();
        let generation = {
            let mut state = self.state.lock().expect("auto-review state");
            let generation = state.next_pending_generation;
            state.next_pending_generation = state.next_pending_generation.saturating_add(1);
            state.pending.insert(
                approval.id.clone(),
                PendingRecord {
                    approval: approval.clone(),
                    sender,
                    generation,
                },
            );
            generation
        };
        self.emit(SandAutoReviewEvent::Created(approval.clone()));

        if matches!(expiry_policy, SandAutoReviewExpiryPolicy::Ttl) {
            let controller = self.clone();
            let approval_id = approval.id.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(controller.approval_ttl_ms));
                controller.retire_if_generation(
                    &approval_id,
                    generation,
                    SandAutoReviewExpiryCause::Ttl,
                    SandAutoReviewDecision::Denied {
                        reason: controller
                            .pending_reason(&approval_id)
                            .map(|reason| format_sand_auto_review_denied_reason(&reason))
                            .unwrap_or_else(|| {
                                format_sand_auto_review_denied_reason(
                                    "This action requires your approval.",
                                )
                            }),
                    },
                );
            });
        }

        SandAutoReviewRequestOutcome::Pending(SandAutoReviewPending {
            approval,
            receiver,
            controller: self.clone(),
        })
    }

    pub fn resolve_approval(
        &self,
        approval_id: &str,
        resolution: SandAutoReviewResolution,
    ) -> Option<SandAutoReviewApproval> {
        let now = (self.now)();
        let mut state = self.state.lock().expect("auto-review state");
        let valid = state.pending.get(approval_id).is_some_and(|record| {
            record.approval.host_generation == self.host_generation
                && record.approval.user_message_epoch == state.user_message_epoch
                && record.approval.expires_at_ms.is_none_or(|expires| expires > now)
        });
        if !valid {
            return None;
        }
        let record = state.pending.remove(approval_id)?;
        drop(state);
        let mut resolved = record.approval.clone();
        resolved.status = match resolution {
            SandAutoReviewResolution::Approved => SandAutoReviewApprovalStatus::Approved,
            SandAutoReviewResolution::Denied => SandAutoReviewApprovalStatus::Denied,
        };
        self.emit(SandAutoReviewEvent::Resolved(resolved.clone()));
        let decision = match resolution {
            SandAutoReviewResolution::Approved => SandAutoReviewDecision::Approved,
            SandAutoReviewResolution::Denied => SandAutoReviewDecision::Denied {
                reason: format_sand_auto_review_denied_reason(&record.approval.reason),
            },
        };
        let _ = record.sender.send(decision);
        Some(resolved)
    }

    pub fn expire_pending_approval(
        &self,
        approval_id: &str,
        cause: SandAutoReviewExpiryCause,
    ) -> bool {
        let Some(reason) = self.pending_reason(approval_id) else {
            return false;
        };
        self.retire(
            approval_id,
            cause,
            SandAutoReviewDecision::Denied {
                reason: format_sand_auto_review_denied_reason(&reason),
            },
        );
        true
    }

    pub fn get_pending_approvals(&self) -> Vec<SandAutoReviewApproval> {
        self.state
            .lock()
            .expect("auto-review state")
            .pending
            .values()
            .map(|record| record.approval.clone())
            .collect()
    }

    pub fn get_pending_approval_for_agent(
        &self,
        agent_id: &str,
    ) -> Option<SandAutoReviewApproval> {
        self.state
            .lock()
            .expect("auto-review state")
            .pending
            .values()
            .find(|record| record.approval.agent_id == agent_id)
            .map(|record| record.approval.clone())
    }

    pub fn subscribe(&self, listener: Listener) -> u64 {
        let mut state = self.state.lock().expect("auto-review state");
        let id = state.next_listener_id;
        state.next_listener_id = state.next_listener_id.saturating_add(1);
        state.listeners.insert(id, listener);
        id
    }

    pub fn unsubscribe(&self, subscription_id: u64) {
        self.state
            .lock()
            .expect("auto-review state")
            .listeners
            .remove(&subscription_id);
    }

    pub fn begin_user_message_epoch(&self) {
        {
            let mut state = self.state.lock().expect("auto-review state");
            state.user_message_epoch = state.user_message_epoch.saturating_add(1);
        }
        self.expire(SandAutoReviewExpiryCause::UserRedirect);
    }

    pub fn expire_surfaces(&self, surfaces: &HashSet<SandAutoReviewSurface>) {
        let ids = self
            .state
            .lock()
            .expect("auto-review state")
            .pending
            .iter()
            .filter(|(_, record)| surfaces.contains(&record.approval.surface))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.retire(
                &id,
                SandAutoReviewExpiryCause::SettingsChange,
                SandAutoReviewDecision::Denied {
                    reason: "Auto-review settings changed; retry the action.".into(),
                },
            );
        }
    }

    pub fn expire(&self, cause: SandAutoReviewExpiryCause) {
        let ids = self
            .state
            .lock()
            .expect("auto-review state")
            .pending
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            let reason = self
                .pending_reason(&id)
                .unwrap_or_else(|| "This action requires your approval.".into());
            self.retire(
                &id,
                cause.clone(),
                SandAutoReviewDecision::Denied {
                    reason: format_sand_auto_review_denied_reason(&reason),
                },
            );
        }
    }

    pub fn expire_for_quiesce(&self) {
        {
            let mut state = self.state.lock().expect("auto-review state");
            state.quiescing_for_host_wind_down = true;
        }
        let ids = self
            .state
            .lock()
            .expect("auto-review state")
            .pending
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            let reason = self
                .pending_reason(&id)
                .unwrap_or_else(|| "This action requires your approval.".into());
            self.retire(
                &id,
                SandAutoReviewExpiryCause::Quiesce,
                SandAutoReviewDecision::Denied {
                    reason: format_sand_auto_review_interrupted_for_update_reason(&reason),
                },
            );
        }
    }

    pub fn cancel_quiesce(&self) {
        self.state
            .lock()
            .expect("auto-review state")
            .quiescing_for_host_wind_down = false;
    }

    fn pending_reason(&self, id: &str) -> Option<String> {
        self.state
            .lock()
            .expect("auto-review state")
            .pending
            .get(id)
            .map(|record| record.approval.reason.clone())
    }

    fn retire_if_generation(
        &self,
        id: &str,
        generation: u64,
        cause: SandAutoReviewExpiryCause,
        decision: SandAutoReviewDecision,
    ) {
        let matches = self
            .state
            .lock()
            .expect("auto-review state")
            .pending
            .get(id)
            .is_some_and(|record| record.generation == generation);
        if matches {
            self.retire(id, cause, decision);
        }
    }

    fn retire(
        &self,
        id: &str,
        cause: SandAutoReviewExpiryCause,
        decision: SandAutoReviewDecision,
    ) {
        let record = self
            .state
            .lock()
            .expect("auto-review state")
            .pending
            .remove(id);
        let Some(record) = record else {
            return;
        };
        let mut approval = record.approval.clone();
        approval.status = SandAutoReviewApprovalStatus::Expired;
        self.emit(SandAutoReviewEvent::Expired { approval, cause });
        let _ = record.sender.send(decision);
    }

    fn emit(&self, event: SandAutoReviewEvent) {
        let listeners = self
            .state
            .lock()
            .expect("auto-review state")
            .listeners
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for listener in listeners {
            listener(&event);
        }
    }
}

pub fn fingerprint_sand_auto_review_target(target: &serde_json::Value) -> String {
    let serialized = serde_json::to_vec(target).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(serialized);
    format!("{:x}", hasher.finalize())
}

fn sanitize(value: &str, fallback: &str, length: usize) -> String {
    let trimmed = value.trim();
    let value = if trimmed.is_empty() { fallback } else { trimmed };
    value.chars().take(length).collect()
}

fn sanitize_optional(
    value: Option<&str>,
    length: usize,
    collapse_whitespace: bool,
) -> Option<String> {
    let value = value?;
    let normalized = if collapse_whitespace {
        value.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        value.to_string()
    };
    let trimmed = normalized.trim();
    (!trimmed.is_empty()).then(|| trimmed.chars().take(length).collect())
}

static DEFAULT_APPROVAL_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn default_approval_id() -> String {
    let counter = DEFAULT_APPROVAL_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let material = format!("{}:{now}:{counter}", std::process::id());
    let mut hasher = Sha256::new();
    hasher.update(material.as_bytes());
    format!("auto-review-{:x}", hasher.finalize())
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
