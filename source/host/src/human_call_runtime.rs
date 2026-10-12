use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HumanCallState {
    Invited,
    Ringing,
    Negotiating,
    Connected,
    Reconnecting,
    Ended,
    Failed,
    OutcomeUnknown,
}

impl HumanCallState {
    fn terminal(&self) -> bool {
        matches!(self, Self::Ended | Self::Failed)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HumanCallSession {
    pub id: String,
    pub request_id: String,
    pub scope_id: String,
    pub creator_id: String,
    pub participant_ids: Vec<String>,
    pub state: HumanCallState,
    pub generation: u64,
    pub signal_seq: u64,
    pub account_fence: String,
    pub media_capabilities: Value,
    pub device_selection: Value,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub terminal_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HumanCallLease {
    pub call_id: String,
    pub user_id: String,
    pub device_id: String,
    pub role: String,
    pub is_owner: bool,
    pub claim_available: bool,
    pub generation: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HumanCallSignal {
    pub call_id: String,
    pub generation: u64,
    pub seq: u64,
    pub sender_device_id: String,
    pub kind: String,
    pub payload: Value,
    pub created_at_ms: u64,
}

#[derive(Default, Deserialize, Serialize)]
struct DurableHumanCallState {
    next_generation: u64,
    sessions: BTreeMap<String, HumanCallSession>,
    request_to_call: BTreeMap<String, String>,
    owner_device_by_call: BTreeMap<String, String>,
    signals: BTreeMap<String, Vec<HumanCallSignal>>,
}

pub struct HumanCallRuntime {
    path: PathBuf,
    state: DurableHumanCallState,
}

impl HumanCallRuntime {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let path = path.into();
        let mut state = if path.exists() {
            serde_json::from_slice::<DurableHumanCallState>(
                &fs::read(&path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| format!("invalid Human call runtime store: {error}"))?
        } else {
            DurableHumanCallState::default()
        };
        let mut changed = false;
        for session in state.sessions.values_mut() {
            if !session.state.terminal() && session.state != HumanCallState::OutcomeUnknown {
                session.state = HumanCallState::OutcomeUnknown;
                session.terminal_reason =
                    Some("Host restarted while live call outcome was unknown; reconcile before continuing".into());
                session.updated_at_ms = now_ms;
                changed = true;
            }
        }
        state.owner_device_by_call.clear();
        let runtime = Self { path, state };
        if changed {
            runtime.persist()?;
        }
        Ok(runtime)
    }

    pub fn create(
        &mut self,
        call_id: &str,
        request_id: &str,
        scope_id: &str,
        creator_id: &str,
        participant_ids: Vec<String>,
        account_fence: &str,
        now_ms: u64,
    ) -> Result<HumanCallSession, String> {
        validate_id("call", call_id)?;
        validate_id("request", request_id)?;
        validate_id("scope", scope_id)?;
        validate_id("creator", creator_id)?;
        if account_fence.trim().is_empty() {
            return Err("call account fence is required".into());
        }
        if participant_ids.is_empty() || participant_ids.len() > 64 {
            return Err("call participants are invalid".into());
        }
        let mut unique = BTreeSet::new();
        for participant in &participant_ids {
            validate_id("participant", participant)?;
            if !unique.insert(participant.clone()) {
                return Err("duplicate call participant".into());
            }
        }
        if let Some(existing_id) = self.state.request_to_call.get(request_id) {
            let existing = self.state.sessions.get(existing_id).ok_or("call request index is corrupt")?;
            if existing.account_fence == account_fence && existing.scope_id == scope_id {
                return Ok(existing.clone());
            }
            return Err("call request id is already bound".into());
        }
        if self.state.sessions.contains_key(call_id) {
            return Err("duplicate call id".into());
        }
        self.state.next_generation = self.state.next_generation.saturating_add(1).max(1);
        let session = HumanCallSession {
            id: call_id.into(),
            request_id: request_id.into(),
            scope_id: scope_id.into(),
            creator_id: creator_id.into(),
            participant_ids,
            state: HumanCallState::Invited,
            generation: self.state.next_generation,
            signal_seq: 0,
            account_fence: account_fence.into(),
            media_capabilities: json!({"audio":false,"video":false,"screenShare":false}),
            device_selection: json!({}),
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            terminal_reason: None,
        };
        self.state.request_to_call.insert(request_id.into(), call_id.into());
        self.state.sessions.insert(call_id.into(), session.clone());
        self.persist()?;
        Ok(session)
    }

    pub fn list(&self, scope_id: &str, account_fence: &str, limit: usize) -> Vec<HumanCallSession> {
        let mut calls = self
            .state
            .sessions
            .values()
            .filter(|session| session.scope_id == scope_id && session.account_fence == account_fence)
            .cloned()
            .collect::<Vec<_>>();
        calls.sort_by_key(|session| std::cmp::Reverse(session.updated_at_ms));
        calls.truncate(limit.min(100));
        calls
    }

    pub fn lease(
        &mut self,
        call_id: &str,
        account_fence: &str,
        generation: u64,
        user_id: &str,
        device_id: &str,
        allow_claim: bool,
    ) -> Result<HumanCallLease, String> {
        validate_id("device", device_id)?;
        let session = self.current(call_id, account_fence, generation)?;
        if session.state.terminal() {
            return Err("terminal call has no transport lease".into());
        }
        let role = if session.creator_id == user_id { "creator" } else { "peer" };
        let owner = self.state.owner_device_by_call.get(call_id).cloned();
        let claim_available = owner.is_none();
        if owner.is_none() && allow_claim {
            self.state.owner_device_by_call.insert(call_id.into(), device_id.into());
            self.persist()?;
        }
        let current_owner = self.state.owner_device_by_call.get(call_id);
        Ok(HumanCallLease {
            call_id: call_id.into(),
            user_id: user_id.into(),
            device_id: device_id.into(),
            role: role.into(),
            is_owner: current_owner.is_some_and(|owner| owner == device_id),
            claim_available,
            generation,
        })
    }

    pub fn transition(
        &mut self,
        call_id: &str,
        account_fence: &str,
        generation: u64,
        action: &str,
        terminal_reason: Option<String>,
        now_ms: u64,
    ) -> Result<HumanCallSession, String> {
        let session = self.current_mut(call_id, account_fence, generation)?;
        let next = match (session.state.clone(), action) {
            (HumanCallState::Invited, "ring") => HumanCallState::Ringing,
            (HumanCallState::Invited | HumanCallState::Ringing, "accept") => HumanCallState::Negotiating,
            (HumanCallState::Negotiating | HumanCallState::Reconnecting, "connect") => HumanCallState::Connected,
            (HumanCallState::Connected, "reconnect") => HumanCallState::Reconnecting,
            (HumanCallState::Invited | HumanCallState::Ringing, "decline") => HumanCallState::Ended,
            (state, "hangup") if !state.terminal() => HumanCallState::Ended,
            (state, "fail") if !state.terminal() => HumanCallState::Failed,
            _ => return Err("invalid Human call state transition".into()),
        };
        session.state = next;
        session.updated_at_ms = now_ms;
        session.terminal_reason = terminal_reason;
        let terminal = session.state.terminal();
        let copy = session.clone();
        if terminal {
            self.state.owner_device_by_call.remove(call_id);
        }
        self.persist()?;
        Ok(copy)
    }

    pub fn update_media(
        &mut self,
        call_id: &str,
        account_fence: &str,
        generation: u64,
        media_capabilities: Value,
        device_selection: Value,
        now_ms: u64,
    ) -> Result<HumanCallSession, String> {
        if !media_capabilities.is_object() || !device_selection.is_object() {
            return Err("call media state must be objects".into());
        }
        let session = self.current_mut(call_id, account_fence, generation)?;
        if session.state.terminal() {
            return Err("terminal call media cannot be updated".into());
        }
        session.media_capabilities = media_capabilities;
        session.device_selection = device_selection;
        session.updated_at_ms = now_ms;
        let copy = session.clone();
        self.persist()?;
        Ok(copy)
    }

    pub fn send_signal(
        &mut self,
        call_id: &str,
        account_fence: &str,
        generation: u64,
        sender_device_id: &str,
        kind: &str,
        payload: Value,
        now_ms: u64,
    ) -> Result<HumanCallSignal, String> {
        if !matches!(kind, "offer" | "answer" | "candidate") || !payload.is_object() {
            return Err("invalid Human call signal".into());
        }
        let owner = self.state.owner_device_by_call.get(call_id).ok_or("call transport lease is unclaimed")?;
        if owner != sender_device_id {
            return Err("call signal sender does not own the transport lease".into());
        }
        let session = self.current_mut(call_id, account_fence, generation)?;
        if session.state.terminal() || session.state == HumanCallState::OutcomeUnknown {
            return Err("call cannot signal from current state".into());
        }
        session.signal_seq = session.signal_seq.saturating_add(1);
        session.updated_at_ms = now_ms;
        let signal = HumanCallSignal {
            call_id: call_id.into(),
            generation,
            seq: session.signal_seq,
            sender_device_id: sender_device_id.into(),
            kind: kind.into(),
            payload,
            created_at_ms: now_ms,
        };
        self.state.signals.entry(call_id.into()).or_default().push(signal.clone());
        if let Some(signals) = self.state.signals.get_mut(call_id) {
            if signals.len() > 512 {
                let drain = signals.len() - 512;
                signals.drain(0..drain);
            }
        }
        self.persist()?;
        Ok(signal)
    }

    pub fn signals_since(
        &self,
        call_id: &str,
        account_fence: &str,
        generation: u64,
        after_seq: u64,
    ) -> Result<Vec<HumanCallSignal>, String> {
        self.current(call_id, account_fence, generation)?;
        Ok(self
            .state
            .signals
            .get(call_id)
            .into_iter()
            .flatten()
            .filter(|signal| signal.generation == generation && signal.seq > after_seq)
            .cloned()
            .collect())
    }

    pub fn reconcile_unknown(
        &mut self,
        call_id: &str,
        account_fence: &str,
        generation: u64,
        terminal_state: &str,
        reason: Option<String>,
        now_ms: u64,
    ) -> Result<HumanCallSession, String> {
        let session = self.current_mut(call_id, account_fence, generation)?;
        if session.state != HumanCallState::OutcomeUnknown {
            return Err("only outcome-unknown calls can reconcile".into());
        }
        session.state = match terminal_state {
            "ended" => HumanCallState::Ended,
            "failed" => HumanCallState::Failed,
            _ => return Err("call reconciliation requires a terminal state".into()),
        };
        session.terminal_reason = reason;
        session.updated_at_ms = now_ms;
        let copy = session.clone();
        self.state.owner_device_by_call.remove(call_id);
        self.persist()?;
        Ok(copy)
    }

    fn current(&self, call_id: &str, account_fence: &str, generation: u64) -> Result<&HumanCallSession, String> {
        let session = self.state.sessions.get(call_id).ok_or("Human call not found")?;
        if session.account_fence != account_fence || session.generation != generation {
            return Err("stale Human call callback fenced by account/generation".into());
        }
        Ok(session)
    }

    fn current_mut(&mut self, call_id: &str, account_fence: &str, generation: u64) -> Result<&mut HumanCallSession, String> {
        let session = self.state.sessions.get_mut(call_id).ok_or("Human call not found")?;
        if session.account_fence != account_fence || session.generation != generation {
            return Err("stale Human call callback fenced by account/generation".into());
        }
        Ok(session)
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let temp = self.path.with_extension("json.tmp");
        fs::write(&temp, serde_json::to_vec_pretty(&self.state).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        fs::rename(temp, &self.path).map_err(|error| error.to_string())
    }
}

fn validate_id(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 200 || value.chars().any(char::is_control) {
        return Err(format!("{label} identity is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime() -> (tempfile::TempDir, HumanCallRuntime) {
        let root = tempfile::tempdir().unwrap();
        let runtime = HumanCallRuntime::open(root.path().join("human-calls.json"), 1).unwrap();
        (root, runtime)
    }

    #[test]
    fn duplicate_request_converges_but_stale_generation_is_fenced() {
        let (_root, mut runtime) = runtime();
        let first = runtime.create("call-1", "req-1", "conversation-1", "human-a", vec!["human-b".into()], "acct-a", 2).unwrap();
        let replay = runtime.create("call-other", "req-1", "conversation-1", "human-a", vec!["human-b".into()], "acct-a", 3).unwrap();
        assert_eq!(replay.id, first.id);
        assert!(runtime.transition("call-1", "acct-a", first.generation + 1, "accept", None, 4).is_err());
        assert!(runtime.transition("call-1", "acct-b", first.generation, "accept", None, 4).is_err());
    }

    #[test]
    fn transport_lease_rejects_second_device_and_signals_are_monotonic() {
        let (_root, mut runtime) = runtime();
        let call = runtime.create("call-1", "req-1", "conversation-1", "human-a", vec!["human-b".into()], "acct-a", 2).unwrap();
        let owner = runtime.lease("call-1", "acct-a", call.generation, "human-a", "device-a", true).unwrap();
        assert!(owner.is_owner);
        let other = runtime.lease("call-1", "acct-a", call.generation, "human-a", "device-b", true).unwrap();
        assert!(!other.is_owner);
        let first = runtime.send_signal("call-1", "acct-a", call.generation, "device-a", "offer", json!({"sdp":"one"}), 3).unwrap();
        let second = runtime.send_signal("call-1", "acct-a", call.generation, "device-a", "candidate", json!({"candidate":"two"}), 4).unwrap();
        assert_eq!((first.seq, second.seq), (1, 2));
        assert!(runtime.send_signal("call-1", "acct-a", call.generation, "device-b", "answer", json!({}), 5).is_err());
    }

    #[test]
    fn process_reopen_requires_terminal_reconciliation_instead_of_replay() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("human-calls.json");
        let generation;
        {
            let mut runtime = HumanCallRuntime::open(&path, 1).unwrap();
            let call = runtime.create("call-1", "req-1", "conversation-1", "human-a", vec!["human-b".into()], "acct-a", 2).unwrap();
            generation = call.generation;
            runtime.transition("call-1", "acct-a", generation, "accept", None, 3).unwrap();
        }
        let mut reopened = HumanCallRuntime::open(&path, 9).unwrap();
        assert_eq!(reopened.list("conversation-1", "acct-a", 20)[0].state, HumanCallState::OutcomeUnknown);
        assert!(reopened.transition("call-1", "acct-a", generation, "connect", None, 10).is_err());
        let reconciled = reopened.reconcile_unknown("call-1", "acct-a", generation, "ended", Some("remote session no longer active".into()), 11).unwrap();
        assert_eq!(reconciled.state, HumanCallState::Ended);
    }

    #[test]
    fn transition_table_rejects_invalid_edges_and_terminal_media_updates() {
        let (_root, mut runtime) = runtime();
        let call = runtime.create("call-1", "req-1", "conversation-1", "human-a", vec!["human-b".into()], "acct-a", 2).unwrap();
        assert!(runtime.transition("call-1", "acct-a", call.generation, "connect", None, 3).is_err());
        runtime.transition("call-1", "acct-a", call.generation, "accept", None, 4).unwrap();
        runtime.transition("call-1", "acct-a", call.generation, "connect", None, 5).unwrap();
        runtime.transition("call-1", "acct-a", call.generation, "hangup", Some("local hangup".into()), 6).unwrap();
        assert!(runtime.update_media("call-1", "acct-a", call.generation, json!({}), json!({}), 7).is_err());
    }
}
