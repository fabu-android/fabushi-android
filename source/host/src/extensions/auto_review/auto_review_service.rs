use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use uuid::Uuid;

use crate::extensions::session::agent_db_serde::AwaitingUserResponse;
use crate::extensions::session::production::ProductionSessionWorkers;
use crate::runner::sand_auto_review::{
    SAND_AUTO_REVIEW_APPROVAL_TTL_MS, SAND_AUTO_REVIEW_MAX_PENDING_PER_AGENT,
    SandAutoReviewController, SandAutoReviewEvent, SandAutoReviewExpiryCause,
    SandAutoReviewMode, SandAutoReviewModes, SandAutoReviewResolution,
    resolve_sand_auto_review_modes,
};

use super::sand_auto_review_awaiting::{
    SAND_AUTO_REVIEW_AWAITING_TAB_ID, SandAutoReviewAwaitingBridge,
    SandAutoReviewAwaitingSink, SandAutoReviewAwaitingState,
};

pub const SETTLED_APPROVAL_MEMORY: usize = 256;
pub const SAND_AUTO_REVIEW_STALE: &str = "SAND_AUTO_REVIEW_STALE";
pub const SAND_AUTO_REVIEW_STALE_MESSAGE: &str =
    "This Auto-review request is no longer pending.";

pub type AutoReviewUpdateSink =
    Arc<dyn Fn(&str, Value) + Send + Sync + 'static>;
pub type AutoReviewTelemetrySink =
    Arc<dyn Fn(&SandAutoReviewEvent) + Send + Sync + 'static>;
pub type AutoReviewDisplayRecheckFailedSink =
    Arc<dyn Fn(&str) + Send + Sync + 'static>;
pub type AutoReviewExpireSweepFailedSink =
    Arc<dyn Fn(&str, &str) + Send + Sync + 'static>;

#[derive(Clone)]
pub struct ProductionAutoReviewAwaitingSink {
    sessions: Arc<ProductionSessionWorkers>,
}

impl ProductionAutoReviewAwaitingSink {
    pub fn new(sessions: Arc<ProductionSessionWorkers>) -> Self {
        Self { sessions }
    }
}

impl SandAutoReviewAwaitingSink for ProductionAutoReviewAwaitingSink {
    fn try_set_for_tab(
        &self,
        agent_id: &str,
        state: SandAutoReviewAwaitingState,
    ) {
        let awaiting = AwaitingUserResponse {
            tab_id: state.tab_id.clone(),
            reason: state.reason,
            since: state.since as f64,
        };
        let _ = self.sessions.set_agent_awaiting_user_response_for_tab(
            agent_id,
            &state.tab_id,
            Some(&awaiting),
            None,
        );
    }

    fn clear_for_tab(&self, agent_id: &str, tab_id: &str) {
        let _ = self.sessions.set_agent_awaiting_user_response_for_tab(
            agent_id,
            tab_id,
            None,
            None,
        );
    }
}

struct ControllerBinding {
    controller: Arc<SandAutoReviewController>,
    subscription_id: u64,
}

pub struct AutoReviewService {
    sessions: Arc<ProductionSessionWorkers>,
    awaiting: Mutex<SandAutoReviewAwaitingBridge<ProductionAutoReviewAwaitingSink>>,
    controllers: Mutex<HashMap<String, ControllerBinding>>,
    settled: Mutex<(HashSet<String>, VecDeque<String>)>,
    on_update: AutoReviewUpdateSink,
    telemetry: AutoReviewTelemetrySink,
    display_recheck_failed: Mutex<Option<AutoReviewDisplayRecheckFailedSink>>,
    expire_sweep_failed: Mutex<Option<AutoReviewExpireSweepFailedSink>>,
    host_generation: String,
}

impl AutoReviewService {
    pub fn new(
        sessions: Arc<ProductionSessionWorkers>,
        host_generation: impl Into<String>,
        on_update: AutoReviewUpdateSink,
        telemetry: AutoReviewTelemetrySink,
    ) -> Arc<Self> {
        let awaiting_sink =
            ProductionAutoReviewAwaitingSink::new(Arc::clone(&sessions));
        Arc::new(Self {
            sessions,
            awaiting: Mutex::new(SandAutoReviewAwaitingBridge::new(awaiting_sink)),
            controllers: Mutex::new(HashMap::new()),
            settled: Mutex::new((HashSet::new(), VecDeque::new())),
            on_update,
            telemetry,
            display_recheck_failed: Mutex::new(None),
            expire_sweep_failed: Mutex::new(None),
            host_generation: host_generation.into(),
        })
    }

    pub fn set_display_recheck_failed_sink(
        &self,
        sink: AutoReviewDisplayRecheckFailedSink,
    ) {
        *self
            .display_recheck_failed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(sink);
    }

    pub fn set_expire_sweep_failed_sink(
        &self,
        sink: AutoReviewExpireSweepFailedSink,
    ) {
        *self
            .expire_sweep_failed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(sink);
    }

    fn report_expire_sweep_failed(&self, stage: &str) {
        let sink = self
            .expire_sweep_failed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if let Some(sink) = sink {
            sink(stage, "Error");
        }
    }

    pub fn bind_runner(
        self: &Arc<Self>,
        agent_id: &str,
        approvals_resolvable: bool,
    ) -> Arc<SandAutoReviewController> {
        self.unbind_runner(agent_id, SandAutoReviewExpiryCause::SessionEnd);

        let weak_display: Weak<Self> = Arc::downgrade(self);
        let controller = Arc::new(SandAutoReviewController::with_options(
            agent_id,
            self.host_generation.clone(),
            SAND_AUTO_REVIEW_APPROVAL_TTL_MS,
            SAND_AUTO_REVIEW_MAX_PENDING_PER_AGENT,
            approvals_resolvable,
            Arc::new(now_ms),
            Arc::new(|| Uuid::new_v4().to_string()),
            Some(Arc::new(move |agent_id| {
                let Some(service) = weak_display.upgrade() else {
                    return;
                };
                let sink = service
                    .display_recheck_failed
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone();
                if let Some(sink) = sink {
                    sink(agent_id);
                }
            })),
        ));
        let weak: Weak<Self> = Arc::downgrade(self);
        let subscription_id = controller.subscribe(Arc::new(move |event| {
            if let Some(service) = weak.upgrade() {
                service.handle_event(event);
            }
        }));
        self.controllers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                agent_id.to_string(),
                ControllerBinding {
                    controller: Arc::clone(&controller),
                    subscription_id,
                },
            );
        controller
    }

    pub fn current_modes(
        settings_enabled: bool,
        enforce_enabled: bool,
        local_override: Option<SandAutoReviewMode>,
    ) -> SandAutoReviewModes {
        resolve_sand_auto_review_modes(
            settings_enabled,
            enforce_enabled,
            local_override,
        )
    }

    pub fn resolve_approval(
        &self,
        request_id: &str,
        resolution: SandAutoReviewResolution,
        agent_id: &str,
    ) -> Result<(), String> {
        self.resolve_approval_for_entry(
            request_id,
            resolution,
            agent_id,
            request_id,
        )
    }

    pub fn resolve_approval_for_entry(
        &self,
        request_id: &str,
        resolution: SandAutoReviewResolution,
        agent_id: &str,
        entry_id: &str,
    ) -> Result<(), String> {
        let owner = {
            let controllers = self
                .controllers
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            controllers
                .values()
                .find(|binding| {
                    binding
                        .controller
                        .get_pending_approvals()
                        .iter()
                        .any(|approval| approval.id == request_id)
                })
                .map(|binding| Arc::clone(&binding.controller))
        };
        if let Some(owner) = owner {
            if owner.resolve_approval(request_id, resolution).is_some() {
                return Ok(());
            }
        }

        if self
            .settled
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .0
            .contains(request_id)
        {
            return Ok(());
        }

        let entries = self.sessions.read_agent_transcript_entries(agent_id)?;
        let exact_pending_card = entries.iter().any(|entry| {
            entry.get("id").and_then(Value::as_str) == Some(entry_id)
                && entry
                    .pointer("/message/approval/requestId")
                    .and_then(Value::as_str)
                    == Some(request_id)
                && entry
                    .pointer("/message/approval/status")
                    .and_then(Value::as_str)
                    == Some("pending")
        });
        if exact_pending_card {
            let retired = self
                .sessions
                .expire_pending_auto_review_approvals(agent_id, Some(request_id))?;
            if retired.iter().any(|id| id == request_id) {
                return Ok(());
            }
        }

        Err(format!(
            "{SAND_AUTO_REVIEW_STALE}: {SAND_AUTO_REVIEW_STALE_MESSAGE}"
        ))
    }

    pub fn expire_pending_approval(
        &self,
        request_id: &str,
        agent_id: &str,
        cause: SandAutoReviewExpiryCause,
    ) -> Result<bool, String> {
        let owner = {
            let controllers = self
                .controllers
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            controllers
                .values()
                .find(|binding| {
                    binding
                        .controller
                        .get_pending_approvals()
                        .iter()
                        .any(|approval| {
                            approval.id == request_id && approval.agent_id == agent_id
                        })
                })
                .map(|binding| Arc::clone(&binding.controller))
        };
        if let Some(owner) = owner {
            if owner.expire_pending_approval(request_id, cause) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn pending_approval_ids_for_agent(&self, agent_id: &str) -> Vec<String> {
        let controllers = self
            .controllers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut ids = controllers
            .values()
            .flat_map(|binding| binding.controller.get_pending_approvals())
            .filter(|approval| approval.agent_id == agent_id)
            .map(|approval| approval.id)
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn agent_ids_with_pending_approvals(&self) -> Vec<String> {
        let controllers = self
            .controllers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut ids = HashSet::new();
        for binding in controllers.values() {
            for approval in binding.controller.get_pending_approvals() {
                ids.insert(approval.agent_id);
            }
        }
        let mut ids = ids.into_iter().collect::<Vec<_>>();
        ids.sort();
        ids
    }

    pub fn pending_awaiting_state(
        &self,
        agent_id: &str,
    ) -> Option<SandAutoReviewAwaitingState> {
        self.awaiting
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pending_awaiting_state(agent_id)
    }

    pub fn sweep_stale_boot_state(&self, if_since_before_ms: u64) {
        let agent_ids = match self.sessions.list_agent_record_ids() {
            Ok(agent_ids) => agent_ids,
            Err(_) => {
                self.report_expire_sweep_failed("list_agent_record_ids");
                return;
            }
        };
        for agent_id in agent_ids {
            if self
                .sessions
                .expire_pending_auto_review_approvals(&agent_id, None)
                .is_err()
            {
                self.report_expire_sweep_failed("expire_pending_auto_review_approvals");
            }
            if self
                .sessions
                .set_agent_awaiting_user_response_for_tab(
                    &agent_id,
                    SAND_AUTO_REVIEW_AWAITING_TAB_ID,
                    None,
                    Some(if_since_before_ms as f64),
                )
                .is_err()
            {
                self.report_expire_sweep_failed("clear_awaiting_user_response");
            }
        }
    }

    pub fn unbind_runner(
        &self,
        agent_id: &str,
        cause: SandAutoReviewExpiryCause,
    ) {
        let binding = self
            .controllers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(agent_id);
        if let Some(binding) = binding {
            binding.controller.expire(cause);
            binding.controller.unsubscribe(binding.subscription_id);
        }
    }

    pub fn stop(&self) {
        let bindings = {
            let mut controllers = self
                .controllers
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            controllers
                .drain()
                .map(|(_, binding)| binding)
                .collect::<Vec<_>>()
        };
        for binding in bindings {
            binding
                .controller
                .expire(SandAutoReviewExpiryCause::SessionEnd);
            binding.controller.unsubscribe(binding.subscription_id);
        }
    }

    fn handle_event(&self, event: &SandAutoReviewEvent) {
        self.awaiting
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .handle_event(event);
        self.persist_event(event);
        (self.telemetry)(event);

        let approval = match event {
            SandAutoReviewEvent::Created(approval)
            | SandAutoReviewEvent::Resolved(approval)
            | SandAutoReviewEvent::Expired { approval, .. } => approval,
        };
        let update = match event {
            SandAutoReviewEvent::Created(_) => json!({
                "type": "send-message",
                "message": {
                    "type": "auto-review-approval",
                    "approval": approval_projection(approval),
                },
                "timestampMs": now_ms(),
            }),
            SandAutoReviewEvent::Resolved(_) => json!({
                "type": "auto-review-status",
                "requestId": approval.id,
                "status": approval_status(approval),
            }),
            SandAutoReviewEvent::Expired { .. } => json!({
                "type": "auto-review-status",
                "requestId": approval.id,
                "status": "expired",
            }),
        };
        (self.on_update)(&approval.agent_id, update);

        if matches!(event, SandAutoReviewEvent::Resolved(_)) {
            self.remember_settled(&approval.id);
        }
    }

    fn persist_event(&self, event: &SandAutoReviewEvent) {
        let approval = match event {
            SandAutoReviewEvent::Created(approval)
            | SandAutoReviewEvent::Resolved(approval)
            | SandAutoReviewEvent::Expired { approval, .. } => approval,
        };

        match event {
            SandAutoReviewEvent::Created(_) => {
                let entry = json!({
                    "id": approval.id,
                    "kind": "send-message",
                    "message": {
                        "type": "auto-review-approval",
                        "approval": approval_projection(approval),
                    },
                    "timestampMs": now_ms(),
                });
                let _ = self
                    .sessions
                    .append_agent_transcript_entries(&approval.agent_id, &[entry]);
            }
            SandAutoReviewEvent::Resolved(_)
            | SandAutoReviewEvent::Expired { .. } => {
                let Ok(entries) =
                    self.sessions.read_agent_transcript_entries(&approval.agent_id)
                else {
                    return;
                };
                let Some(mut entry) = entries.into_iter().find(|entry| {
                    entry.get("id").and_then(Value::as_str)
                        == Some(approval.id.as_str())
                }) else {
                    return;
                };
                if let Some(status) = entry
                    .get_mut("message")
                    .and_then(Value::as_object_mut)
                    .and_then(|message| message.get_mut("approval"))
                    .and_then(Value::as_object_mut)
                {
                    status.insert(
                        "status".into(),
                        Value::String(
                            if matches!(
                                event,
                                SandAutoReviewEvent::Expired { .. }
                            ) {
                                "expired"
                            } else {
                                approval_status(approval)
                            }
                            .into(),
                        ),
                    );
                }
                let _ = self.sessions.update_agent_transcript_entry(
                    &approval.agent_id,
                    &approval.id,
                    &entry,
                );
            }
        }
    }

    fn remember_settled(&self, id: &str) {
        let mut settled = self
            .settled
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if settled.0.insert(id.to_string()) {
            settled.1.push_back(id.to_string());
        }
        while settled.1.len() > SETTLED_APPROVAL_MEMORY {
            if let Some(oldest) = settled.1.pop_front() {
                settled.0.remove(&oldest);
            }
        }
    }
}

fn approval_projection(
    approval: &crate::runner::sand_auto_review::SandAutoReviewApproval,
) -> Value {
    let mut value = json!({
        "requestId": approval.id,
        "surface": approval.surface.key(),
        "summary": approval.summary,
        "reason": approval.reason,
        "status": approval_status(approval),
    });
    if let Some(command) = approval.command.as_deref() {
        value["command"] = Value::String(command.to_string());
    }
    if let Some(rule) = approval.proposed_rule.as_deref() {
        value["proposedRule"] = Value::String(rule.to_string());
    }
    value
}

fn approval_status(
    approval: &crate::runner::sand_auto_review::SandAutoReviewApproval,
) -> &'static str {
    use crate::runner::sand_auto_review::SandAutoReviewApprovalStatus;
    match approval.status {
        SandAutoReviewApprovalStatus::Pending => "pending",
        SandAutoReviewApprovalStatus::Approved => "approved",
        SandAutoReviewApprovalStatus::Denied => "denied",
        SandAutoReviewApprovalStatus::Expired => "expired",
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
