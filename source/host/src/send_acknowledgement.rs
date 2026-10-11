use crate::agent_attachment::AgentAttachment;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SendAcknowledgementPhase {
    Pending,
    Queued,
    Dispatching,
    AcceptedAwaitingEcho,
    Failed,
    OutcomeUnknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SendAcknowledgementEntryKind {
    Message,
    UserAttachment,
}

impl SendAcknowledgementEntryKind {
    pub fn from_wire(value: &str) -> Result<Self, String> {
        match value {
            "message" => Ok(Self::Message),
            "user-attachment" => Ok(Self::UserAttachment),
            _ => Err(format!("unsupported send acknowledgement entry kind: {value}")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SendAcknowledgementEntry {
    pub id: String,
    pub kind: SendAcknowledgementEntryKind,
    pub client_nonce: String,
}

impl SendAcknowledgementEntry {
    pub fn list_from_json(value: &Value) -> Result<Vec<Self>, String> {
        let rows = value
            .as_array()
            .ok_or("send acknowledgement entries array is required")?;
        rows.iter()
            .map(|row| {
                let id = required_string(row, "id")?.to_string();
                let kind = SendAcknowledgementEntryKind::from_wire(required_string(row, "kind")?)?;
                let client_nonce = required_string(row, "clientNonce")?.to_string();
                Ok(Self {
                    id,
                    kind,
                    client_nonce,
                })
            })
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SendAcknowledgementRecord {
    pub account_slot: Option<String>,
    pub account_fence: String,
    pub agent_id: String,
    pub nonce: String,
    pub prior_nonces: Vec<String>,
    pub entries: Vec<SendAcknowledgementEntry>,
    pub message_text: String,
    #[serde(default)]
    pub attachments: Vec<AgentAttachment>,
    pub phase: SendAcknowledgementPhase,
    pub generation: u64,
    pub failed_at_ms: Option<u64>,
    pub reason: Option<String>,
}

#[derive(Default, Deserialize, Serialize)]
struct SendAcknowledgementState {
    owner_generation: u64,
    records: BTreeMap<String, SendAcknowledgementRecord>,
    seen_echoes: BTreeMap<String, BTreeMap<String, SendAcknowledgementEntryKind>>,
}

pub struct DurableSendAcknowledgementOwner {
    path: PathBuf,
    state: SendAcknowledgementState,
}

impl DurableSendAcknowledgementOwner {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let path = path.into();
        let mut state = match fs::read(&path) {
            Ok(raw) => serde_json::from_slice::<SendAcknowledgementState>(&raw)
                .map_err(|error| format!("invalid send acknowledgement journal: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                SendAcknowledgementState::default()
            }
            Err(error) => return Err(error.to_string()),
        };
        state.owner_generation = state.owner_generation.saturating_add(1).max(1);
        for record in state.records.values_mut() {
            record.generation = state.owner_generation;
            if record.phase == SendAcknowledgementPhase::Dispatching {
                record.phase = SendAcknowledgementPhase::OutcomeUnknown;
                record.reason = Some(
                    "Host reopened while dispatch outcome was unknown; reconcile canonical echo before retry"
                        .into(),
                );
                record.failed_at_ms = Some(now_ms);
            }
        }
        let owner = Self { path, state };
        owner.persist()?;
        Ok(owner)
    }

    pub fn generation(&self) -> u64 {
        self.state.owner_generation
    }

    pub fn begin(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        nonce: &str,
        message_text: &str,
        entries: Vec<SendAcknowledgementEntry>,
    ) -> Result<SendAcknowledgementRecord, String> {
        self.begin_with_attachments(
            account_slot,
            account_fence,
            agent_id,
            nonce,
            message_text,
            Vec::new(),
            entries,
        )
    }

    pub fn begin_with_attachments(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        nonce: &str,
        message_text: &str,
        attachments: Vec<AgentAttachment>,
        entries: Vec<SendAcknowledgementEntry>,
    ) -> Result<SendAcknowledgementRecord, String> {
        validate_identity(account_fence, "account fence")?;
        validate_optional_identity(account_slot, "account slot")?;
        validate_identity(agent_id, "agent id")?;
        validate_identity(nonce, "send nonce")?;
        if message_text.trim().is_empty() && attachments.is_empty() {
            return Err("send acknowledgement requires message text or an attachment".into());
        }
        if message_text.len() > 2 * 1024 * 1024 {
            return Err("send acknowledgement message text is too large".into());
        }
        for attachment in &attachments {
            attachment.validate_public()?;
        }
        validate_entries(nonce, &entries)?;

        if let Some(key) = self.find_key(account_slot, account_fence, agent_id, nonce) {
            let existing = self.state.records.get(&key).expect("located record");
            if existing.nonce == nonce
                && existing.message_text == message_text
                && existing.attachments == attachments
                && existing.entries == entries
            {
                return Ok(existing.clone());
            }
            return Err("send nonce was reused with different payload".into());
        }

        let record = SendAcknowledgementRecord {
            account_slot: account_slot.map(str::to_string),
            account_fence: account_fence.to_string(),
            agent_id: agent_id.to_string(),
            nonce: nonce.to_string(),
            prior_nonces: Vec::new(),
            entries,
            message_text: message_text.to_string(),
            attachments,
            phase: SendAcknowledgementPhase::Pending,
            generation: self.state.owner_generation,
            failed_at_ms: None,
            reason: None,
        };
        let key = scope_key(account_slot, account_fence, agent_id, nonce);
        self.state.records.insert(key, record.clone());
        self.persist()?;
        Ok(record)
    }

    pub fn mark_queued(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        nonce: &str,
        generation: u64,
    ) -> Result<SendAcknowledgementRecord, String> {
        self.transition(
            account_slot,
            account_fence,
            agent_id,
            nonce,
            generation,
            SendAcknowledgementPhase::Queued,
            &[SendAcknowledgementPhase::Pending],
        )
    }

    pub fn mark_dispatching(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        nonce: &str,
        generation: u64,
    ) -> Result<SendAcknowledgementRecord, String> {
        self.transition(
            account_slot,
            account_fence,
            agent_id,
            nonce,
            generation,
            SendAcknowledgementPhase::Dispatching,
            &[
                SendAcknowledgementPhase::Pending,
                SendAcknowledgementPhase::Queued,
            ],
        )
    }

    pub fn mark_accepted(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        nonce: &str,
        generation: u64,
    ) -> Result<SendAcknowledgementRecord, String> {
        self.transition(
            account_slot,
            account_fence,
            agent_id,
            nonce,
            generation,
            SendAcknowledgementPhase::AcceptedAwaitingEcho,
            &[SendAcknowledgementPhase::Dispatching],
        )
    }

    pub fn mark_failed(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        nonce: &str,
        generation: u64,
        failed_at_ms: u64,
        reason: impl Into<String>,
    ) -> Result<SendAcknowledgementRecord, String> {
        let key = self.require_key(account_slot, account_fence, agent_id, nonce)?;
        let record = self.state.records.get_mut(&key).expect("located record");
        assert_generation(record, generation)?;
        if matches!(
            record.phase,
            SendAcknowledgementPhase::AcceptedAwaitingEcho
                | SendAcknowledgementPhase::OutcomeUnknown
        ) {
            return Err("accepted/outcome-unknown send cannot be downgraded to failed".into());
        }
        if record.phase != SendAcknowledgementPhase::Failed {
            record.phase = SendAcknowledgementPhase::Failed;
            record.failed_at_ms = Some(failed_at_ms);
            record.reason = Some(reason.into());
            self.persist()?;
        }
        Ok(self.state.records.get(&key).expect("record retained").clone())
    }

    pub fn mark_outcome_unknown(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        nonce: &str,
        generation: u64,
        observed_at_ms: u64,
        reason: impl Into<String>,
    ) -> Result<SendAcknowledgementRecord, String> {
        let key = self.require_key(account_slot, account_fence, agent_id, nonce)?;
        let record = self.state.records.get_mut(&key).expect("located record");
        assert_generation(record, generation)?;
        if record.phase == SendAcknowledgementPhase::Failed {
            return Err("failed send cannot become outcome-unknown without a new dispatch".into());
        }
        if record.phase != SendAcknowledgementPhase::OutcomeUnknown {
            record.phase = SendAcknowledgementPhase::OutcomeUnknown;
            record.failed_at_ms = Some(observed_at_ms);
            record.reason = Some(reason.into());
            self.persist()?;
        }
        Ok(self.state.records.get(&key).expect("record retained").clone())
    }

    pub fn retry_failed(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        nonce: &str,
        generation: u64,
        fresh_nonce: &str,
        entries: Vec<SendAcknowledgementEntry>,
    ) -> Result<SendAcknowledgementRecord, String> {
        validate_identity(fresh_nonce, "fresh send nonce")?;
        validate_entries(fresh_nonce, &entries)?;
        let old_key = self.require_key(account_slot, account_fence, agent_id, nonce)?;
        let old = self.state.records.get(&old_key).expect("located record").clone();
        assert_generation(&old, generation)?;
        if old.phase != SendAcknowledgementPhase::Failed {
            return Err("only a proven failed send may be explicitly retried".into());
        }
        if self.state.records.values().any(|record| {
            record.account_fence == account_fence
                && record.account_slot.as_deref() == account_slot
                && record.agent_id == agent_id
                && record.nonce == fresh_nonce
        }) {
            return Err("fresh retry nonce is already in use".into());
        }

        let mut prior_nonces = old.prior_nonces.clone();
        if !prior_nonces.iter().any(|value| value == &old.nonce) {
            prior_nonces.push(old.nonce.clone());
        }
        self.state.records.remove(&old_key);
        self.state.seen_echoes.remove(&old_key);

        let record = SendAcknowledgementRecord {
            account_slot: old.account_slot,
            account_fence: old.account_fence,
            agent_id: old.agent_id,
            nonce: fresh_nonce.to_string(),
            prior_nonces,
            entries,
            message_text: old.message_text,
            attachments: old.attachments,
            phase: SendAcknowledgementPhase::Pending,
            generation: self.state.owner_generation,
            failed_at_ms: None,
            reason: None,
        };
        let key = scope_key(
            account_slot,
            account_fence,
            agent_id,
            fresh_nonce,
        );
        self.state.records.insert(key, record.clone());
        self.persist()?;
        Ok(record)
    }

    pub fn ingest_echo(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        generation: u64,
        entry_id: &str,
        entry_kind: SendAcknowledgementEntryKind,
        client_nonce: &str,
    ) -> Result<bool, String> {
        let Some(key) = self.find_key(account_slot, account_fence, agent_id, client_nonce) else {
            return Ok(false);
        };
        let record = self.state.records.get(&key).expect("located record").clone();
        assert_generation(&record, generation)?;
        if !record_matches_nonce(&record, client_nonce) {
            return Ok(false);
        }

        let seen = self.state.seen_echoes.entry(key.clone()).or_default();
        seen.entry(entry_id.to_string()).or_insert(entry_kind);

        let echoed_message = seen
            .values()
            .any(|kind| *kind == SendAcknowledgementEntryKind::Message);
        let expected_attachments = record
            .entries
            .iter()
            .filter(|entry| entry.kind == SendAcknowledgementEntryKind::UserAttachment)
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>();
        let has_message = record
            .entries
            .iter()
            .any(|entry| entry.kind == SendAcknowledgementEntryKind::Message);
        let attachments_complete = !expected_attachments.is_empty()
            && expected_attachments.iter().all(|expected| {
                seen.get(*expected) == Some(&SendAcknowledgementEntryKind::UserAttachment)
            });
        let complete = echoed_message || (!has_message && attachments_complete);

        if complete {
            self.state.records.remove(&key);
            self.state.seen_echoes.remove(&key);
        }
        self.persist()?;
        Ok(complete)
    }

    pub fn reconcile_transcript(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        entries: &[Value],
    ) -> Result<usize, String> {
        let generation = self.state.owner_generation;
        let mut settled = 0usize;
        for entry in entries {
            let Some(client_nonce) = entry
                .get("clientNonce")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            else {
                continue;
            };
            if entry.get("agentId").and_then(Value::as_str) != Some(agent_id) {
                continue;
            }
            let kind = match entry.get("kind").and_then(Value::as_str) {
                Some("message") if entry.get("role").and_then(Value::as_str) == Some("user") => {
                    if let Some(key) = self.find_key(account_slot, account_fence, agent_id, client_nonce) {
                        let record = self.state.records.get(&key).expect("located record");
                        if !message_echo_matches_attachments(entry, &record.attachments) {
                            continue;
                        }
                    }
                    SendAcknowledgementEntryKind::Message
                }
                Some("user-attachment") => SendAcknowledgementEntryKind::UserAttachment,
                _ => continue,
            };
            let entry_id = entry
                .get("id")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(client_nonce);
            if self.ingest_echo(
                account_slot,
                account_fence,
                agent_id,
                generation,
                entry_id,
                kind,
                client_nonce,
            )? {
                settled += 1;
            }
        }
        Ok(settled)
    }

    pub fn snapshot(
        &self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
    ) -> Vec<SendAcknowledgementRecord> {
        self.state
            .records
            .values()
            .filter(|record| {
                record.account_fence == account_fence
                    && record.account_slot.as_deref() == account_slot
                    && record.agent_id == agent_id
            })
            .cloned()
            .collect()
    }

    pub fn projection(
        &self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
    ) -> Value {
        json!({
            "generation": self.state.owner_generation,
            "records": self.snapshot(account_slot, account_fence, agent_id),
        })
    }

    pub fn record(
        &self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        nonce: &str,
    ) -> Option<SendAcknowledgementRecord> {
        self.find_key(account_slot, account_fence, agent_id, nonce)
            .and_then(|key| self.state.records.get(&key).cloned())
    }

    fn transition(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        nonce: &str,
        generation: u64,
        next: SendAcknowledgementPhase,
        allowed: &[SendAcknowledgementPhase],
    ) -> Result<SendAcknowledgementRecord, String> {
        let key = self.require_key(account_slot, account_fence, agent_id, nonce)?;
        let record = self.state.records.get_mut(&key).expect("located record");
        assert_generation(record, generation)?;
        if record.phase == next {
            return Ok(record.clone());
        }
        if !allowed.contains(&record.phase) {
            return Err(format!(
                "send acknowledgement transition {:?} -> {:?} is not allowed",
                record.phase, next
            ));
        }
        record.phase = next;
        record.failed_at_ms = None;
        record.reason = None;
        self.persist()?;
        Ok(self.state.records.get(&key).expect("record retained").clone())
    }

    fn require_key(
        &self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        nonce: &str,
    ) -> Result<String, String> {
        self.find_key(account_slot, account_fence, agent_id, nonce)
            .ok_or_else(|| "send acknowledgement record is missing or fenced to another scope".into())
    }

    fn find_key(
        &self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        nonce: &str,
    ) -> Option<String> {
        self.state.records.iter().find_map(|(key, record)| {
            (record.account_fence == account_fence
                && record.account_slot.as_deref() == account_slot
                && record.agent_id == agent_id
                && record_matches_nonce(record, nonce))
            .then(|| key.clone())
        })
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let temporary = self.path.with_extension("json.tmp");
        fs::write(
            &temporary,
            serde_json::to_vec_pretty(&self.state).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        fs::rename(temporary, &self.path).map_err(|error| error.to_string())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn message_echo_matches_attachments(entry: &Value, expected: &[AgentAttachment]) -> bool {
    if expected.is_empty() {
        return true;
    }
    let Some(rows) = entry.get("attachments").and_then(Value::as_array) else {
        return false;
    };
    expected.iter().all(|attachment| {
        rows.iter().any(|row| {
            row.get("id").and_then(Value::as_str) == Some(attachment.id.as_str())
                && row.get("name").and_then(Value::as_str) == Some(attachment.name.as_str())
                && row.get("size").and_then(Value::as_u64) == Some(attachment.size_bytes)
        })
    })
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{key} is required"))
}

fn validate_entries(nonce: &str, entries: &[SendAcknowledgementEntry]) -> Result<(), String> {
    if entries.is_empty() {
        return Err("send acknowledgement requires at least one optimistic entry".into());
    }
    let mut ids = BTreeMap::new();
    for entry in entries {
        validate_identity(&entry.id, "optimistic entry id")?;
        if entry.client_nonce != nonce {
            return Err("optimistic entry clientNonce must equal the current send nonce".into());
        }
        if entry.kind == SendAcknowledgementEntryKind::UserAttachment
            && !entry.id.contains(nonce)
        {
            return Err("attachment optimistic id must include the current send nonce".into());
        }
        if ids.insert(entry.id.as_str(), true).is_some() {
            return Err("optimistic entry ids must be unique".into());
        }
    }
    Ok(())
}

fn validate_optional_identity(value: Option<&str>, label: &str) -> Result<(), String> {
    if let Some(value) = value {
        validate_identity(value, label)?;
    }
    Ok(())
}

fn validate_identity(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 1024 || value.chars().any(|ch| ch == '\0') {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

fn assert_generation(record: &SendAcknowledgementRecord, generation: u64) -> Result<(), String> {
    if generation == 0 || record.generation != generation {
        return Err("stale send acknowledgement callback fenced by generation".into());
    }
    Ok(())
}

fn scope_key(
    account_slot: Option<&str>,
    account_fence: &str,
    agent_id: &str,
    nonce: &str,
) -> String {
    format!(
        "{}\0{}\0{}\0{}",
        account_slot.unwrap_or("anonymous"),
        account_fence,
        agent_id,
        nonce
    )
}

fn record_matches_nonce(record: &SendAcknowledgementRecord, nonce: &str) -> bool {
    let logical = logical_nonce(nonce);
    logical_nonce(&record.nonce) == logical
        || record
            .prior_nonces
            .iter()
            .any(|prior| logical_nonce(prior) == logical)
}

fn logical_nonce(nonce: &str) -> &str {
    const PREFIX: &str = "sand-resend-v1:";
    let Some(rest) = nonce.strip_prefix(PREFIX) else {
        return nonce;
    };
    let Some(length_end) = rest.find(':') else {
        return nonce;
    };
    let Ok(original_length) = rest[..length_end].parse::<usize>() else {
        return nonce;
    };
    let original_start = length_end + 1;
    let original_end = original_start.saturating_add(original_length);
    if original_end >= rest.len() || rest.as_bytes().get(original_end) != Some(&b':') {
        return nonce;
    }
    rest.get(original_start..original_end).unwrap_or(nonce)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message(nonce: &str) -> Vec<SendAcknowledgementEntry> {
        vec![SendAcknowledgementEntry {
            id: nonce.into(),
            kind: SendAcknowledgementEntryKind::Message,
            client_nonce: nonce.into(),
        }]
    }

    fn attachments(nonce: &str) -> Vec<SendAcknowledgementEntry> {
        vec![
            SendAcknowledgementEntry {
                id: format!("attachment:{nonce}:1"),
                kind: SendAcknowledgementEntryKind::UserAttachment,
                client_nonce: nonce.into(),
            },
            SendAcknowledgementEntry {
                id: format!("attachment:{nonce}:2"),
                kind: SendAcknowledgementEntryKind::UserAttachment,
                client_nonce: nonce.into(),
            },
        ]
    }

    #[test]
    fn success_duplicate_and_accepted_before_echo_reconcile_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("send-acks.json");
        let mut owner = DurableSendAcknowledgementOwner::open(&path, 1).unwrap();
        let first = owner
            .begin(
                Some("slot-a"),
                "session:a",
                "agent-a",
                "nonce-a",
                "hello",
                message("nonce-a"),
            )
            .unwrap();
        let duplicate = owner
            .begin(
                Some("slot-a"),
                "session:a",
                "agent-a",
                "nonce-a",
                "hello",
                message("nonce-a"),
            )
            .unwrap();
        assert_eq!(first, duplicate);
        owner
            .mark_queued(Some("slot-a"), "session:a", "agent-a", "nonce-a", first.generation)
            .unwrap();
        owner
            .mark_dispatching(Some("slot-a"), "session:a", "agent-a", "nonce-a", first.generation)
            .unwrap();
        let accepted = owner
            .mark_accepted(Some("slot-a"), "session:a", "agent-a", "nonce-a", first.generation)
            .unwrap();
        assert_eq!(accepted.phase, SendAcknowledgementPhase::AcceptedAwaitingEcho);
        assert_eq!(owner.snapshot(Some("slot-a"), "session:a", "agent-a").len(), 1);
        assert!(owner
            .ingest_echo(
                Some("slot-a"),
                "session:a",
                "agent-a",
                first.generation,
                "nonce-a",
                SendAcknowledgementEntryKind::Message,
                "nonce-a",
            )
            .unwrap());
        assert!(owner.snapshot(Some("slot-a"), "session:a", "agent-a").is_empty());
        assert!(!owner
            .ingest_echo(
                Some("slot-a"),
                "session:a",
                "agent-a",
                first.generation,
                "nonce-a",
                SendAcknowledgementEntryKind::Message,
                "nonce-a",
            )
            .unwrap());
    }

    #[test]
    fn attachment_echo_waits_for_all_expected_rows() {
        let dir = tempfile::tempdir().unwrap();
        let mut owner = DurableSendAcknowledgementOwner::open(dir.path().join("acks.json"), 1).unwrap();
        let record = owner
            .begin(
                Some("slot-a"),
                "session:a",
                "agent-a",
                "nonce-a",
                "attachments",
                attachments("nonce-a"),
            )
            .unwrap();
        assert!(!owner
            .ingest_echo(
                Some("slot-a"),
                "session:a",
                "agent-a",
                record.generation,
                "attachment:nonce-a:1",
                SendAcknowledgementEntryKind::UserAttachment,
                "nonce-a",
            )
            .unwrap());
        assert!(owner
            .ingest_echo(
                Some("slot-a"),
                "session:a",
                "agent-a",
                record.generation,
                "attachment:nonce-a:2",
                SendAcknowledgementEntryKind::UserAttachment,
                "nonce-a",
            )
            .unwrap());
    }

    #[test]
    fn failure_requires_explicit_retry_and_preserves_logical_nonce_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut owner = DurableSendAcknowledgementOwner::open(dir.path().join("acks.json"), 1).unwrap();
        let record = owner
            .begin(None, "session:a", "agent-a", "nonce-a", "hello", message("nonce-a"))
            .unwrap();
        owner
            .mark_queued(None, "session:a", "agent-a", "nonce-a", record.generation)
            .unwrap();
        owner
            .mark_failed(
                None,
                "session:a",
                "agent-a",
                "nonce-a",
                record.generation,
                2,
                "proven not dispatched",
            )
            .unwrap();
        assert!(owner
            .mark_dispatching(None, "session:a", "agent-a", "nonce-a", record.generation)
            .is_err());
        let fresh = "sand-resend-v1:7:nonce-a:retry-1";
        let retried = owner
            .retry_failed(
                None,
                "session:a",
                "agent-a",
                "nonce-a",
                record.generation,
                fresh,
                message(fresh),
            )
            .unwrap();
        assert!(retried.prior_nonces.contains(&"nonce-a".to_string()));
        assert!(owner.record(None, "session:a", "agent-a", "nonce-a").is_some());
        assert_eq!(retried.phase, SendAcknowledgementPhase::Pending);
    }

    #[test]
    fn account_agent_and_generation_fences_discard_stale_callbacks() {
        let dir = tempfile::tempdir().unwrap();
        let mut owner = DurableSendAcknowledgementOwner::open(dir.path().join("acks.json"), 1).unwrap();
        let record = owner
            .begin(
                Some("slot-a"),
                "session:a",
                "agent-a",
                "nonce-a",
                "hello",
                message("nonce-a"),
            )
            .unwrap();
        assert!(owner
            .mark_queued(Some("slot-b"), "session:a", "agent-a", "nonce-a", record.generation)
            .is_err());
        assert!(owner
            .mark_queued(Some("slot-a"), "session:a", "agent-b", "nonce-a", record.generation)
            .is_err());
        assert!(owner
            .mark_queued(Some("slot-a"), "session:b", "agent-a", "nonce-a", record.generation)
            .is_err());
        assert!(owner
            .mark_queued(Some("slot-a"), "session:a", "agent-a", "nonce-a", record.generation + 1)
            .is_err());
    }

    #[test]
    fn process_reopen_converts_dispatching_to_outcome_unknown_and_never_blind_retries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("acks.json");
        let generation = {
            let mut owner = DurableSendAcknowledgementOwner::open(&path, 1).unwrap();
            let record = owner
                .begin(None, "session:a", "agent-a", "nonce-a", "hello", message("nonce-a"))
                .unwrap();
            owner
                .mark_queued(None, "session:a", "agent-a", "nonce-a", record.generation)
                .unwrap();
            owner
                .mark_dispatching(None, "session:a", "agent-a", "nonce-a", record.generation)
                .unwrap();
            record.generation
        };
        let mut reopened = DurableSendAcknowledgementOwner::open(&path, 2).unwrap();
        let snapshot = reopened.snapshot(None, "session:a", "agent-a");
        assert_eq!(snapshot.len(), 1);
        let record = snapshot[0].clone();
        assert!(record.generation > generation);
        assert_eq!(record.phase, SendAcknowledgementPhase::OutcomeUnknown);
        assert!(reopened
            .retry_failed(
                None,
                "session:a",
                "agent-a",
                "nonce-a",
                record.generation,
                "fresh",
                message("fresh"),
            )
            .is_err());
    }

    #[test]
    fn accepted_reopen_reconciles_canonical_message_echo_without_resend() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("acks.json");
        {
            let mut owner = DurableSendAcknowledgementOwner::open(&path, 1).unwrap();
            let record = owner
                .begin(None, "session:a", "agent-a", "nonce-a", "hello", message("nonce-a"))
                .unwrap();
            owner
                .mark_queued(None, "session:a", "agent-a", "nonce-a", record.generation)
                .unwrap();
            owner
                .mark_dispatching(None, "session:a", "agent-a", "nonce-a", record.generation)
                .unwrap();
            owner
                .mark_accepted(None, "session:a", "agent-a", "nonce-a", record.generation)
                .unwrap();
        }
        let mut reopened = DurableSendAcknowledgementOwner::open(&path, 2).unwrap();
        assert_eq!(
            reopened.snapshot(None, "session:a", "agent-a")[0].phase,
            SendAcknowledgementPhase::AcceptedAwaitingEcho
        );
        let transcript = vec![json!({
            "id":"nonce-a",
            "kind":"message",
            "role":"user",
            "agentId":"agent-a",
            "clientNonce":"nonce-a"
        })];
        assert_eq!(
            reopened
                .reconcile_transcript(None, "session:a", "agent-a", &transcript)
                .unwrap(),
            1
        );
        assert!(reopened.snapshot(None, "session:a", "agent-a").is_empty());
    }
}
