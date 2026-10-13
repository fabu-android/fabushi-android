use crate::automation_schedule::{compute_next_run_at, normalize_schedule};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutomationRunState {
    Running,
    AwaitingApproval,
    Completed,
    Failed,
    Cancelled,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AutomationSpec {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub schedule: String,
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub trigger_description: String,
    pub enabled: bool,
    pub account_fence: String,
    pub created_at_ms: u64,
    pub last_run_at_ms: Option<u64>,
    pub next_run_at_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AutomationRun {
    pub run_id: String,
    pub automation_id: String,
    pub request_id: String,
    pub account_fence: String,
    pub generation: u64,
    pub state: AutomationRunState,
    pub step_index: u32,
    pub step_mutation_id: Option<String>,
    pub approval_id: Option<String>,
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
    pub reason: Option<String>,
    #[serde(default = "default_automation_trigger_kind")]
    pub trigger_kind: String,
    #[serde(default)]
    pub event_batch_id: Option<String>,
    #[serde(default)]
    pub event_count: usize,
}

fn default_automation_trigger_kind() -> String {
    "manual".into()
}

pub const AUTOMATION_EVENT_DEBOUNCE_MS: u64 = 750;
pub const MAX_QUEUED_EVENT_FIRES_PER_AUTOMATION: usize = 500;
pub const MAX_EVENTS_IN_AUTOMATION_WAKE: usize = 25;
pub const MAX_SETTLED_EVENT_OCCURRENCES: usize = 1_024;
pub const MAX_AUTOMATION_EVENT_BYTES: usize = 256 * 1_024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AutomationEventFire {
    pub occurrence_id: String,
    pub automation_id: String,
    pub account_fence: String,
    pub event: Value,
    pub enqueued_at_ms: u64,
    pub available_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutomationEventBatchState {
    Claimed,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AutomationEventBatch {
    pub batch_id: String,
    pub claim_id: String,
    pub automation_id: String,
    pub account_fence: String,
    pub occurrence_ids: Vec<String>,
    pub events: Vec<Value>,
    pub claimed_at_ms: u64,
    pub state: AutomationEventBatchState,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AutomationEventEnqueueOutcome {
    pub accepted: bool,
    pub duplicate: bool,
    pub dropped_occurrence_ids: Vec<String>,
    pub pending_count: usize,
}

pub const SPEND_GUARD_IDLE_TTL_MS: u64 = 3 * 24 * 60 * 60 * 1_000;
pub const SPEND_GUARD_MIN_UNREAD_COUNT: u64 = 15;
pub const SPEND_GUARD_MIN_FIRES_SINCE_VIEWED: u64 = 20;
pub const SPEND_GUARD_PAUSE_DELAY_MS: u64 = 3 * 24 * 60 * 60 * 1_000;
pub const SPEND_GUARD_SNOOZE_MS: u64 = 30 * 24 * 60 * 60 * 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomationSpendGuardDecision {
    OptedOut,
    UserActive,
    Snoozed,
    Pause,
    AwaitingAck,
    Nudge,
    BelowThresholds,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct AutomationSpendGuardState {
    pub nudged_at_ms: Option<u64>,
    pub snoozed_until_ms: Option<u64>,
    pub opted_out: bool,
    #[serde(default)]
    pub paused_automation_ids: Vec<String>,
    pub card_entry_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutomationSpendGuardEvaluation {
    pub now_ms: u64,
    pub last_viewed_at_ms: u64,
    pub unread_count: u64,
    pub fires_since_viewed_count: u64,
}

pub fn evaluate_automation_spend_guard(
    input: AutomationSpendGuardEvaluation,
    state: &AutomationSpendGuardState,
) -> AutomationSpendGuardDecision {
    if state.opted_out {
        return AutomationSpendGuardDecision::OptedOut;
    }
    if input.now_ms.saturating_sub(input.last_viewed_at_ms) < SPEND_GUARD_IDLE_TTL_MS {
        return AutomationSpendGuardDecision::UserActive;
    }
    if state
        .snoozed_until_ms
        .is_some_and(|until| input.now_ms < until)
    {
        return AutomationSpendGuardDecision::Snoozed;
    }
    if let Some(nudged_at_ms) = state
        .nudged_at_ms
        .filter(|nudged| *nudged > input.last_viewed_at_ms)
    {
        return if input.now_ms.saturating_sub(nudged_at_ms) >= SPEND_GUARD_PAUSE_DELAY_MS {
            AutomationSpendGuardDecision::Pause
        } else {
            AutomationSpendGuardDecision::AwaitingAck
        };
    }
    if input.unread_count >= SPEND_GUARD_MIN_UNREAD_COUNT
        || input.fires_since_viewed_count >= SPEND_GUARD_MIN_FIRES_SINCE_VIEWED
    {
        return AutomationSpendGuardDecision::Nudge;
    }
    AutomationSpendGuardDecision::BelowThresholds
}

#[derive(Default, Deserialize, Serialize)]
struct DurableAutomationState {
    next_generation: u64,
    specs: BTreeMap<String, AutomationSpec>,
    runs: BTreeMap<String, AutomationRun>,
    request_to_run: BTreeMap<String, String>,
    #[serde(default)]
    next_event_batch_generation: u64,
    #[serde(default)]
    event_fires: BTreeMap<String, Vec<AutomationEventFire>>,
    #[serde(default)]
    event_batches: BTreeMap<String, AutomationEventBatch>,
    #[serde(default)]
    settled_event_occurrences: Vec<String>,
    #[serde(default)]
    spend_guard: BTreeMap<String, AutomationSpendGuardState>,
}

pub struct AutomationRuntime {
    path: PathBuf,
    state: DurableAutomationState,
}

impl AutomationRuntime {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let path = path.into();
        let mut state = if path.exists() {
            serde_json::from_slice::<DurableAutomationState>(
                &fs::read(&path).map_err(|e| e.to_string())?,
            )
            .map_err(|e| format!("invalid automation runtime store: {e}"))?
        } else {
            DurableAutomationState::default()
        };

        let mut changed = false;
        for batch in state.event_batches.values_mut() {
            if batch.state == AutomationEventBatchState::Claimed {
                batch.state = AutomationEventBatchState::OutcomeUnknown;
                changed = true;
            }
        }
        for run in state.runs.values_mut() {
            if run.state == AutomationRunState::Running {
                run.state = AutomationRunState::OutcomeUnknown;
                run.reason = Some(
                    "Host restarted while workflow step may have produced side effects; reconcile before retry"
                        .into(),
                );
                run.updated_at_ms = now_ms;
                changed = true;
            }
        }
        let runtime = Self { path, state };
        if changed {
            runtime.persist()?;
        }
        Ok(runtime)
    }

    pub fn upsert_spec(&mut self, mut spec: AutomationSpec, fallback_time_zone: Option<&str>) -> Result<(), String> {
        spec.schedule = normalize_schedule(&spec.schedule);
        validate_spec(&spec)?;
        spec.next_run_at_ms = canonical_next_run_ms(&spec, fallback_time_zone)?;
        if let Some(existing) = self.state.specs.get(&spec.id) {
            if existing.account_fence != spec.account_fence { return Err("automation id belongs to another account fence".into()); }
        }
        self.state.specs.insert(spec.id.clone(), spec);
        self.persist()
    }

    pub fn refresh_next_runs(&mut self, fallback_time_zone: Option<&str>) -> Result<(), String> {
        let mut changed = false;
        for spec in self.state.specs.values_mut() {
            let normalized = normalize_schedule(&spec.schedule);
            if normalized != spec.schedule { spec.schedule = normalized; changed = true; }
            validate_spec(spec)?;
            let next = canonical_next_run_ms(spec, fallback_time_zone)?;
            if spec.next_run_at_ms != next { spec.next_run_at_ms = next; changed = true; }
        }
        if changed { self.persist()?; }
        Ok(())
    }

    pub fn list_for_account(&self, account_fence: &str) -> Vec<Value> {
        self.state
            .specs
            .values()
            .filter(|spec| spec.account_fence == account_fence)
            .map(|spec| serde_json::to_value(spec).unwrap_or(Value::Null))
            .collect()
    }

    pub fn list_for_agent(&self, account_fence: &str, agent_id: &str) -> Vec<Value> {
        self.state
            .specs
            .values()
            .filter(|spec| spec.account_fence == account_fence && spec.agent_id == agent_id)
            .map(|spec| serde_json::to_value(spec).unwrap_or(Value::Null))
            .collect()
    }

    pub fn spec_for_account(&self, id: &str, account_fence: &str) -> Option<AutomationSpec> {
        self.state.specs.get(id).filter(|spec| spec.account_fence == account_fence).cloned()
    }

    pub fn run_count_for_automation(&self, id: &str, account_fence: &str) -> u64 {
        self.state
            .runs
            .values()
            .filter(|run| run.automation_id == id && run.account_fence == account_fence)
            .count()
            .try_into()
            .unwrap_or(u64::MAX)
    }

    fn spend_guard_key(account_fence: &str, agent_id: &str) -> String {
        format!("{account_fence}\n{agent_id}")
    }

    pub fn spend_guard_state(
        &self,
        account_fence: &str,
        agent_id: &str,
    ) -> AutomationSpendGuardState {
        self.state
            .spend_guard
            .get(&Self::spend_guard_key(account_fence, agent_id))
            .cloned()
            .unwrap_or_default()
    }

    pub fn evaluate_spend_guard(
        &self,
        account_fence: &str,
        agent_id: &str,
        last_viewed_at_ms: u64,
        unread_count: u64,
        now_ms: u64,
    ) -> Result<(AutomationSpendGuardDecision, AutomationSpendGuardEvaluation), String> {
        if account_fence.trim().is_empty() || agent_id.trim().is_empty() {
            return Err("spend guard requires account and Agent identity".into());
        }
        let owned_ids = self
            .state
            .specs
            .values()
            .filter(|spec| spec.account_fence == account_fence && spec.agent_id == agent_id)
            .map(|spec| spec.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let fires_since_viewed_count = self
            .state
            .runs
            .values()
            .filter(|run| {
                run.account_fence == account_fence
                    && owned_ids.contains(run.automation_id.as_str())
                    && run.started_at_ms > last_viewed_at_ms
            })
            .count()
            .try_into()
            .unwrap_or(u64::MAX);
        let evaluation = AutomationSpendGuardEvaluation {
            now_ms,
            last_viewed_at_ms,
            unread_count,
            fires_since_viewed_count,
        };
        let state = self.spend_guard_state(account_fence, agent_id);
        Ok((evaluate_automation_spend_guard(evaluation, &state), evaluation))
    }

    pub fn record_spend_guard_nudge(
        &mut self,
        account_fence: &str,
        agent_id: &str,
        card_entry_id: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        if card_entry_id.trim().is_empty() {
            return Err("spend guard card entry id is required".into());
        }
        let key = Self::spend_guard_key(account_fence, agent_id);
        let state = self.state.spend_guard.entry(key).or_default();
        state.nudged_at_ms = Some(now_ms);
        state.card_entry_id = Some(card_entry_id.to_string());
        state.snoozed_until_ms = None;
        self.persist()
    }

    pub fn set_spend_guard_card_entry(
        &mut self,
        account_fence: &str,
        agent_id: &str,
        card_entry_id: &str,
    ) -> Result<(), String> {
        if card_entry_id.trim().is_empty() {
            return Err("spend guard card entry id is required".into());
        }
        let key = Self::spend_guard_key(account_fence, agent_id);
        let state = self.state.spend_guard.entry(key).or_default();
        state.card_entry_id = Some(card_entry_id.to_string());
        self.persist()
    }

    pub fn pause_agent_automations_for_spend_guard(
        &mut self,
        account_fence: &str,
        agent_id: &str,
        fallback_time_zone: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let mut paused = Vec::new();
        for spec in self.state.specs.values_mut() {
            if spec.account_fence == account_fence && spec.agent_id == agent_id && spec.enabled {
                spec.enabled = false;
                spec.next_run_at_ms = canonical_next_run_ms(spec, fallback_time_zone)?;
                paused.push(spec.id.clone());
            }
        }
        paused.sort();
        let key = Self::spend_guard_key(account_fence, agent_id);
        let state = self.state.spend_guard.entry(key).or_default();
        for id in &paused {
            if !state.paused_automation_ids.contains(id) {
                state.paused_automation_ids.push(id.clone());
            }
        }
        state.paused_automation_ids.sort();
        state.paused_automation_ids.dedup();
        self.persist()?;
        Ok(state.paused_automation_ids.clone())
    }

    pub fn apply_spend_guard_answer(
        &mut self,
        account_fence: &str,
        agent_id: &str,
        card_entry_id: &str,
        value: &str,
        now_ms: u64,
        fallback_time_zone: Option<&str>,
    ) -> Result<bool, String> {
        let key = Self::spend_guard_key(account_fence, agent_id);
        let Some(current) = self.state.spend_guard.get(&key).cloned() else {
            return Ok(false);
        };
        if current.card_entry_id.as_deref() != Some(card_entry_id) {
            return Ok(false);
        }
        let mut next = AutomationSpendGuardState::default();
        match value {
            "spend-guard:keep" | "spend-guard:resume" | "spend-guard:never-ask" => {
                for id in &current.paused_automation_ids {
                    if let Some(spec) = self.state.specs.get_mut(id) {
                        if spec.account_fence == account_fence && spec.agent_id == agent_id {
                            spec.enabled = true;
                            spec.next_run_at_ms = canonical_next_run_ms(spec, fallback_time_zone)?;
                        }
                    }
                }
                if value == "spend-guard:never-ask" {
                    next.opted_out = true;
                } else {
                    next.snoozed_until_ms = Some(now_ms.saturating_add(SPEND_GUARD_SNOOZE_MS));
                }
            }
            "spend-guard:pause" => {
                for spec in self.state.specs.values_mut() {
                    if spec.account_fence == account_fence && spec.agent_id == agent_id && spec.enabled {
                        spec.enabled = false;
                        spec.next_run_at_ms = canonical_next_run_ms(spec, fallback_time_zone)?;
                        next.paused_automation_ids.push(spec.id.clone());
                    }
                }
                next.paused_automation_ids.sort();
                next.card_entry_id = Some(card_entry_id.to_string());
            }
            "spend-guard:stay-paused" => {}
            _ => return Ok(false),
        }
        self.state.spend_guard.insert(key, next);
        self.persist()?;
        Ok(true)
    }

    pub fn has_owned_spec(&self, id: &str, account_fence: &str, agent_id: &str) -> bool {
        self.state.specs.get(id).is_some_and(|spec| {
            spec.account_fence == account_fence && spec.agent_id == agent_id
        })
    }

    pub fn set_agent_workflow_enabled(
        &mut self,
        account_fence: &str,
        agent_id: &str,
        automation_id: &str,
        enabled: bool,
        fallback_time_zone: Option<&str>,
    ) -> Result<AutomationSpec, String> {
        let agent_id = agent_id.trim();
        let automation_id = automation_id.trim();
        if agent_id.is_empty() || automation_id.is_empty() {
            return Err("automation agent and workflow ids are required".into());
        }
        let spec = self
            .state
            .specs
            .get_mut(automation_id)
            .ok_or("automation not found")?;
        if spec.account_fence != account_fence {
            return Err("automation account fence mismatch".into());
        }
        if spec.agent_id != agent_id {
            return Err("automation belongs to a different Agent".into());
        }
        spec.enabled = enabled;
        spec.next_run_at_ms = canonical_next_run_ms(spec, fallback_time_zone)?;
        let copy = spec.clone();
        self.persist()?;
        Ok(copy)
    }

    fn event_queue_key(account_fence: &str, automation_id: &str) -> String {
        format!("{account_fence}\n{automation_id}")
    }

    fn event_occurrence_key(
        account_fence: &str,
        automation_id: &str,
        occurrence_id: &str,
    ) -> String {
        format!("{account_fence}\n{automation_id}\n{occurrence_id}")
    }

    fn event_occurrence_known(
        &self,
        account_fence: &str,
        automation_id: &str,
        occurrence_id: &str,
    ) -> bool {
        let key = Self::event_occurrence_key(account_fence, automation_id, occurrence_id);
        if self.state.settled_event_occurrences.iter().any(|value| value == &key) {
            return true;
        }
        if self
            .state
            .event_fires
            .get(&Self::event_queue_key(account_fence, automation_id))
            .is_some_and(|fires| fires.iter().any(|fire| fire.occurrence_id == occurrence_id))
        {
            return true;
        }
        self.state.event_batches.values().any(|batch| {
            batch.account_fence == account_fence
                && batch.automation_id == automation_id
                && batch.occurrence_ids.iter().any(|value| value == occurrence_id)
        })
    }

    pub fn enqueue_event_fire(
        &mut self,
        automation_id: &str,
        account_fence: &str,
        occurrence_id: &str,
        event: Value,
        now_ms: u64,
    ) -> Result<AutomationEventEnqueueOutcome, String> {
        let occurrence_id = occurrence_id.trim();
        if occurrence_id.is_empty() || occurrence_id.len() > 256 {
            return Err("automation event occurrence id is invalid".into());
        }
        let spec = self
            .state
            .specs
            .get(automation_id)
            .ok_or("automation not found")?;
        if !spec.enabled {
            return Err("automation is disabled".into());
        }
        if spec.account_fence != account_fence {
            return Err("automation account fence mismatch".into());
        }
        let event_bytes = serde_json::to_vec(&event).map_err(|error| error.to_string())?;
        if event_bytes.len() > MAX_AUTOMATION_EVENT_BYTES {
            return Err("automation event payload exceeds the durable queue limit".into());
        }
        if self.event_occurrence_known(account_fence, automation_id, occurrence_id) {
            let pending_count = self
                .state
                .event_fires
                .get(&Self::event_queue_key(account_fence, automation_id))
                .map(Vec::len)
                .unwrap_or(0);
            return Ok(AutomationEventEnqueueOutcome {
                accepted: false,
                duplicate: true,
                dropped_occurrence_ids: Vec::new(),
                pending_count,
            });
        }

        let queue_key = Self::event_queue_key(account_fence, automation_id);
        let queue = self.state.event_fires.entry(queue_key).or_default();
        queue.push(AutomationEventFire {
            occurrence_id: occurrence_id.to_string(),
            automation_id: automation_id.to_string(),
            account_fence: account_fence.to_string(),
            event,
            enqueued_at_ms: now_ms,
            available_at_ms: now_ms.saturating_add(AUTOMATION_EVENT_DEBOUNCE_MS),
        });
        let mut dropped_occurrence_ids = Vec::new();
        while queue.len() > MAX_QUEUED_EVENT_FIRES_PER_AUTOMATION {
            let dropped = queue.remove(0);
            dropped_occurrence_ids.push(dropped.occurrence_id);
        }
        let pending_count = queue.len();
        for occurrence_id in &dropped_occurrence_ids {
            self.remember_settled_event_occurrence(Self::event_occurrence_key(
                account_fence,
                automation_id,
                occurrence_id,
            ));
        }
        self.persist()?;
        Ok(AutomationEventEnqueueOutcome {
            accepted: true,
            duplicate: false,
            dropped_occurrence_ids,
            pending_count,
        })
    }

    pub fn claim_event_batch(
        &mut self,
        automation_id: &str,
        account_fence: &str,
        claim_id: &str,
        now_ms: u64,
    ) -> Result<Option<AutomationEventBatch>, String> {
        let claim_id = claim_id.trim();
        if claim_id.is_empty() || claim_id.len() > 256 {
            return Err("automation event claim id is invalid".into());
        }
        if let Some(existing) = self
            .state
            .event_batches
            .values()
            .find(|batch| batch.claim_id == claim_id)
        {
            if existing.account_fence != account_fence || existing.automation_id != automation_id {
                return Err("automation event claim id is already bound".into());
            }
            if existing.state == AutomationEventBatchState::OutcomeUnknown {
                return Err("automation event batch outcome is unknown; reconcile before replay".into());
            }
            return Ok(Some(existing.clone()));
        }
        if self.state.event_batches.values().any(|batch| {
            batch.account_fence == account_fence && batch.automation_id == automation_id
        }) {
            return Err("automation event batch is already claimed or outcome-unknown".into());
        }

        let queue_key = Self::event_queue_key(account_fence, automation_id);
        let Some(queue) = self.state.event_fires.get_mut(&queue_key) else {
            return Ok(None);
        };
        let ready = queue
            .iter()
            .take_while(|fire| fire.available_at_ms <= now_ms)
            .count()
            .min(MAX_EVENTS_IN_AUTOMATION_WAKE);
        if ready == 0 {
            return Ok(None);
        }
        let claimed = queue.drain(..ready).collect::<Vec<_>>();
        if queue.is_empty() {
            self.state.event_fires.remove(&queue_key);
        }
        self.state.next_event_batch_generation =
            self.state.next_event_batch_generation.saturating_add(1).max(1);
        let batch_id = format!(
            "automation-event-batch-{}",
            self.state.next_event_batch_generation
        );
        let batch = AutomationEventBatch {
            batch_id: batch_id.clone(),
            claim_id: claim_id.to_string(),
            automation_id: automation_id.to_string(),
            account_fence: account_fence.to_string(),
            occurrence_ids: claimed
                .iter()
                .map(|fire| fire.occurrence_id.clone())
                .collect(),
            events: claimed.into_iter().map(|fire| fire.event).collect(),
            claimed_at_ms: now_ms,
            state: AutomationEventBatchState::Claimed,
        };
        self.state.event_batches.insert(batch_id, batch.clone());
        self.persist()?;
        Ok(Some(batch))
    }

    fn remember_settled_event_occurrence(&mut self, key: String) {
        self.state.settled_event_occurrences.retain(|value| value != &key);
        self.state.settled_event_occurrences.push(key);
        while self.state.settled_event_occurrences.len() > MAX_SETTLED_EVENT_OCCURRENCES {
            self.state.settled_event_occurrences.remove(0);
        }
    }

    pub fn settle_event_batch(
        &mut self,
        batch_id: &str,
        account_fence: &str,
        succeeded: bool,
        now_ms: u64,
    ) -> Result<(), String> {
        let batch = self
            .state
            .event_batches
            .get(batch_id)
            .cloned()
            .ok_or("automation event batch not found")?;
        if batch.account_fence != account_fence {
            return Err("automation event batch account fence mismatch".into());
        }
        if batch.state == AutomationEventBatchState::OutcomeUnknown {
            return Err("automation event batch outcome is unknown; reconcile before settlement".into());
        }
        self.state.event_batches.remove(batch_id);
        if succeeded {
            for occurrence_id in batch.occurrence_ids {
                self.remember_settled_event_occurrence(Self::event_occurrence_key(
                    account_fence,
                    &batch.automation_id,
                    &occurrence_id,
                ));
            }
        } else {
            let automation_id = batch.automation_id.clone();
            let claimed_at_ms = batch.claimed_at_ms;
            let queue_key = Self::event_queue_key(account_fence, &automation_id);
            let queue = self.state.event_fires.entry(queue_key).or_default();
            let mut restored = batch
                .occurrence_ids
                .into_iter()
                .zip(batch.events)
                .map(|(occurrence_id, event)| AutomationEventFire {
                    occurrence_id,
                    automation_id: automation_id.clone(),
                    account_fence: account_fence.to_string(),
                    event,
                    enqueued_at_ms: claimed_at_ms,
                    available_at_ms: now_ms,
                })
                .collect::<Vec<_>>();
            restored.append(queue);
            *queue = restored;
        }
        self.persist()
    }

    pub fn reconcile_event_batch(
        &mut self,
        batch_id: &str,
        account_fence: &str,
        completed: bool,
        now_ms: u64,
    ) -> Result<(), String> {
        let batch = self
            .state
            .event_batches
            .get(batch_id)
            .cloned()
            .ok_or("automation event batch not found")?;
        if batch.account_fence != account_fence {
            return Err("automation event batch account fence mismatch".into());
        }
        if batch.state != AutomationEventBatchState::OutcomeUnknown {
            return Err("automation event batch does not require reconciliation".into());
        }
        self.state.event_batches.remove(batch_id);
        if completed {
            for occurrence_id in batch.occurrence_ids {
                self.remember_settled_event_occurrence(Self::event_occurrence_key(
                    account_fence,
                    &batch.automation_id,
                    &occurrence_id,
                ));
            }
        } else {
            let automation_id = batch.automation_id.clone();
            let claimed_at_ms = batch.claimed_at_ms;
            let queue_key = Self::event_queue_key(account_fence, &automation_id);
            let queue = self.state.event_fires.entry(queue_key).or_default();
            let mut restored = batch
                .occurrence_ids
                .into_iter()
                .zip(batch.events)
                .map(|(occurrence_id, event)| AutomationEventFire {
                    occurrence_id,
                    automation_id: automation_id.clone(),
                    account_fence: account_fence.to_string(),
                    event,
                    enqueued_at_ms: claimed_at_ms,
                    available_at_ms: now_ms,
                })
                .collect::<Vec<_>>();
            restored.append(queue);
            *queue = restored;
        }
        self.persist()
    }

    pub fn begin_run(
        &mut self,
        automation_id: &str,
        request_id: &str,
        run_id: &str,
        account_fence: &str,
        now_ms: u64,
    ) -> Result<AutomationRun, String> {
        let spec = self
            .state
            .specs
            .get(automation_id)
            .ok_or("automation not found")?;
        if !spec.enabled {
            return Err("automation is disabled".into());
        }
        if spec.account_fence != account_fence {
            return Err("automation account fence mismatch".into());
        }
        if let Some(existing_run_id) = self.state.request_to_run.get(request_id) {
            let existing = self
                .state
                .runs
                .get(existing_run_id)
                .ok_or("automation request index is corrupt")?;
            if existing.automation_id == automation_id
                && existing.account_fence == account_fence
                && matches!(
                    existing.state,
                    AutomationRunState::Completed
                        | AutomationRunState::Failed
                        | AutomationRunState::Cancelled
                )
            {
                return Ok(existing.clone());
            }
            return Err(match existing.state {
                AutomationRunState::OutcomeUnknown => {
                    "automation request outcome is unknown; reconcile before replay".into()
                }
                _ => "duplicate automation request is already active or bound".into(),
            });
        }
        if self.state.runs.contains_key(run_id) {
            return Err("duplicate automation run id".into());
        }

        self.state.next_generation = self.state.next_generation.saturating_add(1).max(1);
        let run = AutomationRun {
            run_id: run_id.into(),
            automation_id: automation_id.into(),
            request_id: request_id.into(),
            account_fence: account_fence.into(),
            generation: self.state.next_generation,
            state: AutomationRunState::Running,
            step_index: 0,
            step_mutation_id: Some(format!("{request_id}:step:0")),
            approval_id: None,
            started_at_ms: now_ms,
            updated_at_ms: now_ms,
            reason: None,
            trigger_kind: default_automation_trigger_kind(),
            event_batch_id: None,
            event_count: 0,
        };
        self.state
            .request_to_run
            .insert(request_id.into(), run_id.into());
        self.state.runs.insert(run_id.into(), run.clone());
        self.persist()?;
        Ok(run)
    }

    pub fn begin_event_run(
        &mut self,
        automation_id: &str,
        account_fence: &str,
        claim_id: &str,
        run_id: &str,
        now_ms: u64,
    ) -> Result<Option<(AutomationRun, AutomationEventBatch)>, String> {
        let Some(batch) = self.claim_event_batch(
            automation_id,
            account_fence,
            claim_id,
            now_ms,
        )? else {
            return Ok(None);
        };
        let request_id = format!("automation-event:{claim_id}");
        let run = self
            .begin_run(
                automation_id,
                &request_id,
                run_id,
                account_fence,
                now_ms,
            )
            .map_err(|error| {
                format!(
                    "automation event run outcome-unknown: batch {} was durably claimed before run settlement failed; reconcile before replay: {error}",
                    batch.batch_id
                )
            })?;
        let current = self.current_mut(&run.run_id, account_fence, run.generation)?;
        current.trigger_kind = "event".into();
        current.event_batch_id = Some(batch.batch_id.clone());
        current.event_count = batch.events.len();
        current.updated_at_ms = now_ms;
        let run = current.clone();
        self.persist()?;
        Ok(Some((run, batch)))
    }

    pub fn advance_step(
        &mut self,
        run_id: &str,
        account_fence: &str,
        generation: u64,
        now_ms: u64,
    ) -> Result<AutomationRun, String> {
        let run = self.current_mut(run_id, account_fence, generation)?;
        if run.state != AutomationRunState::Running {
            return Err("automation run is not running".into());
        }
        run.step_index = run.step_index.saturating_add(1);
        run.step_mutation_id =
            Some(format!("{}:step:{}", run.request_id, run.step_index));
        run.updated_at_ms = now_ms;
        let copy = run.clone();
        self.persist()?;
        Ok(copy)
    }

    pub fn await_approval(
        &mut self,
        run_id: &str,
        account_fence: &str,
        generation: u64,
        approval_id: &str,
        now_ms: u64,
    ) -> Result<AutomationRun, String> {
        if approval_id.trim().is_empty() {
            return Err("approval id is required".into());
        }
        let run = self.current_mut(run_id, account_fence, generation)?;
        if run.state != AutomationRunState::Running {
            return Err("automation run is not running".into());
        }
        run.state = AutomationRunState::AwaitingApproval;
        run.approval_id = Some(approval_id.into());
        run.updated_at_ms = now_ms;
        let copy = run.clone();
        self.persist()?;
        Ok(copy)
    }

    pub fn resolve_approval(
        &mut self,
        run_id: &str,
        account_fence: &str,
        generation: u64,
        approval_id: &str,
        allowed: bool,
        now_ms: u64,
    ) -> Result<AutomationRun, String> {
        let run = self.current_mut(run_id, account_fence, generation)?;
        if run.state != AutomationRunState::AwaitingApproval
            || run.approval_id.as_deref() != Some(approval_id)
        {
            return Err("stale or mismatched automation approval".into());
        }
        run.approval_id = None;
        run.state = if allowed {
            AutomationRunState::Running
        } else {
            AutomationRunState::Cancelled
        };
        run.reason = (!allowed).then(|| "approval denied".into());
        run.updated_at_ms = now_ms;
        let copy = run.clone();
        self.persist()?;
        Ok(copy)
    }

    pub fn cancel(
        &mut self,
        run_id: &str,
        account_fence: &str,
        generation: u64,
        reason: &str,
        now_ms: u64,
    ) -> Result<AutomationRun, String> {
        let run = self.current_mut(run_id, account_fence, generation)?;
        if matches!(
            run.state,
            AutomationRunState::Completed
                | AutomationRunState::Failed
                | AutomationRunState::Cancelled
                | AutomationRunState::OutcomeUnknown
        ) {
            return Ok(run.clone());
        }
        run.state = AutomationRunState::Cancelled;
        run.approval_id = None;
        run.reason = Some(reason.into());
        run.updated_at_ms = now_ms;
        let copy = run.clone();
        self.persist()?;
        Ok(copy)
    }

    pub fn settle(
        &mut self,
        run_id: &str,
        account_fence: &str,
        generation: u64,
        succeeded: bool,
        reason: Option<String>,
        now_ms: u64,
        fallback_time_zone: Option<&str>,
    ) -> Result<AutomationRun, String> {
        let run = self.current_mut(run_id, account_fence, generation)?;
        if run.state != AutomationRunState::Running {
            return Err("automation run cannot settle from current state".into());
        }
        run.state = if succeeded {
            AutomationRunState::Completed
        } else {
            AutomationRunState::Failed
        };
        run.reason = reason;
        run.updated_at_ms = now_ms;
        let automation_id = run.automation_id.clone();
        let copy = run.clone();
        if let Some(spec) = self.state.specs.get_mut(&automation_id) {
            spec.last_run_at_ms = Some(now_ms);
            spec.next_run_at_ms = canonical_next_run_ms(spec, fallback_time_zone)?;
        }
        self.persist()?;
        Ok(copy)
    }

    pub fn snapshot(&self, run_id: &str) -> Option<Value> {
        self.state
            .runs
            .get(run_id)
            .and_then(|run| serde_json::to_value(run).ok())
    }

    fn current_mut(
        &mut self,
        run_id: &str,
        account_fence: &str,
        generation: u64,
    ) -> Result<&mut AutomationRun, String> {
        let run = self.state.runs.get_mut(run_id).ok_or("automation run not found")?;
        if run.account_fence != account_fence || run.generation != generation {
            return Err("stale automation callback fenced by account/generation".into());
        }
        Ok(run)
    }

    fn persist(&self) -> Result<(), String> {
        let parent = self
            .path
            .parent()
            .ok_or("automation runtime path has no parent")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let temp = self.path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(&self.state).map_err(|e| e.to_string())?;
        let result = (|| -> Result<(), String> {
            let mut file = File::create(&temp).map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            fs::rename(&temp, &self.path).map_err(|e| e.to_string())?;
            if let Ok(directory) = File::open(parent) {
                directory.sync_all().map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}

fn canonical_next_run_ms(spec: &AutomationSpec, fallback_time_zone: Option<&str>) -> Result<Option<u64>, String> {
    if !spec.enabled { return Ok(None); }
    let anchor = spec.last_run_at_ms.unwrap_or(spec.created_at_ms);
    let next = compute_next_run_at(&spec.schedule, anchor as f64, fallback_time_zone)
        .ok_or_else(|| "automation schedule is invalid or has no next run".to_string())?;
    if !next.is_finite() || next < 0.0 || next > u64::MAX as f64 { return Err("automation next run is out of range".into()); }
    Ok(Some(next.floor() as u64))
}

fn validate_spec(spec: &AutomationSpec) -> Result<(), String> {
    if spec.id.trim().is_empty() || spec.id.len() > 160 {
        return Err("automation id is invalid".into());
    }
    if spec.name.trim().is_empty() || spec.name.len() > 80 {
        return Err("automation name is invalid".into());
    }
    if spec.prompt.trim().is_empty() || spec.prompt.len() > 100_000 {
        return Err("automation prompt is invalid".into());
    }
    if spec.schedule.trim().is_empty() || spec.schedule.len() > 512 {
        return Err("automation schedule is invalid".into());
    }
    if spec.agent_id.trim().is_empty() || spec.agent_id.len() > 160 {
        return Err("automation agent owner is required".into());
    }
    if spec.trigger_description.len() > 512 {
        return Err("automation trigger description is invalid".into());
    }
    if spec.account_fence.trim().is_empty() {
        return Err("automation account fence is required".into());
    }
    Ok(())
}

pub fn run_json(run: &AutomationRun) -> Value {
    serde_json::to_value(run).unwrap_or_else(|_| json!({"error":"serialization_failed"}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(account: &str) -> AutomationSpec {
        AutomationSpec {
            id: "daily-brief".into(),
            name: "Daily brief".into(),
            prompt: "Summarize updates".into(),
            schedule: "0 8 * * *".into(),
            agent_id: "agent-a".into(),
            trigger_description: "Every day at 8:00 AM".into(),
            enabled: true,
            account_fence: account.into(),
            created_at_ms: 1,
            last_run_at_ms: None,
            next_run_at_ms: Some(100),
        }
    }

    #[test]
    fn automation_owner_survives_restart_and_account_projection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("automation.json");
        let mut runtime = AutomationRuntime::open(&path, 1).unwrap();
        runtime.upsert_spec(spec("acct-a"), Some("UTC")).unwrap();
        drop(runtime);

        let recovered = AutomationRuntime::open(&path, 2).unwrap();
        let listed = recovered.list_for_account("acct-a");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].get("agent_id").and_then(Value::as_str), Some("agent-a"));
        assert_eq!(
            listed[0].get("trigger_description").and_then(Value::as_str),
            Some("Every day at 8:00 AM")
        );
        assert!(recovered.list_for_account("acct-b").is_empty());
    }

    #[test]
    fn ownerless_legacy_record_loads_but_cannot_be_rewritten_as_valid_spec() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("automation.json");
        fs::write(
            &path,
            r#"{
              "next_generation":0,
              "specs":{"legacy":{"id":"legacy","name":"Legacy","prompt":"p","schedule":"0 8 * * *","enabled":true,"account_fence":"acct-a","created_at_ms":1,"last_run_at_ms":null,"next_run_at_ms":null}},
              "runs":{},
              "request_to_run":{}
            }"#,
        )
        .unwrap();
        let mut runtime = AutomationRuntime::open(&path, 2).unwrap();
        let legacy = runtime.list_for_account("acct-a");
        assert_eq!(legacy.len(), 1);
        assert_eq!(legacy[0].get("agent_id").and_then(Value::as_str), Some(""));
        let mut replacement = spec("acct-a");
        replacement.id = "legacy".into();
        replacement.agent_id.clear();
        assert!(runtime.upsert_spec(replacement, Some("UTC")).is_err());
    }

    #[test]
    fn owned_spec_lookup_is_account_and_agent_fenced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("automation.json");
        let mut runtime = AutomationRuntime::open(&path, 1).unwrap();
        runtime.upsert_spec(spec("acct-a"), Some("UTC")).unwrap();
        assert!(runtime.has_owned_spec("daily-brief", "acct-a", "agent-a"));
        assert!(!runtime.has_owned_spec("daily-brief", "acct-b", "agent-a"));
        assert!(!runtime.has_owned_spec("daily-brief", "acct-a", "agent-b"));
        assert_eq!(
            runtime.spec_for_account("daily-brief", "acct-a").unwrap().agent_id,
            "agent-a"
        );
        assert!(runtime.spec_for_account("daily-brief", "acct-b").is_none());
        assert_eq!(runtime.run_count_for_automation("daily-brief", "acct-a"), 0);
        runtime
            .begin_run("daily-brief", "request-count", "run-count", "acct-a", 2)
            .unwrap();
        assert_eq!(runtime.run_count_for_automation("daily-brief", "acct-a"), 1);
        assert_eq!(runtime.run_count_for_automation("daily-brief", "acct-b"), 0);
    }

    #[test]
    fn per_agent_workflow_enablement_is_account_fenced_and_durable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("automation.json");
        let mut runtime = AutomationRuntime::open(&path, 1).unwrap();
        runtime.upsert_spec(spec("acct-a"), Some("UTC")).unwrap();

        let disabled = runtime
            .set_agent_workflow_enabled("acct-a", "agent-a", "daily-brief", false, Some("UTC"))
            .unwrap();
        assert!(!disabled.enabled);
        assert_eq!(runtime.list_for_agent("acct-a", "agent-a").len(), 1);
        assert!(runtime
            .set_agent_workflow_enabled("acct-b", "agent-a", "daily-brief", true, Some("UTC"))
            .is_err());
        assert!(runtime
            .set_agent_workflow_enabled("acct-a", "agent-b", "daily-brief", true, Some("UTC"))
            .is_err());
        assert!(runtime
            .begin_run("daily-brief", "request-disabled", "run-disabled", "acct-a", 2)
            .is_err());
        drop(runtime);

        let mut reopened = AutomationRuntime::open(&path, 3).unwrap();
        let listed = reopened.list_for_agent("acct-a", "agent-a");
        assert_eq!(listed[0].get("enabled").and_then(Value::as_bool), Some(false));
        reopened
            .set_agent_workflow_enabled("acct-a", "agent-a", "daily-brief", true, Some("UTC"))
            .unwrap();
        assert!(reopened
            .begin_run("daily-brief", "request-enabled", "run-enabled", "acct-a", 4)
            .is_ok());
        assert!(
            fs::read_dir(dir.path())
                .unwrap()
                .all(|entry| !entry.unwrap().file_name().to_string_lossy().ends_with(".tmp"))
        );
    }

    #[test]
    fn duplicate_request_converges_only_after_known_terminal_state() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = AutomationRuntime::open(dir.path().join("automation.json"), 1).unwrap();
        runtime.upsert_spec(spec("acct-a"), Some("UTC")).unwrap();
        let run = runtime
            .begin_run("daily-brief", "request-1", "run-1", "acct-a", 2)
            .unwrap();
        assert!(runtime
            .begin_run("daily-brief", "request-1", "run-2", "acct-a", 3)
            .is_err());
        runtime
            .settle("run-1", "acct-a", run.generation, true, None, 4, Some("UTC"))
            .unwrap();
        let replay = runtime
            .begin_run("daily-brief", "request-1", "run-2", "acct-a", 5)
            .unwrap();
        assert_eq!(replay.run_id, "run-1");
    }

    #[test]
    fn restart_turns_running_side_effect_into_outcome_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("automation.json");
        let mut runtime = AutomationRuntime::open(&path, 1).unwrap();
        runtime.upsert_spec(spec("acct-a"), Some("UTC")).unwrap();
        runtime
            .begin_run("daily-brief", "request-1", "run-1", "acct-a", 2)
            .unwrap();
        drop(runtime);
        let mut recovered = AutomationRuntime::open(&path, 3).unwrap();
        assert!(recovered
            .begin_run("daily-brief", "request-1", "run-2", "acct-a", 4)
            .is_err());
        assert_eq!(
            recovered
                .snapshot("run-1")
                .unwrap()
                .get("state")
                .and_then(Value::as_str),
            Some("outcome_unknown")
        );
    }

    #[test]
    fn approval_and_account_generation_are_fenced() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = AutomationRuntime::open(dir.path().join("automation.json"), 1).unwrap();
        runtime.upsert_spec(spec("acct-a"), Some("UTC")).unwrap();
        let run = runtime
            .begin_run("daily-brief", "request-1", "run-1", "acct-a", 2)
            .unwrap();
        runtime
            .await_approval("run-1", "acct-a", run.generation, "approval-1", 3)
            .unwrap();
        assert!(runtime
            .resolve_approval(
                "run-1",
                "acct-b",
                run.generation,
                "approval-1",
                true,
                4
            )
            .is_err());
        assert!(runtime
            .resolve_approval(
                "run-1",
                "acct-a",
                run.generation + 1,
                "approval-1",
                true,
                4
            )
            .is_err());
        assert!(runtime
            .resolve_approval(
                "run-1",
                "acct-a",
                run.generation,
                "other",
                true,
                4
            )
            .is_err());
        let resumed = runtime
            .resolve_approval(
                "run-1",
                "acct-a",
                run.generation,
                "approval-1",
                true,
                5
            )
            .unwrap();
        assert_eq!(resumed.state, AutomationRunState::Running);
    }

    #[test]
    fn step_mutation_identity_is_stable_and_monotonic() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = AutomationRuntime::open(dir.path().join("automation.json"), 1).unwrap();
        runtime.upsert_spec(spec("acct-a"), Some("UTC")).unwrap();
        let run = runtime
            .begin_run("daily-brief", "request-1", "run-1", "acct-a", 2)
            .unwrap();
        assert_eq!(run.step_mutation_id.as_deref(), Some("request-1:step:0"));
        let next = runtime
            .advance_step("run-1", "acct-a", run.generation, 3)
            .unwrap();
        assert_eq!(next.step_mutation_id.as_deref(), Some("request-1:step:1"));
    }
    #[test]
    fn schedule_is_normalized_and_next_run_is_host_owned() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = AutomationRuntime::open(dir.path().join("automation.json"), 1).unwrap();
        let mut value = spec("acct-a");
        value.schedule = "  @every   5m  ".into();
        value.created_at_ms = 1_000;
        value.next_run_at_ms = Some(9_999_999);
        runtime.upsert_spec(value, Some("UTC")).unwrap();
        let listed = runtime.list_for_account("acct-a");
        assert_eq!(listed[0]["schedule"], "@every 5m");
        assert_eq!(listed[0]["next_run_at_ms"], 301_000);
    }

    #[test]
    fn spend_guard_matches_desktop_thresholds_and_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("automation.json");
        let mut runtime = AutomationRuntime::open(&path, 1).unwrap();
        runtime.upsert_spec(spec("acct-a"), Some("UTC")).unwrap();

        let idle = SPEND_GUARD_IDLE_TTL_MS + 100;
        let (below, _) = runtime
            .evaluate_spend_guard("acct-a", "agent-a", 100, 14, idle)
            .unwrap();
        assert_eq!(below, AutomationSpendGuardDecision::BelowThresholds);

        let (nudge, _) = runtime
            .evaluate_spend_guard("acct-a", "agent-a", 100, 15, idle)
            .unwrap();
        assert_eq!(nudge, AutomationSpendGuardDecision::Nudge);
        runtime
            .record_spend_guard_nudge("acct-a", "agent-a", "guard-card", idle)
            .unwrap();

        let (awaiting, _) = runtime
            .evaluate_spend_guard("acct-a", "agent-a", 100, 15, idle + 1)
            .unwrap();
        assert_eq!(awaiting, AutomationSpendGuardDecision::AwaitingAck);
        drop(runtime);

        let mut reopened = AutomationRuntime::open(&path, idle + SPEND_GUARD_PAUSE_DELAY_MS).unwrap();
        let (pause, _) = reopened
            .evaluate_spend_guard(
                "acct-a",
                "agent-a",
                100,
                15,
                idle + SPEND_GUARD_PAUSE_DELAY_MS,
            )
            .unwrap();
        assert_eq!(pause, AutomationSpendGuardDecision::Pause);
        let paused = reopened
            .pause_agent_automations_for_spend_guard("acct-a", "agent-a", Some("UTC"))
            .unwrap();
        assert_eq!(paused, vec!["daily-brief".to_string()]);
        assert_eq!(
            reopened.list_for_agent("acct-a", "agent-a")[0]["enabled"],
            false
        );
    }

    #[test]
    fn spend_guard_answer_is_card_and_account_fenced() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = AutomationRuntime::open(dir.path().join("automation.json"), 1).unwrap();
        runtime.upsert_spec(spec("acct-a"), Some("UTC")).unwrap();
        runtime
            .record_spend_guard_nudge("acct-a", "agent-a", "guard-card", 10)
            .unwrap();
        runtime
            .pause_agent_automations_for_spend_guard("acct-a", "agent-a", Some("UTC"))
            .unwrap();

        assert!(!runtime
            .apply_spend_guard_answer(
                "acct-a",
                "agent-a",
                "forged-card",
                "spend-guard:resume",
                20,
                Some("UTC"),
            )
            .unwrap());
        assert!(!runtime
            .apply_spend_guard_answer(
                "acct-b",
                "agent-a",
                "guard-card",
                "spend-guard:resume",
                20,
                Some("UTC"),
            )
            .unwrap());
        assert!(runtime
            .apply_spend_guard_answer(
                "acct-a",
                "agent-a",
                "guard-card",
                "spend-guard:resume",
                20,
                Some("UTC"),
            )
            .unwrap());
        assert_eq!(runtime.list_for_agent("acct-a", "agent-a")[0]["enabled"], true);
        assert!(runtime.spend_guard_state("acct-a", "agent-a").snoozed_until_ms.is_some());
    }

    #[test]
    fn invalid_schedule_is_rejected_and_disable_clears_next_run() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = AutomationRuntime::open(dir.path().join("automation.json"), 1).unwrap();
        let mut invalid = spec("acct-a"); invalid.schedule = "not a cron".into();
        assert!(runtime.upsert_spec(invalid, Some("UTC")).is_err());
        runtime.upsert_spec(spec("acct-a"), Some("UTC")).unwrap();
        let disabled = runtime.set_agent_workflow_enabled("acct-a", "agent-a", "daily-brief", false, Some("UTC")).unwrap();
        assert_eq!(disabled.next_run_at_ms, None);
        let enabled = runtime.set_agent_workflow_enabled("acct-a", "agent-a", "daily-brief", true, Some("UTC")).unwrap();
        assert!(enabled.next_run_at_ms.is_some());
    }

    #[test]
    fn event_occurrences_are_debounced_bounded_batched_and_deduplicated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("automation.json");
        let mut runtime = AutomationRuntime::open(&path, 1).unwrap();
        runtime.upsert_spec(spec("acct-a"), Some("UTC")).unwrap();

        let first = runtime
            .enqueue_event_fire(
                "daily-brief",
                "acct-a",
                "occurrence-1",
                json!({"kind":"message","value":1}),
                1_000,
            )
            .unwrap();
        assert!(first.accepted);
        assert!(!first.duplicate);
        assert!(runtime
            .claim_event_batch("daily-brief", "acct-a", "claim-early", 1_749)
            .unwrap()
            .is_none());
        let duplicate = runtime
            .enqueue_event_fire(
                "daily-brief",
                "acct-a",
                "occurrence-1",
                json!({"kind":"message","value":999}),
                1_100,
            )
            .unwrap();
        assert!(duplicate.duplicate);

        for index in 2..=30 {
            runtime
                .enqueue_event_fire(
                    "daily-brief",
                    "acct-a",
                    &format!("occurrence-{index}"),
                    json!({"kind":"message","value":index}),
                    1_000,
                )
                .unwrap();
        }
        let batch = runtime
            .claim_event_batch("daily-brief", "acct-a", "claim-1", 1_750)
            .unwrap()
            .unwrap();
        assert_eq!(batch.events.len(), MAX_EVENTS_IN_AUTOMATION_WAKE);
        assert_eq!(batch.occurrence_ids[0], "occurrence-1");
        assert_eq!(batch.occurrence_ids[24], "occurrence-25");
        assert_eq!(
            runtime
                .claim_event_batch("daily-brief", "acct-a", "claim-1", 1_751)
                .unwrap()
                .unwrap(),
            batch,
            "stable claim replay must return the same batch"
        );
        runtime
            .settle_event_batch(&batch.batch_id, "acct-a", true, 1_752)
            .unwrap();
        let settled_duplicate = runtime
            .enqueue_event_fire(
                "daily-brief",
                "acct-a",
                "occurrence-1",
                json!({"kind":"message"}),
                2_000,
            )
            .unwrap();
        assert!(settled_duplicate.duplicate);
        let next = runtime
            .claim_event_batch("daily-brief", "acct-a", "claim-2", 2_000)
            .unwrap()
            .unwrap();
        assert_eq!(next.events.len(), 5);
    }

    #[test]
    fn claimed_event_batch_requires_reconciliation_after_process_death() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("automation.json");
        let batch_id;
        {
            let mut runtime = AutomationRuntime::open(&path, 1).unwrap();
            runtime.upsert_spec(spec("acct-a"), Some("UTC")).unwrap();
            runtime
                .enqueue_event_fire(
                    "daily-brief",
                    "acct-a",
                    "occurrence-crash",
                    json!({"kind":"connector","id":"event-1"}),
                    10,
                )
                .unwrap();
            let batch = runtime
                .claim_event_batch("daily-brief", "acct-a", "claim-crash", 1_000)
                .unwrap()
                .unwrap();
            batch_id = batch.batch_id;
        }

        let mut recovered = AutomationRuntime::open(&path, 2_000).unwrap();
        assert!(recovered
            .claim_event_batch("daily-brief", "acct-a", "claim-after-restart", 2_000)
            .unwrap_err()
            .contains("outcome-unknown"));
        assert!(recovered
            .settle_event_batch(&batch_id, "acct-a", true, 2_001)
            .unwrap_err()
            .contains("reconcile"));
        recovered
            .reconcile_event_batch(&batch_id, "acct-a", false, 2_002)
            .unwrap();
        let replay = recovered
            .claim_event_batch("daily-brief", "acct-a", "claim-reconciled", 2_002)
            .unwrap()
            .unwrap();
        assert_eq!(replay.occurrence_ids, vec!["occurrence-crash"]);
        recovered
            .settle_event_batch(&replay.batch_id, "acct-a", true, 2_003)
            .unwrap();
        assert!(recovered
            .enqueue_event_fire(
                "daily-brief",
                "acct-a",
                "occurrence-crash",
                json!({"kind":"connector","id":"event-1"}),
                2_004,
            )
            .unwrap()
            .duplicate);
    }

    #[test]
    fn event_queue_drops_oldest_overflow_and_is_account_fenced() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = AutomationRuntime::open(dir.path().join("automation.json"), 1).unwrap();
        runtime.upsert_spec(spec("acct-a"), Some("UTC")).unwrap();
        assert!(runtime
            .enqueue_event_fire(
                "daily-brief",
                "acct-b",
                "wrong-account",
                json!({"value":0}),
                1,
            )
            .unwrap_err()
            .contains("account fence"));
        let mut dropped = Vec::new();
        for index in 0..=MAX_QUEUED_EVENT_FIRES_PER_AUTOMATION {
            let outcome = runtime
                .enqueue_event_fire(
                    "daily-brief",
                    "acct-a",
                    &format!("overflow-{index}"),
                    json!({"value":index}),
                    1,
                )
                .unwrap();
            dropped.extend(outcome.dropped_occurrence_ids);
        }
        assert_eq!(dropped, vec!["overflow-0".to_string()]);
        assert!(runtime
            .enqueue_event_fire(
                "daily-brief",
                "acct-a",
                "overflow-0",
                json!({"value":"duplicate-after-drop"}),
                2,
            )
            .unwrap()
            .duplicate);
        let batch = runtime
            .claim_event_batch("daily-brief", "acct-a", "overflow-claim", 751)
            .unwrap()
            .unwrap();
        assert_eq!(batch.occurrence_ids[0], "overflow-1");
        assert_eq!(batch.occurrence_ids.len(), MAX_EVENTS_IN_AUTOMATION_WAKE);
    }

    #[test]
    fn event_batch_binds_to_one_durable_event_run() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("automation.json");
        let mut runtime = AutomationRuntime::open(&path, 1).unwrap();
        runtime.upsert_spec(spec("acct-a"), Some("UTC")).unwrap();
        runtime
            .enqueue_event_fire(
                "daily-brief",
                "acct-a",
                "event-run-occurrence",
                json!({"kind":"github","delivery":"42"}),
                1,
            )
            .unwrap();
        let (run, batch) = runtime
            .begin_event_run(
                "daily-brief",
                "acct-a",
                "event-claim-1",
                "event-run-1",
                1_000,
            )
            .unwrap()
            .unwrap();
        assert_eq!(run.trigger_kind, "event");
        assert_eq!(run.event_batch_id.as_deref(), Some(batch.batch_id.as_str()));
        assert_eq!(run.event_count, 1);
        assert_eq!(run.request_id, "automation-event:event-claim-1");

        drop(runtime);
        let recovered = AutomationRuntime::open(&path, 2_000).unwrap();
        let recovered_run = recovered.snapshot("event-run-1").unwrap();
        assert_eq!(recovered_run["state"], "outcome_unknown");
        assert_eq!(recovered_run["trigger_kind"], "event");
        assert_eq!(recovered_run["event_count"], 1);
        assert!(recovered_run["event_batch_id"].is_string());
    }

}
