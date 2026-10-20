use std::collections::{HashMap, HashSet};

use crate::sand_activity::{
    ActivityTransition, ActivityUpdate, AgentActivity, NAMED_ACTIVITY_MAX_HOLD_MS,
    NamedActivityHoldState, SEND_MESSAGE_TOOL_CALL_OUTLINE_NAME, resolve_named_activity_hold,
};

use super::run_scheduler::RunLane;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunWindowCompleted {
    pub agent_id: String,
    pub duration_ms: u64,
    pub turn_worthy_begin_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnUsageReport {
    pub agent_id: String,
    pub source: String,
    pub request_id: Option<String>,
    pub request_id_count: usize,
    pub turn_ended_seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExclusiveRunRequest {
    pub agent_id: String,
    pub task_id: String,
    pub lane: RunLane,
    pub source: String,
    pub accepted_at_ms: Option<u64>,
    pub ack_token: Option<String>,
}

#[derive(Debug, Default, Clone)]
struct SessionRun {
    in_flight: u64,
    turn_worthy_in_flight: u64,
    window_started_at_ms: Option<u64>,
    turn_worthy_begins: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunStateProjection {
    pub is_running: bool,
    pub is_running_turn: bool,
    pub is_composing_message: bool,
    pub is_retrying: bool,
    pub current_activity: Option<AgentActivity>,
    pub active_remote_member_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct RunLifecycleState {
    sessions: HashMap<String, SessionRun>,
    active_run_session: Option<String>,
    composing: HashSet<String>,
    retrying: HashSet<String>,
    activity: HashMap<String, String>,
    structured_activity: HashMap<String, AgentActivity>,
    activity_holds: HashMap<String, NamedActivityHoldState>,
    provider_run_counts: HashMap<String, u64>,
    provider_turn_worthy_counts: HashMap<String, u64>,
    last_request_id: HashMap<String, String>,
    turn_request_ids: HashMap<String, HashSet<String>>,
    turn_ended_seq: HashMap<String, u64>,
    active_remote_members: HashMap<String, String>,
    live_subagent_parent_counts: HashMap<String, u64>,
}

impl RunLifecycleState {
    pub fn begin_session_run(
        &mut self,
        agent_id: impl Into<String>,
        now_ms: u64,
        is_group_member_turn: bool,
    ) {
        let agent_id = agent_id.into();
        let state = self.sessions.entry(agent_id.clone()).or_default();
        if state.in_flight == 0 {
            state.window_started_at_ms = Some(now_ms);
        }
        state.in_flight = state.in_flight.saturating_add(1);
        if !is_group_member_turn {
            state.turn_worthy_begins = state.turn_worthy_begins.saturating_add(1);
            state.turn_worthy_in_flight = state.turn_worthy_in_flight.saturating_add(1);
        }
        self.active_run_session = Some(agent_id);
    }

    pub fn end_session_run(
        &mut self,
        agent_id: &str,
        now_ms: u64,
    ) -> Option<RunWindowCompleted> {
        self.end_session_run_with_kind(agent_id, now_ms, false)
    }

    pub fn end_session_run_with_kind(
        &mut self,
        agent_id: &str,
        now_ms: u64,
        is_group_member_turn: bool,
    ) -> Option<RunWindowCompleted> {
        if self.provider_run_count(agent_id) == 0 {
            self.clear_visible_run_state(agent_id);
        }

        let state = self.sessions.get_mut(agent_id)?;
        if !is_group_member_turn {
            state.turn_worthy_in_flight = state.turn_worthy_in_flight.saturating_sub(1);
        }
        if state.in_flight > 1 {
            state.in_flight -= 1;
            return None;
        }
        let state = self.sessions.remove(agent_id)?;
        self.turn_request_ids.remove(agent_id);
        self.turn_ended_seq.remove(agent_id);
        if self.active_run_session.as_deref() == Some(agent_id) {
            self.active_run_session = None;
        }
        if state.turn_worthy_begins == 0 {
            return None;
        }
        Some(RunWindowCompleted {
            agent_id: agent_id.to_string(),
            duration_ms: now_ms.saturating_sub(state.window_started_at_ms.unwrap_or(now_ms)),
            turn_worthy_begin_count: state.turn_worthy_begins,
        })
    }

    pub fn running_agent_ids(&self) -> HashSet<String> {
        let mut running = self.sessions
            .iter()
            .filter(|(_, state)| state.in_flight > 0)
            .map(|(agent_id, _)| agent_id.clone())
            .collect::<HashSet<_>>();
        running.extend(
            self.provider_run_counts
                .iter()
                .filter(|(_, count)| **count > 0)
                .map(|(agent_id, _)| agent_id.clone()),
        );
        running.extend(
            self.live_subagent_parent_counts
                .iter()
                .filter(|(_, count)| **count > 0)
                .map(|(agent_id, _)| agent_id.clone()),
        );
        running
    }

    pub fn begin_live_subagent(&mut self, parent_agent_id: &str) {
        let parent_agent_id = parent_agent_id.trim();
        if parent_agent_id.is_empty() {
            return;
        }
        let count = self
            .live_subagent_parent_counts
            .entry(parent_agent_id.to_string())
            .or_default();
        *count = count.saturating_add(1);
    }

    pub fn end_live_subagent(&mut self, parent_agent_id: &str) {
        let mut remove = false;
        if let Some(count) = self.live_subagent_parent_counts.get_mut(parent_agent_id) {
            *count = count.saturating_sub(1);
            remove = *count == 0;
        }
        if remove {
            self.live_subagent_parent_counts.remove(parent_agent_id);
        }
    }

    pub fn has_live_subagent(&self, parent_agent_id: &str) -> bool {
        self.live_subagent_parent_counts
            .get(parent_agent_id)
            .copied()
            .unwrap_or_default()
            > 0
    }

    pub fn begin_provider_run(&mut self, agent_id: &str) {
        self.begin_provider_run_with_kind(agent_id, false);
    }

    pub fn begin_provider_run_with_kind(
        &mut self,
        agent_id: &str,
        is_group_member_turn: bool,
    ) {
        if agent_id.trim().is_empty() {
            return;
        }
        let count = self.provider_run_counts.entry(agent_id.to_string()).or_default();
        *count = count.saturating_add(1);
        if !is_group_member_turn {
            let turn_count = self
                .provider_turn_worthy_counts
                .entry(agent_id.to_string())
                .or_default();
            *turn_count = turn_count.saturating_add(1);
        }
        self.active_run_session = Some(agent_id.to_string());
    }

    pub fn end_provider_run(&mut self, agent_id: &str) {
        self.end_provider_run_with_kind(agent_id, false);
    }

    pub fn end_provider_run_with_kind(
        &mut self,
        agent_id: &str,
        is_group_member_turn: bool,
    ) {
        let mut remove = false;
        if let Some(count) = self.provider_run_counts.get_mut(agent_id) {
            *count = count.saturating_sub(1);
            remove = *count == 0;
        }
        if remove {
            self.provider_run_counts.remove(agent_id);
        }
        if !is_group_member_turn {
            let mut remove_turn = false;
            if let Some(count) = self.provider_turn_worthy_counts.get_mut(agent_id) {
                *count = count.saturating_sub(1);
                remove_turn = *count == 0;
            }
            if remove_turn {
                self.provider_turn_worthy_counts.remove(agent_id);
            }
        }
        if self.provider_run_count(agent_id) == 0 && self.in_flight_count(agent_id) == 0 {
            self.clear_visible_run_state(agent_id);
            if self.active_run_session.as_deref() == Some(agent_id) {
                self.active_run_session = None;
            }
        }
    }

    pub fn provider_run_count(&self, agent_id: &str) -> u64 {
        self.provider_run_counts.get(agent_id).copied().unwrap_or_default()
    }

    pub fn provider_turn_worthy_count(&self, agent_id: &str) -> u64 {
        self.provider_turn_worthy_counts
            .get(agent_id)
            .copied()
            .unwrap_or_default()
    }

    pub fn is_running(&self, agent_id: &str) -> bool {
        self.in_flight_count(agent_id) > 0 || self.provider_run_count(agent_id) > 0
    }

    pub fn is_turn_worthy_running(&self, agent_id: &str) -> bool {
        self.sessions
            .get(agent_id)
            .is_some_and(|state| state.turn_worthy_in_flight > 0)
            || self.provider_turn_worthy_count(agent_id) > 0
    }

    pub fn active_run_session(&self) -> Option<&str> {
        self.active_run_session.as_deref()
    }

    pub fn set_active_remote_member(&mut self, agent_id: &str, remote_member_id: Option<&str>) {
        let remote_member_id = remote_member_id.map(str::trim).filter(|value| !value.is_empty());
        match remote_member_id {
            Some(remote_member_id) => {
                self.active_remote_members
                    .insert(agent_id.to_string(), remote_member_id.to_string());
            }
            None => {
                self.active_remote_members.remove(agent_id);
            }
        }
    }

    pub fn active_remote_member_id(&self, agent_id: &str) -> Option<&str> {
        self.active_remote_members.get(agent_id).map(String::as_str)
    }

    pub fn project_run_state(
        &self,
        agent_id: &str,
        has_running_subagent: bool,
        active_remote_member_id: Option<&str>,
    ) -> RunStateProjection {
        let is_running = self.is_running(agent_id)
            || self.has_live_subagent(agent_id)
            || has_running_subagent;
        let is_running_turn = self.is_turn_worthy_running(agent_id);
        RunStateProjection {
            is_running,
            is_running_turn,
            is_composing_message: is_running && self.is_composing(agent_id),
            is_retrying: is_running && self.is_retrying(agent_id),
            current_activity: if is_running {
                self.structured_activity(agent_id).cloned()
            } else {
                None
            },
            active_remote_member_id: if is_running {
                active_remote_member_id
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
            } else {
                None
            },
        }
    }

    pub fn in_flight_count(&self, agent_id: &str) -> u64 {
        self.sessions.get(agent_id).map(|state| state.in_flight).unwrap_or(0)
    }

    pub fn record_request_id(&mut self, agent_id: &str, request_id: &str) -> bool {
        let request_id = request_id.trim();
        if agent_id.trim().is_empty() || request_id.is_empty() {
            return false;
        }
        self.last_request_id
            .insert(agent_id.to_string(), request_id.to_string());
        true
    }

    pub fn last_request_id(&self, agent_id: &str) -> Option<&str> {
        self.last_request_id.get(agent_id).map(String::as_str)
    }

    pub fn track_turn_request_id(&mut self, agent_id: &str, request_id: &str) {
        let request_id = request_id.trim();
        if request_id.is_empty() {
            return;
        }
        self.turn_request_ids
            .entry(agent_id.to_string())
            .or_default()
            .insert(request_id.to_string());
    }

    pub fn settle_turn_usage(&mut self, agent_id: &str, source: &str) -> TurnUsageReport {
        let request_ids = self.turn_request_ids.remove(agent_id).unwrap_or_default();
        let turn_ended_seq = self
            .turn_ended_seq
            .entry(agent_id.to_string())
            .and_modify(|value| *value = value.saturating_add(1))
            .or_insert(1);
        let mut ordered = request_ids.into_iter().collect::<Vec<_>>();
        ordered.sort();
        TurnUsageReport {
            agent_id: agent_id.to_string(),
            source: if source.trim().is_empty() {
                "turn".to_string()
            } else {
                source.to_string()
            },
            request_id: ordered.first().cloned(),
            request_id_count: ordered.len(),
            turn_ended_seq: *turn_ended_seq,
        }
    }

    pub fn set_composing(&mut self, agent_id: &str, composing: bool) {
        if composing {
            self.composing.insert(agent_id.to_string());
        } else {
            self.composing.remove(agent_id);
        }
    }

    pub fn is_composing(&self, agent_id: &str) -> bool {
        self.composing.contains(agent_id)
    }

    pub fn set_retrying(&mut self, agent_id: &str, retrying: bool) {
        if retrying {
            self.retrying.insert(agent_id.to_string());
        } else {
            self.retrying.remove(agent_id);
        }
    }

    pub fn is_retrying(&self, agent_id: &str) -> bool {
        self.retrying.contains(agent_id)
    }

    pub fn set_activity(&mut self, agent_id: &str, activity: Option<impl Into<String>>) {
        match activity {
            Some(activity) => {
                let activity = activity.into();
                if activity.trim().is_empty() {
                    self.activity.remove(agent_id);
                } else {
                    self.activity.insert(agent_id.to_string(), activity);
                }
            }
            None => {
                self.activity.remove(agent_id);
            }
        }
    }

    pub fn activity(&self, agent_id: &str) -> Option<&str> {
        self.activity.get(agent_id).map(String::as_str)
    }

    pub fn structured_activity(&self, agent_id: &str) -> Option<&AgentActivity> {
        self.structured_activity.get(agent_id)
    }

    pub fn track_activity_from_update(
        &mut self,
        agent_id: &str,
        update: &ActivityUpdate,
        now_ms: u64,
    ) {
        let prior = self.activity_holds.get(agent_id).cloned().unwrap_or_default();
        let (transition, state) = resolve_named_activity_hold(
            update,
            &prior,
            now_ms,
            NAMED_ACTIVITY_MAX_HOLD_MS,
        );
        if state == NamedActivityHoldState::default() {
            self.activity_holds.remove(agent_id);
        } else {
            self.activity_holds.insert(agent_id.to_string(), state);
        }
        match transition {
            ActivityTransition::Keep => {}
            ActivityTransition::Clear => {
                self.structured_activity.remove(agent_id);
            }
            ActivityTransition::Set(activity) => {
                self.structured_activity.insert(agent_id.to_string(), activity);
            }
        }
    }

    pub fn track_composing_from_update(&mut self, agent_id: &str, update: &ActivityUpdate) {
        match update {
            ActivityUpdate::ToolCall { name, status, .. }
                if name == SEND_MESSAGE_TOOL_CALL_OUTLINE_NAME =>
            {
                self.set_composing(agent_id, status == "pending");
            }
            ActivityUpdate::SendMessage | ActivityUpdate::TurnEnded => {
                self.set_composing(agent_id, false);
            }
            _ => {}
        }
    }

    pub fn track_retrying_from_update(&mut self, agent_id: &str, update: &ActivityUpdate) {
        match update {
            ActivityUpdate::Retrying => self.set_retrying(agent_id, true),
            ActivityUpdate::TextDelta { .. }
            | ActivityUpdate::ThinkingDelta
            | ActivityUpdate::ToolCall { .. }
            | ActivityUpdate::SendMessage
            | ActivityUpdate::TurnEnded => self.set_retrying(agent_id, false),
            ActivityUpdate::Other { .. } => {}
        }
    }

    fn clear_visible_run_state(&mut self, agent_id: &str) {
        self.set_composing(agent_id, false);
        self.set_retrying(agent_id, false);
        self.activity.remove(agent_id);
        self.structured_activity.remove(agent_id);
        self.activity_holds.remove(agent_id);
        self.active_remote_members.remove(agent_id);
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::sand_activity::ActivityUpdate;

    #[test]
    fn nested_session_runs_keep_window_open_until_final_settle() {
        let mut state = RunLifecycleState::default();
        state.begin_session_run("agent-a", 100, false);
        state.begin_session_run("agent-a", 120, false);
        assert_eq!(state.in_flight_count("agent-a"), 2);
        assert!(state.is_turn_worthy_running("agent-a"));
        assert!(state.end_session_run("agent-a", 150).is_none());
        let completed = state.end_session_run("agent-a", 200).expect("final run closes window");
        assert_eq!(completed.agent_id, "agent-a");
        assert_eq!(completed.duration_ms, 100);
        assert_eq!(completed.turn_worthy_begin_count, 2);
        assert!(!state.is_running("agent-a"));
    }

    #[test]
    fn provider_and_live_subagent_contribute_to_running_projection() {
        let mut state = RunLifecycleState::default();
        state.begin_provider_run("agent-a");
        state.begin_live_subagent("agent-a");
        state.set_composing("agent-a", true);
        let projection = state.project_run_state("agent-a", false, Some("remote-1"));
        assert!(projection.is_running);
        assert!(projection.is_running_turn);
        assert!(projection.is_composing_message);
        assert_eq!(projection.active_remote_member_id.as_deref(), Some("remote-1"));

        state.end_provider_run("agent-a");
        assert!(state.has_live_subagent("agent-a"));
        state.end_live_subagent("agent-a");
        assert!(!state.has_live_subagent("agent-a"));
    }

    #[test]
    fn composing_retrying_and_activity_clear_on_terminal_updates() {
        let mut state = RunLifecycleState::default();
        state.begin_provider_run("agent-a");
        state.track_composing_from_update(
            "agent-a",
            &ActivityUpdate::ToolCall {
                id: "call-1".into(),
                name: SEND_MESSAGE_TOOL_CALL_OUTLINE_NAME.into(),
                status: "pending".into(),
                args: None,
                summary: None,
            },
        );
        state.track_retrying_from_update("agent-a", &ActivityUpdate::Retrying);
        assert!(state.is_composing("agent-a"));
        assert!(state.is_retrying("agent-a"));

        state.track_composing_from_update("agent-a", &ActivityUpdate::TurnEnded);
        state.track_retrying_from_update("agent-a", &ActivityUpdate::TurnEnded);
        assert!(!state.is_composing("agent-a"));
        assert!(!state.is_retrying("agent-a"));
    }

    #[test]
    fn turn_usage_deduplicates_request_ids_and_sequences_settlements() {
        let mut state = RunLifecycleState::default();
        state.track_turn_request_id("agent-a", "req-b");
        state.track_turn_request_id("agent-a", "req-a");
        state.track_turn_request_id("agent-a", "req-a");
        let first = state.settle_turn_usage("agent-a", "provider");
        assert_eq!(first.request_id.as_deref(), Some("req-a"));
        assert_eq!(first.request_id_count, 2);
        assert_eq!(first.turn_ended_seq, 1);
        let second = state.settle_turn_usage("agent-a", "");
        assert_eq!(second.request_id_count, 0);
        assert_eq!(second.turn_ended_seq, 2);
        assert_eq!(second.source, "turn");
    }
}
