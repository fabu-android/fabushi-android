use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HumanRemoteOutboundPhase {
    Pending,
    Dispatching,
    AcceptedAwaitingEcho,
    Failed,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HumanRemoteOutboundRecord {
    pub account_slot: Option<String>,
    pub account_fence: String,
    pub actor_id: String,
    pub peer_actor_id: String,
    pub client_request_id: String,
    pub text: String,
    pub reply_to_message_id: Option<String>,
    pub phase: HumanRemoteOutboundPhase,
    pub generation: u64,
    pub server_message_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct HumanRemoteOutboundState {
    generation: u64,
    records: BTreeMap<String, HumanRemoteOutboundRecord>,
}

pub struct DurableHumanRemoteOutboundOwner {
    path: PathBuf,
    state: HumanRemoteOutboundState,
}

impl DurableHumanRemoteOutboundOwner {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        let mut state = match fs::read(&path) {
            Ok(raw) => serde_json::from_slice::<HumanRemoteOutboundState>(&raw)
                .map_err(|error| format!("invalid Human remote outbound journal: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                HumanRemoteOutboundState::default()
            }
            Err(error) => return Err(error.to_string()),
        };
        state.generation = state.generation.saturating_add(1).max(1);
        for record in state.records.values_mut() {
            record.generation = state.generation;
            if record.phase == HumanRemoteOutboundPhase::Dispatching {
                record.phase = HumanRemoteOutboundPhase::OutcomeUnknown;
                record.reason = Some(
                    "Host reopened while Human message dispatch outcome was unknown; reconcile server echo before retry"
                        .into(),
                );
            }
        }
        let owner = Self { path, state };
        owner.persist()?;
        Ok(owner)
    }

    pub fn generation(&self) -> u64 {
        self.state.generation
    }

    pub fn begin(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        actor_id: &str,
        peer_actor_id: &str,
        client_request_id: &str,
        text: &str,
        reply_to_message_id: Option<&str>,
    ) -> Result<HumanRemoteOutboundRecord, String> {
        for (value, label) in [
            (account_fence, "account fence"),
            (actor_id, "actor id"),
            (peer_actor_id, "peer actor id"),
            (client_request_id, "client request id"),
        ] {
            validate_identity(value, label)?;
        }
        if actor_id == peer_actor_id {
            return Err("Human remote outbound peer must differ from the sender".into());
        }
        if text.is_empty() || text.len() > 2 * 1024 * 1024 {
            return Err("Human remote outbound text is invalid".into());
        }
        if let Some(reply) = reply_to_message_id {
            validate_identity(reply, "reply message id")?;
        }
        let key = scope_key(account_slot, account_fence, actor_id, client_request_id);
        if let Some(existing) = self.state.records.get(&key) {
            if existing.peer_actor_id == peer_actor_id
                && existing.text == text
                && existing.reply_to_message_id.as_deref() == reply_to_message_id
            {
                return Ok(existing.clone());
            }
            return Err("Human remote clientRequestId was reused with different payload".into());
        }
        let record = HumanRemoteOutboundRecord {
            account_slot: account_slot.map(str::to_string),
            account_fence: account_fence.to_string(),
            actor_id: actor_id.to_string(),
            peer_actor_id: peer_actor_id.to_string(),
            client_request_id: client_request_id.to_string(),
            text: text.to_string(),
            reply_to_message_id: reply_to_message_id.map(str::to_string),
            phase: HumanRemoteOutboundPhase::Pending,
            generation: self.state.generation,
            server_message_id: None,
            reason: None,
        };
        self.state.records.insert(key, record.clone());
        self.persist()?;
        Ok(record)
    }

    pub fn mark_dispatching(
        &mut self,
        record: &HumanRemoteOutboundRecord,
    ) -> Result<HumanRemoteOutboundRecord, String> {
        self.transition(
            record,
            HumanRemoteOutboundPhase::Dispatching,
            &[HumanRemoteOutboundPhase::Pending],
            None,
            None,
        )
    }

    pub fn mark_accepted(
        &mut self,
        record: &HumanRemoteOutboundRecord,
        server_message_id: &str,
    ) -> Result<HumanRemoteOutboundRecord, String> {
        validate_identity(server_message_id, "server message id")?;
        self.transition(
            record,
            HumanRemoteOutboundPhase::AcceptedAwaitingEcho,
            &[HumanRemoteOutboundPhase::Dispatching],
            Some(server_message_id.to_string()),
            None,
        )
    }

    pub fn mark_failed(
        &mut self,
        record: &HumanRemoteOutboundRecord,
        reason: impl Into<String>,
    ) -> Result<HumanRemoteOutboundRecord, String> {
        self.transition(
            record,
            HumanRemoteOutboundPhase::Failed,
            &[HumanRemoteOutboundPhase::Pending],
            None,
            Some(reason.into()),
        )
    }

    pub fn mark_outcome_unknown(
        &mut self,
        record: &HumanRemoteOutboundRecord,
        reason: impl Into<String>,
    ) -> Result<HumanRemoteOutboundRecord, String> {
        self.transition(
            record,
            HumanRemoteOutboundPhase::OutcomeUnknown,
            &[HumanRemoteOutboundPhase::Dispatching],
            None,
            Some(reason.into()),
        )
    }

    pub fn settle_server_echo(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        actor_id: &str,
        client_request_id: &str,
        server_message_id: &str,
    ) -> Result<bool, String> {
        let key = scope_key(account_slot, account_fence, actor_id, client_request_id);
        let Some(record) = self.state.records.get(&key).cloned() else {
            return Ok(false);
        };
        if !matches!(
            record.phase,
            HumanRemoteOutboundPhase::AcceptedAwaitingEcho
                | HumanRemoteOutboundPhase::OutcomeUnknown
        ) {
            return Ok(false);
        }
        if let Some(expected) = record.server_message_id.as_deref() {
            if expected != server_message_id {
                return Err("Human remote server echo message id mismatched accepted dispatch".into());
            }
        }
        self.state.records.remove(&key);
        self.persist()?;
        Ok(true)
    }

    pub fn record(
        &self,
        account_slot: Option<&str>,
        account_fence: &str,
        actor_id: &str,
        client_request_id: &str,
    ) -> Option<HumanRemoteOutboundRecord> {
        self.state
            .records
            .get(&scope_key(
                account_slot,
                account_fence,
                actor_id,
                client_request_id,
            ))
            .cloned()
    }

    fn transition(
        &mut self,
        source: &HumanRemoteOutboundRecord,
        next: HumanRemoteOutboundPhase,
        allowed: &[HumanRemoteOutboundPhase],
        server_message_id: Option<String>,
        reason: Option<String>,
    ) -> Result<HumanRemoteOutboundRecord, String> {
        let key = scope_key(
            source.account_slot.as_deref(),
            &source.account_fence,
            &source.actor_id,
            &source.client_request_id,
        );
        let record = self
            .state
            .records
            .get_mut(&key)
            .ok_or("Human remote outbound record no longer exists")?;
        if source.generation == 0
            || source.generation != self.state.generation
            || record.generation != source.generation
        {
            return Err("stale Human remote outbound callback fenced by generation".into());
        }
        if !allowed.contains(&record.phase) {
            return Err(format!(
                "Human remote outbound transition from {:?} to {:?} is not allowed",
                record.phase, next
            ));
        }
        record.phase = next;
        if server_message_id.is_some() {
            record.server_message_id = server_message_id;
        }
        record.reason = reason;
        let result = record.clone();
        self.persist()?;
        Ok(result)
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(&self.state)
            .map_err(|error| format!("failed to encode Human remote outbound journal: {error}"))?;
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, bytes).map_err(|error| error.to_string())?;
        fs::rename(&tmp, &self.path).map_err(|error| error.to_string())
    }
}

fn scope_key(
    account_slot: Option<&str>,
    account_fence: &str,
    actor_id: &str,
    client_request_id: &str,
) -> String {
    format!(
        "{}\0{}\0{}\0{}",
        account_slot.unwrap_or("anonymous"),
        account_fence,
        actor_id,
        client_request_id,
    )
}

fn validate_identity(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 1024 || value.chars().any(|ch| ch == '\0') {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_payload_replays_but_mismatched_payload_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut owner =
            DurableHumanRemoteOutboundOwner::open(dir.path().join("human-outbound.json")).unwrap();
        let first = owner
            .begin(
                Some("slot-a"),
                "session:a",
                "human:self",
                "human:peer",
                "client-a",
                "hello",
                None,
            )
            .unwrap();
        let replay = owner
            .begin(
                Some("slot-a"),
                "session:a",
                "human:self",
                "human:peer",
                "client-a",
                "hello",
                None,
            )
            .unwrap();
        assert_eq!(first, replay);
        assert!(owner
            .begin(
                Some("slot-a"),
                "session:a",
                "human:self",
                "human:peer",
                "client-a",
                "different",
                None,
            )
            .is_err());
    }

    #[test]
    fn process_reopen_converts_dispatching_to_outcome_unknown_and_never_blind_retries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("human-outbound.json");
        let original_generation = {
            let mut owner = DurableHumanRemoteOutboundOwner::open(&path).unwrap();
            let pending = owner
                .begin(
                    Some("slot-a"),
                    "session:a",
                    "human:self",
                    "human:peer",
                    "client-a",
                    "hello",
                    None,
                )
                .unwrap();
            let dispatching = owner.mark_dispatching(&pending).unwrap();
            dispatching.generation
        };

        let mut reopened = DurableHumanRemoteOutboundOwner::open(&path).unwrap();
        let recovered = reopened
            .record(Some("slot-a"), "session:a", "human:self", "client-a")
            .unwrap();
        assert!(recovered.generation > original_generation);
        assert_eq!(recovered.phase, HumanRemoteOutboundPhase::OutcomeUnknown);
        assert!(reopened.mark_dispatching(&recovered).is_err());
    }

    #[test]
    fn accepted_and_unknown_records_require_matching_server_echo_to_clear() {
        let dir = tempfile::tempdir().unwrap();
        let mut owner =
            DurableHumanRemoteOutboundOwner::open(dir.path().join("human-outbound.json")).unwrap();
        let pending = owner
            .begin(
                None,
                "session:a",
                "human:self",
                "human:peer",
                "client-a",
                "hello",
                None,
            )
            .unwrap();
        let dispatching = owner.mark_dispatching(&pending).unwrap();
        let accepted = owner.mark_accepted(&dispatching, "42").unwrap();
        assert!(owner
            .settle_server_echo(None, "session:a", "human:self", "client-a", "41")
            .is_err());
        assert!(owner
            .settle_server_echo(None, "session:a", "human:self", "client-a", "42")
            .unwrap());
        assert!(owner
            .record(None, "session:a", "human:self", "client-a")
            .is_none());

        let pending = owner
            .begin(
                None,
                "session:a",
                "human:self",
                "human:peer",
                "client-b",
                "hello again",
                None,
            )
            .unwrap();
        let dispatching = owner.mark_dispatching(&pending).unwrap();
        owner
            .mark_outcome_unknown(&dispatching, "transport closed after write")
            .unwrap();
        assert!(owner
            .settle_server_echo(None, "session:a", "human:self", "client-b", "99")
            .unwrap());
    }
}
