use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::{fs, io::Write, path::PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DurableTurnState {
    Running,
    Completed,
    Failed,
    Cancelled,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DurableTurnRecord {
    pub request_id: String,
    pub operation_id: String,
    pub account_fence: String,
    pub generation: u64,
    pub state: DurableTurnState,
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
    pub reason: Option<String>,
    #[serde(default)]
    pub request_source: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub checkpoint_attempt: Option<usize>,
    #[serde(default)]
    pub checkpoint_emitted_chunks: Option<usize>,
    #[serde(default)]
    pub checkpointed_at_ms: Option<u64>,
}

#[derive(Default, Deserialize, Serialize)]
struct JournalState {
    next_generation: u64,
    records: BTreeMap<String, DurableTurnRecord>,
}

pub struct DurableTurnJournal {
    path: PathBuf,
    state: JournalState,
}

impl DurableTurnJournal {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let path = path.into();
        let mut state = if path.exists() {
            serde_json::from_slice::<JournalState>(&fs::read(&path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("invalid durable turn journal: {e}"))?
        } else {
            JournalState::default()
        };
        let mut changed = false;
        for record in state.records.values_mut() {
            if record.state == DurableTurnState::Running {
                record.state = DurableTurnState::OutcomeUnknown;
                record.updated_at_ms = now_ms;
                record.reason = Some(
                    "Host restarted while the run was in flight; reconcile durable transcript/provider state before any retry"
                        .into(),
                );
                changed = true;
            }
        }
        let journal = Self { path, state };
        if changed {
            journal.persist()?;
        }
        Ok(journal)
    }

    pub fn begin(
        &mut self,
        request_id: &str,
        operation_id: &str,
        account_fence: &str,
        now_ms: u64,
    ) -> Result<u64, String> {
        self.begin_with_source(request_id, operation_id, account_fence, None, now_ms)
    }

    pub fn begin_with_source(
        &mut self,
        request_id: &str,
        operation_id: &str,
        account_fence: &str,
        request_source: Option<&str>,
        now_ms: u64,
    ) -> Result<u64, String> {
        self.begin_with_context(
            request_id,
            operation_id,
            account_fence,
            request_source,
            None,
            now_ms,
        )
    }

    pub fn begin_with_context(
        &mut self,
        request_id: &str,
        operation_id: &str,
        account_fence: &str,
        request_source: Option<&str>,
        agent_id: Option<&str>,
        now_ms: u64,
    ) -> Result<u64, String> {
        if agent_id.is_some_and(|agent_id| agent_id.trim().is_empty()) {
            return Err("durable turn agent id must be non-empty when provided".into());
        }
        if let Some(existing) = self.state.records.get(request_id) {
            return match existing.state {
                DurableTurnState::OutcomeUnknown => Err(format!(
                    "request {request_id} has outcome-unknown state; reconcile before retry"
                )),
                DurableTurnState::Running => Err(format!(
                    "request {request_id} is already running as {}",
                    existing.operation_id
                )),
                _ => Err(format!(
                    "request {request_id} already has terminal state {:?}",
                    existing.state
                )),
            };
        }
        if self.state.records.values().any(|record| {
            record.operation_id == operation_id && record.state == DurableTurnState::Running
        }) {
            return Err(format!("operation {operation_id} is already running"));
        }
        self.state.next_generation = self.state.next_generation.saturating_add(1).max(1);
        let generation = self.state.next_generation;
        self.state.records.insert(
            request_id.into(),
            DurableTurnRecord {
                request_id: request_id.into(),
                operation_id: operation_id.into(),
                account_fence: account_fence.into(),
                generation,
                state: DurableTurnState::Running,
                started_at_ms: now_ms,
                updated_at_ms: now_ms,
                reason: None,
                request_source: request_source.map(str::to_string),
                agent_id: agent_id.map(str::to_string),
                checkpoint_attempt: None,
                checkpoint_emitted_chunks: None,
                checkpointed_at_ms: None,
            },
        );
        self.persist()?;
        Ok(generation)
    }

    pub fn assert_current(
        &self,
        request_id: &str,
        operation_id: &str,
        account_fence: &str,
        generation: u64,
    ) -> Result<(), String> {
        let record = self
            .state
            .records
            .get(request_id)
            .ok_or("durable turn record is missing")?;
        if record.state != DurableTurnState::Running {
            return Err(format!("durable turn is {:?}", record.state));
        }
        if record.operation_id != operation_id
            || record.account_fence != account_fence
            || record.generation != generation
        {
            return Err("stale turn callback fenced by operation/account/generation".into());
        }
        Ok(())
    }

    pub fn persist_checkpoint_metadata(
        &mut self,
        request_id: &str,
        operation_id: &str,
        account_fence: &str,
        generation: u64,
        attempt: usize,
        emitted_chunks: usize,
        now_ms: u64,
    ) -> Result<(), String> {
        self.assert_current(request_id, operation_id, account_fence, generation)?;
        let record = self.state.records.get_mut(request_id).expect("asserted record");
        record.checkpoint_attempt = Some(attempt);
        record.checkpoint_emitted_chunks = Some(emitted_chunks);
        record.checkpointed_at_ms = Some(now_ms);
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn settle(
        &mut self,
        request_id: &str,
        operation_id: &str,
        account_fence: &str,
        generation: u64,
        state: DurableTurnState,
        reason: Option<String>,
        now_ms: u64,
    ) -> Result<(), String> {
        if matches!(state, DurableTurnState::Running) {
            return Err("terminal settlement cannot use running state".into());
        }
        self.assert_current(request_id, operation_id, account_fence, generation)?;
        let record = self.state.records.get_mut(request_id).expect("asserted record");
        record.state = state;
        record.reason = reason;
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn settle_operation_cancelled(
        &mut self,
        operation_id: &str,
        reason: &str,
        now_ms: u64,
    ) -> Result<bool, String> {
        let request_id = self.state.records.iter().find_map(|(request_id, record)| {
            (record.operation_id == operation_id && record.state == DurableTurnState::Running)
                .then_some(request_id.clone())
        });
        let Some(request_id) = request_id else { return Ok(false); };
        let record = self.state.records.get(&request_id).cloned().expect("found");
        self.settle(
            &request_id,
            operation_id,
            &record.account_fence,
            record.generation,
            DurableTurnState::Cancelled,
            Some(reason.into()),
            now_ms,
        )?;
        Ok(true)
    }

    pub fn mark_outcome_unknown(
        &mut self,
        request_id: &str,
        operation_id: &str,
        account_fence: &str,
        generation: u64,
        reason: impl Into<String>,
        now_ms: u64,
    ) -> Result<(), String> {
        self.assert_current(request_id, operation_id, account_fence, generation)?;
        let record = self.state.records.get_mut(request_id).expect("asserted record");
        record.state = DurableTurnState::OutcomeUnknown;
        record.reason = Some(reason.into());
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn mark_agents_outcome_unknown(
        &mut self,
        account_fence: &str,
        agent_ids: &std::collections::BTreeSet<String>,
        reason: &str,
        now_ms: u64,
    ) -> Result<Vec<DurableTurnRecord>, String> {
        let mut fenced = Vec::new();
        for record in self.state.records.values_mut() {
            if record.state == DurableTurnState::Running
                && record.account_fence == account_fence
                && record
                    .agent_id
                    .as_ref()
                    .is_some_and(|agent_id| agent_ids.contains(agent_id))
            {
                record.state = DurableTurnState::OutcomeUnknown;
                record.reason = Some(reason.to_string());
                record.updated_at_ms = now_ms;
                fenced.push(record.clone());
            }
        }
        if !fenced.is_empty() {
            self.persist()?;
        }
        Ok(fenced)
    }

    pub fn mark_account_outcome_unknown(
        &mut self,
        account_fence: &str,
        reason: &str,
        now_ms: u64,
    ) -> Result<Vec<DurableTurnRecord>, String> {
        let mut fenced = Vec::new();
        for record in self.state.records.values_mut() {
            if record.state == DurableTurnState::Running
                && record.account_fence == account_fence
            {
                record.state = DurableTurnState::OutcomeUnknown;
                record.reason = Some(reason.to_string());
                record.updated_at_ms = now_ms;
                fenced.push(record.clone());
            }
        }
        if !fenced.is_empty() {
            self.persist()?;
        }
        Ok(fenced)
    }

    pub fn reconcile_outcome_unknown(
        &mut self,
        request_id: &str,
        account_fence: &str,
        state: DurableTurnState,
        reason: Option<String>,
        now_ms: u64,
    ) -> Result<DurableTurnRecord, String> {
        if matches!(state, DurableTurnState::Running | DurableTurnState::OutcomeUnknown) {
            return Err("reconciliation requires a proven terminal state".into());
        }
        let record = self
            .state
            .records
            .get_mut(request_id)
            .ok_or("durable turn record is missing")?;
        if record.account_fence != account_fence {
            return Err("outcome-unknown reconciliation is fenced by account identity".into());
        }
        if record.state != DurableTurnState::OutcomeUnknown {
            return Err(format!(
                "durable turn is {:?}, not outcome-unknown",
                record.state
            ));
        }
        record.state = state;
        record.reason = reason;
        record.updated_at_ms = now_ms;
        let reconciled = record.clone();
        self.persist()?;
        Ok(reconciled)
    }

    pub fn record(&self, request_id: &str) -> Option<&DurableTurnRecord> {
        self.state.records.get(request_id)
    }

    pub fn record_by_operation(&self, operation_id: &str) -> Option<&DurableTurnRecord> {
        self.state
            .records
            .values()
            .find(|record| record.operation_id == operation_id)
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(&self.state).map_err(|e| e.to_string())?;
        let result = (|| -> Result<(), String> {
            let mut file = fs::File::create(&tmp).map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            fs::rename(&tmp, &self.path).map_err(|e| e.to_string())?;
            if let Some(parent) = self.path.parent() {
                fs::File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_lookup_returns_canonical_running_turn_context() {
        let dir = tempfile::tempdir().unwrap();
        let mut journal = DurableTurnJournal::open(dir.path().join("turns.json"), 1).unwrap();
        journal
            .begin_with_context(
                "request-a",
                "operation-a",
                "acct:a",
                Some("turn"),
                Some("agent-a"),
                2,
            )
            .unwrap();

        let record = journal
            .record_by_operation("operation-a")
            .expect("operation lookup must recover canonical turn context");
        assert_eq!(record.request_id, "request-a");
        assert_eq!(record.agent_id.as_deref(), Some("agent-a"));
        assert_eq!(record.state, DurableTurnState::Running);
        assert!(journal.record_by_operation("missing-operation").is_none());
    }

    #[test]
    fn request_source_is_durable_when_owned_by_the_turn_journal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("turns.json");
        let mut journal = DurableTurnJournal::open(&path, 1).unwrap();
        journal
            .begin_with_source("r1", "o1", "acct:a", Some("automation"), 2)
            .unwrap();
        drop(journal);

        let reopened = DurableTurnJournal::open(&path, 3).unwrap();
        let record = reopened.record("r1").unwrap();
        assert_eq!(record.request_source.as_deref(), Some("automation"));
        assert_eq!(record.state, DurableTurnState::OutcomeUnknown);
    }

    #[test]
    fn agent_identity_is_durable_and_delete_fencing_only_targets_matching_running_turns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("turns.json");
        let mut journal = DurableTurnJournal::open(&path, 1).unwrap();
        journal
            .begin_with_context(
                "r-a",
                "o-a",
                "acct:a",
                Some("turn"),
                Some("agent-a"),
                2,
            )
            .unwrap();
        journal
            .begin_with_context(
                "r-b",
                "o-b",
                "acct:a",
                Some("turn"),
                Some("agent-b"),
                3,
            )
            .unwrap();
        let targets = std::collections::BTreeSet::from(["agent-a".to_string()]);
        let fenced = journal
            .mark_agents_outcome_unknown("acct:a", &targets, "agent deleted", 4)
            .unwrap();
        assert_eq!(fenced.len(), 1);
        assert_eq!(fenced[0].agent_id.as_deref(), Some("agent-a"));
        assert_eq!(
            journal.record("r-a").unwrap().state,
            DurableTurnState::OutcomeUnknown
        );
        assert_eq!(journal.record("r-b").unwrap().state, DurableTurnState::Running);
        drop(journal);

        let reopened = DurableTurnJournal::open(&path, 5).unwrap();
        assert_eq!(reopened.record("r-a").unwrap().agent_id.as_deref(), Some("agent-a"));
        assert_eq!(reopened.record("r-b").unwrap().agent_id.as_deref(), Some("agent-b"));
    }

    #[test]
    fn duplicate_account_generation_and_terminal_races_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let mut journal = DurableTurnJournal::open(dir.path().join("turns.json"), 1).unwrap();
        let generation = journal.begin("r1", "o1", "acct:a", 2).unwrap();
        assert!(journal.begin("r1", "o2", "acct:a", 3).is_err());
        assert!(journal.assert_current("r1", "o1", "acct:b", generation).is_err());
        assert!(journal.assert_current("r1", "o1", "acct:a", generation + 1).is_err());
        journal
            .settle(
                "r1",
                "o1",
                "acct:a",
                generation,
                DurableTurnState::Completed,
                None,
                4,
            )
            .unwrap();
        assert!(journal
            .settle(
                "r1",
                "o1",
                "acct:a",
                generation,
                DurableTurnState::Cancelled,
                None,
                5,
            )
            .is_err());
    }

    #[test]
    fn checkpoint_metadata_is_durable_before_terminal_settlement() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("turns.json");
        let mut journal = DurableTurnJournal::open(&path, 1).unwrap();
        let generation = journal.begin("r1", "o1", "acct:a", 2).unwrap();
        journal
            .persist_checkpoint_metadata("r1", "o1", "acct:a", generation, 3, 7, 4)
            .unwrap();
        assert_eq!(journal.record("r1").unwrap().checkpoint_attempt, Some(3));
        assert_eq!(journal.record("r1").unwrap().checkpoint_emitted_chunks, Some(7));
        drop(journal);

        let recovered = DurableTurnJournal::open(&path, 5).unwrap();
        let record = recovered.record("r1").unwrap();
        assert_eq!(record.state, DurableTurnState::OutcomeUnknown);
        assert_eq!(record.checkpoint_attempt, Some(3));
        assert_eq!(record.checkpoint_emitted_chunks, Some(7));
        assert_eq!(record.checkpointed_at_ms, Some(4));
    }

    #[test]
    fn terminal_state_reopens_after_atomic_directory_synced_replace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("turns.json");
        let mut journal = DurableTurnJournal::open(&path, 1).unwrap();
        let generation = journal.begin("r1", "o1", "acct:a", 2).unwrap();
        journal
            .settle(
                "r1",
                "o1",
                "acct:a",
                generation,
                DurableTurnState::Completed,
                Some("durable".into()),
                3,
            )
            .unwrap();
        drop(journal);
        let reopened = DurableTurnJournal::open(&path, 4).unwrap();
        assert_eq!(
            reopened.record("r1").unwrap().state,
            DurableTurnState::Completed
        );
        assert_eq!(
            reopened.record("r1").unwrap().reason.as_deref(),
            Some("durable")
        );
    }

    #[test]
    fn process_restart_marks_inflight_as_outcome_unknown_and_forbids_blind_replay() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("turns.json");
        let mut journal = DurableTurnJournal::open(&path, 1).unwrap();
        journal.begin("r1", "o1", "acct:a", 2).unwrap();
        drop(journal);
        let mut recovered = DurableTurnJournal::open(&path, 3).unwrap();
        assert_eq!(
            recovered.record("r1").unwrap().state,
            DurableTurnState::OutcomeUnknown
        );
        assert!(recovered.begin("r1", "o2", "acct:a", 4).is_err());
    }

    #[test]
    fn cancellation_persists_and_duplicate_cancel_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("turns.json");
        let mut journal = DurableTurnJournal::open(&path, 1).unwrap();
        journal.begin("r1", "o1", "acct:a", 2).unwrap();
        assert!(journal
            .settle_operation_cancelled("o1", "user", 3)
            .unwrap());
        assert!(!journal
            .settle_operation_cancelled("o1", "duplicate", 4)
            .unwrap());
        drop(journal);
        let journal = DurableTurnJournal::open(&path, 5).unwrap();
        assert_eq!(
            journal.record("r1").unwrap().state,
            DurableTurnState::Cancelled
        );
    }

    #[test]
    fn account_switch_fences_running_turn_before_stale_callback_can_settle() {
        let dir = tempfile::tempdir().unwrap();
        let mut journal = DurableTurnJournal::open(dir.path().join("turns.json"), 1).unwrap();
        let generation = journal.begin("r1", "o1", "acct:a", 2).unwrap();
        let fenced = journal
            .mark_account_outcome_unknown("acct:a", "account switched", 3)
            .unwrap();
        assert_eq!(fenced.len(), 1);
        assert_eq!(fenced[0].operation_id, "o1");
        assert!(journal
            .assert_current("r1", "o1", "acct:a", generation)
            .is_err());
        assert!(journal
            .settle(
                "r1",
                "o1",
                "acct:a",
                generation,
                DurableTurnState::Completed,
                None,
                4,
            )
            .is_err());
    }

    #[test]
    fn outcome_unknown_reconciliation_is_account_fenced_and_terminal_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("turns.json");
        let mut journal = DurableTurnJournal::open(&path, 1).unwrap();
        journal.begin("r1", "o1", "acct:a", 2).unwrap();
        drop(journal);
        let mut journal = DurableTurnJournal::open(&path, 3).unwrap();
        assert!(journal
            .reconcile_outcome_unknown(
                "r1",
                "acct:b",
                DurableTurnState::Completed,
                None,
                4,
            )
            .is_err());
        assert!(journal
            .reconcile_outcome_unknown(
                "r1",
                "acct:a",
                DurableTurnState::OutcomeUnknown,
                None,
                5,
            )
            .is_err());
        let reconciled = journal
            .reconcile_outcome_unknown(
                "r1",
                "acct:a",
                DurableTurnState::Completed,
                Some("assistant transcript proven durable".into()),
                6,
            )
            .unwrap();
        assert_eq!(reconciled.state, DurableTurnState::Completed);
        assert_eq!(reconciled.operation_id, "o1");
    }
}
