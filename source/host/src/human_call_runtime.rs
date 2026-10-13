use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
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
    #[serde(default)]
    remote_event_seq_by_call: BTreeMap<String, u64>,
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
        if participant_ids.len() < 2 || participant_ids.len() > 64 {
            return Err("call session requires between 2 and 64 participants".into());
        }
        let mut unique = BTreeSet::new();
        for participant in &participant_ids {
            validate_id("participant", participant)?;
            if !unique.insert(participant.clone()) {
                return Err("duplicate call participant".into());
            }
        }
        if !unique.contains(creator_id) {
            return Err("call creator must be a call participant".into());
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
        let session = HumanCallSession {
            id: call_id.into(),
            request_id: request_id.into(),
            scope_id: scope_id.into(),
            creator_id: creator_id.into(),
            participant_ids,
            state: HumanCallState::Invited,
            generation: 0,
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
            (HumanCallState::Negotiating | HumanCallState::Reconnecting, "connected" | "connect") => HumanCallState::Connected,
            (HumanCallState::Connected, "reconnect") => HumanCallState::Reconnecting,
            (HumanCallState::Reconnecting, "resume") => HumanCallState::Negotiating,
            (HumanCallState::Invited | HumanCallState::Ringing, "decline") => HumanCallState::Ended,
            (state, "hangup") if !state.terminal() => HumanCallState::Ended,
            (state, "fail") if !state.terminal() => HumanCallState::Failed,
            _ => return Err("invalid Human call state transition".into()),
        };
        let reconnect_generation = action == "reconnect";
        session.state = next;
        if reconnect_generation {
            session.generation = session.generation.saturating_add(1);
            session.signal_seq = 0;
        }
        session.updated_at_ms = now_ms;
        session.terminal_reason = terminal_reason;
        let terminal = session.state.terminal();
        let copy = session.clone();
        if terminal || reconnect_generation {
            self.state.owner_device_by_call.remove(call_id);
        }
        if reconnect_generation {
            self.state.remote_event_seq_by_call.remove(call_id);
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
        seq: u64,
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
        let current = self.current(call_id, account_fence, generation)?;
        if current.state.terminal() || current.state == HumanCallState::OutcomeUnknown {
            return Err("call cannot signal from current state".into());
        }
        if seq <= current.signal_seq {
            let existing = self
                .state
                .signals
                .get(call_id)
                .and_then(|signals| signals.iter().find(|signal| signal.generation == generation && signal.seq == seq))
                .cloned()
                .ok_or("call signal sequence is stale")?;
            if existing.sender_device_id == sender_device_id
                && existing.kind == kind
                && existing.payload == payload
            {
                return Ok(existing);
            }
            return Err("call signal sequence conflicts with previously accepted payload".into());
        }
        if seq != current.signal_seq.saturating_add(1) {
            return Err(format!(
                "call signal sequence gap: expected {}, received {}",
                current.signal_seq.saturating_add(1),
                seq
            ));
        }
        let session = self.current_mut(call_id, account_fence, generation)?;
        session.signal_seq = seq;
        session.updated_at_ms = now_ms;
        let signal = HumanCallSignal {
            call_id: call_id.into(),
            generation,
            seq,
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

    pub fn remote_event_seq(
        &self,
        call_id: &str,
        account_fence: &str,
        generation: u64,
    ) -> Result<u64, String> {
        self.current(call_id, account_fence, generation)?;
        Ok(self
            .state
            .remote_event_seq_by_call
            .get(call_id)
            .copied()
            .unwrap_or_default())
    }

    pub fn receive_remote_signal(
        &mut self,
        call_id: &str,
        account_fence: &str,
        generation: u64,
        event_seq: u64,
        sender_device_id: &str,
        kind: &str,
        payload: Value,
        now_ms: u64,
    ) -> Result<HumanCallSignal, String> {
        validate_id("remote sender device", sender_device_id)?;
        if !matches!(kind, "offer" | "answer" | "candidate") || !payload.is_object() {
            return Err("invalid remote Human call signal".into());
        }
        let current = self.current(call_id, account_fence, generation)?;
        if current.state.terminal() || current.state == HumanCallState::OutcomeUnknown {
            return Err("call cannot accept remote signaling from current state".into());
        }

        let accepted_event_seq = self
            .state
            .remote_event_seq_by_call
            .get(call_id)
            .copied()
            .unwrap_or_default();
        if event_seq <= accepted_event_seq {
            let existing = self
                .state
                .signals
                .get(call_id)
                .and_then(|signals| {
                    signals.iter().find(|signal| {
                        signal.generation == generation && signal.seq == event_seq
                    })
                })
                .cloned()
                .ok_or("remote call event sequence is stale")?;
            if existing.sender_device_id == sender_device_id
                && existing.kind == kind
                && existing.payload == payload
            {
                return Ok(existing);
            }
            return Err("remote call event sequence conflicts with previously accepted payload".into());
        }

        let signal = HumanCallSignal {
            call_id: call_id.into(),
            generation,
            seq: event_seq,
            sender_device_id: sender_device_id.into(),
            kind: kind.into(),
            payload,
            created_at_ms: now_ms,
        };
        self.state
            .signals
            .entry(call_id.into())
            .or_default()
            .push(signal.clone());
        if let Some(signals) = self.state.signals.get_mut(call_id) {
            signals.sort_by_key(|signal| (signal.generation, signal.seq));
            if signals.len() > 512 {
                let drain = signals.len() - 512;
                signals.drain(0..drain);
            }
        }
        let session = self.current_mut(call_id, account_fence, generation)?;
        session.signal_seq = session.signal_seq.max(event_seq);
        session.updated_at_ms = now_ms;
        self.state
            .remote_event_seq_by_call
            .insert(call_id.into(), event_seq);
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

    pub fn fence_account(
        &mut self,
        account_fence: &str,
        reason: &str,
        now_ms: u64,
    ) -> Result<usize, String> {
        let mut fenced = Vec::new();
        for (call_id, session) in self.state.sessions.iter_mut() {
            if session.account_fence == account_fence
                && !session.state.terminal()
                && session.state != HumanCallState::OutcomeUnknown
            {
                session.state = HumanCallState::OutcomeUnknown;
                session.terminal_reason = Some(reason.to_string());
                session.updated_at_ms = now_ms;
                fenced.push(call_id.clone());
            }
        }
        for call_id in &fenced {
            self.state.owner_device_by_call.remove(call_id);
        }
        if !fenced.is_empty() {
            self.persist()?;
        }
        Ok(fenced.len())
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
        let payload = serde_json::to_vec_pretty(&self.state).map_err(|error| error.to_string())?;
        let result = (|| -> Result<(), String> {
            let mut file = fs::File::create(&temp).map_err(|error| error.to_string())?;
            file.write_all(&payload).map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            fs::rename(&temp, &self.path).map_err(|error| error.to_string())?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
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
        let first = runtime.create("call-1", "req-1", "conversation-1", "human-a", vec!["human-a".into(), "human-b".into()], "acct-a", 2).unwrap();
        let replay = runtime.create("call-other", "req-1", "conversation-1", "human-a", vec!["human-a".into(), "human-b".into()], "acct-a", 3).unwrap();
        assert_eq!(replay.id, first.id);
        assert!(runtime.transition("call-1", "acct-a", first.generation + 1, "accept", None, 4).is_err());
        assert!(runtime.transition("call-1", "acct-b", first.generation, "accept", None, 4).is_err());
    }

    #[test]
    fn transport_lease_rejects_second_device_and_signals_are_monotonic() {
        let (_root, mut runtime) = runtime();
        let call = runtime.create("call-1", "req-1", "conversation-1", "human-a", vec!["human-a".into(), "human-b".into()], "acct-a", 2).unwrap();
        let owner = runtime.lease("call-1", "acct-a", call.generation, "human-a", "device-a", true).unwrap();
        assert!(owner.is_owner);
        let other = runtime.lease("call-1", "acct-a", call.generation, "human-a", "device-b", true).unwrap();
        assert!(!other.is_owner);
        let first = runtime.send_signal("call-1", "acct-a", call.generation, "device-a", 1, "offer", json!({"sdp":"one"}), 3).unwrap();
        let second = runtime.send_signal("call-1", "acct-a", call.generation, "device-a", 2, "candidate", json!({"candidate":"two"}), 4).unwrap();
        assert_eq!((first.seq, second.seq), (1, 2));
        assert!(runtime.send_signal("call-1", "acct-a", call.generation, "device-b", 3, "answer", json!({}), 5).is_err());
    }

    #[test]
    fn durable_store_reopens_after_synced_atomic_replacement() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("human-calls.json");
        {
            let mut runtime = HumanCallRuntime::open(&path, 1).unwrap();
            let call = runtime.create(
                "call-durable",
                "req-durable",
                "conversation-1",
                "human-a",
                vec!["human-a".into(), "human-b".into()],
                "acct-a",
                2,
            ).unwrap();
            runtime.transition("call-durable", "acct-a", call.generation, "accept", None, 3).unwrap();
        }
        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());
        let reopened = HumanCallRuntime::open(&path, 9).unwrap();
        let restored = reopened.list("conversation-1", "acct-a", 20);
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].id, "call-durable");
        assert_eq!(restored[0].state, HumanCallState::OutcomeUnknown);
    }

    #[test]
    fn process_reopen_requires_terminal_reconciliation_instead_of_replay() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("human-calls.json");
        let generation;
        {
            let mut runtime = HumanCallRuntime::open(&path, 1).unwrap();
            let call = runtime.create("call-1", "req-1", "conversation-1", "human-a", vec!["human-a".into(), "human-b".into()], "acct-a", 2).unwrap();
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
    fn remote_event_watermark_accepts_monotonic_gaps_and_rejects_conflicting_replay() {
        let (_root, mut runtime) = runtime();
        let call = runtime
            .create(
                "call-remote",
                "req-remote",
                "conversation-1",
                "human-a",
                vec!["human-a".into(), "human-b".into()],
                "acct-a",
                2,
            )
            .unwrap();
        runtime
            .transition("call-remote", "acct-a", call.generation, "accept", None, 3)
            .unwrap();

        let first = runtime
            .receive_remote_signal(
                "call-remote",
                "acct-a",
                call.generation,
                4,
                "remote-device",
                "offer",
                json!({"sdp":"remote"}),
                4,
            )
            .unwrap();
        assert_eq!(first.seq, 4);
        assert_eq!(
            runtime
                .remote_event_seq("call-remote", "acct-a", call.generation)
                .unwrap(),
            4
        );

        let replay = runtime
            .receive_remote_signal(
                "call-remote",
                "acct-a",
                call.generation,
                4,
                "remote-device",
                "offer",
                json!({"sdp":"remote"}),
                5,
            )
            .unwrap();
        assert_eq!(replay, first);
        assert!(runtime
            .receive_remote_signal(
                "call-remote",
                "acct-a",
                call.generation,
                4,
                "remote-device",
                "offer",
                json!({"sdp":"different"}),
                6,
            )
            .unwrap_err()
            .contains("conflicts"));
        assert!(runtime
            .receive_remote_signal(
                "call-remote",
                "acct-b",
                call.generation,
                5,
                "remote-device",
                "candidate",
                json!({"candidate":"stale-account"}),
                7,
            )
            .is_err());
    }

    #[test]
    fn remote_event_watermark_is_durable_and_resets_on_reconnect_generation() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("human-calls.json");
        let generation;
        {
            let mut runtime = HumanCallRuntime::open(&path, 1).unwrap();
            let call = runtime
                .create(
                    "call-remote",
                    "req-remote",
                    "conversation-1",
                    "human-a",
                    vec!["human-a".into(), "human-b".into()],
                    "acct-a",
                    2,
                )
                .unwrap();
            generation = call.generation;
            runtime
                .transition("call-remote", "acct-a", generation, "accept", None, 3)
                .unwrap();
            runtime
                .receive_remote_signal(
                    "call-remote",
                    "acct-a",
                    generation,
                    9,
                    "remote-device",
                    "answer",
                    json!({"sdp":"answer"}),
                    4,
                )
                .unwrap();
        }

        let mut reopened = HumanCallRuntime::open(&path, 10).unwrap();
        assert_eq!(
            reopened
                .remote_event_seq("call-remote", "acct-a", generation)
                .unwrap(),
            9
        );
        reopened
            .reconcile_unknown(
                "call-remote",
                "acct-a",
                generation,
                "ended",
                Some("recovered remote call no longer active".into()),
                11,
            )
            .unwrap();

        let second = reopened
            .create(
                "call-reconnect",
                "req-reconnect",
                "conversation-1",
                "human-a",
                vec!["human-a".into(), "human-b".into()],
                "acct-a",
                12,
            )
            .unwrap();
        reopened
            .transition("call-reconnect", "acct-a", second.generation, "accept", None, 13)
            .unwrap();
        reopened
            .transition("call-reconnect", "acct-a", second.generation, "connected", None, 14)
            .unwrap();
        reopened
            .receive_remote_signal(
                "call-reconnect",
                "acct-a",
                second.generation,
                7,
                "remote-device",
                "candidate",
                json!({"candidate":"before-reconnect"}),
                15,
            )
            .unwrap();
        let reconnected = reopened
            .transition("call-reconnect", "acct-a", second.generation, "reconnect", None, 16)
            .unwrap();
        assert_eq!(reconnected.generation, second.generation + 1);
        assert_eq!(
            reopened
                .remote_event_seq("call-reconnect", "acct-a", reconnected.generation)
                .unwrap(),
            0
        );
        assert!(reopened
            .receive_remote_signal(
                "call-reconnect",
                "acct-a",
                second.generation,
                8,
                "remote-device",
                "candidate",
                json!({"candidate":"old-generation"}),
                17,
            )
            .is_err());
    }

    #[test]
    fn account_switch_fences_live_calls_and_releases_transport_owner() {
        let (_root, mut runtime) = runtime();
        let call = runtime.create("call-1", "req-1", "conversation-1", "human-a", vec!["human-a".into(), "human-b".into()], "acct-a", 2).unwrap();
        runtime.lease("call-1", "acct-a", call.generation, "human-a", "device-a", true).unwrap();
        assert_eq!(runtime.fence_account("acct-a", "account switched", 3).unwrap(), 1);
        assert_eq!(runtime.list("conversation-1", "acct-a", 20)[0].state, HumanCallState::OutcomeUnknown);
        assert!(runtime.send_signal("call-1", "acct-a", call.generation, "device-a", 1, "offer", json!({}), 4).is_err());
    }

    #[test]
    fn transition_table_rejects_invalid_edges_and_terminal_media_updates() {
        let (_root, mut runtime) = runtime();
        let call = runtime.create("call-1", "req-1", "conversation-1", "human-a", vec!["human-a".into(), "human-b".into()], "acct-a", 2).unwrap();
        assert!(runtime.transition("call-1", "acct-a", call.generation, "connect", None, 3).is_err());
        runtime.transition("call-1", "acct-a", call.generation, "accept", None, 4).unwrap();
        runtime.transition("call-1", "acct-a", call.generation, "connected", None, 5).unwrap();
        runtime.transition("call-1", "acct-a", call.generation, "reconnect", None, 6).unwrap();
        runtime.transition("call-1", "acct-a", call.generation, "resume", None, 7).unwrap();
        runtime.transition("call-1", "acct-a", call.generation, "connected", None, 8).unwrap();
        runtime.transition("call-1", "acct-a", call.generation, "hangup", Some("local hangup".into()), 9).unwrap();
        assert!(runtime.update_media("call-1", "acct-a", call.generation, json!({}), json!({}), 7).is_err());
    }
}
