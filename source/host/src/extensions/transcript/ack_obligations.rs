use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};
use uuid::Uuid;

use super::sand_ack_obligation_store::{AckObligation, RecordSendOutcome, SandAckObligationStore};

pub const MAX_ACK_REDRIVES: u64 = 3;
pub const ACK_REDRIVE_IDLE_DELAY_MS: u64 = 5_000;

pub fn build_ack_redrive_prompt() -> &'static str {
    "[System recovery] The user sent one or more messages that were never visibly acknowledged. Respond now with a real user-visible SendMessage. If the latest request is unavailable, say it may have been missed and ask the user to resend it. Never invent completion."
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckRedriveTrigger { Boot, Idle }

impl AckRedriveTrigger {
    pub fn as_str(self) -> &'static str {
        match self { Self::Boot => "boot", Self::Idle => "idle" }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AckRedriveSchedule {
    pub trigger: AckRedriveTrigger,
    pub due_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AckRedrivePreparation {
    Missing,
    Ready(AckObligation),
    LostAgentDeleted(AckObligation),
    LostMaxRedrives(AckObligation),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AckReservation {
    pub ack_token: String,
    pub obligation: AckObligation,
    pub created: bool,
}

#[derive(Debug, Clone)]
struct ReservationState {
    account_fence: String,
    agent_id: String,
    previous: Option<AckObligation>,
    recorded: AckObligation,
}

pub fn build_ack_redrive_send_args(
    agent_id: &str,
    obligation: &AckObligation,
    trigger: AckRedriveTrigger,
    now_ms: u64,
) -> Value {
    json!({
        "type": "chat.send",
        "requestId": format!("ack-redrive:{agent_id}:{}:{now_ms}", obligation.redrive_attempts.max(0.0).floor() as u64),
        "agentId": agent_id,
        "text": build_ack_redrive_prompt(),
        "appendUserMessage": false,
        "hidden": true,
        "awaitTurn": false,
        "skipAckObligation": true,
        "ackRedrive": true,
        "ackRedriveTrigger": trigger.as_str(),
        "redriveAttempts": obligation.redrive_attempts,
        "requestSource": "handoff-resume",
    })
}

/// Durable account-fenced obligation to eventually produce a visible Agent acknowledgement.
///
/// This is intentionally separate from DurableSendAcknowledgementOwner. That owner proves the
/// user's outgoing message reached canonical transcript storage. This owner begins after that
/// durable user echo exists and ends only after a visible Agent message is durably appended.
pub struct AckSendGuard {
    obligations: Arc<AckObligations>,
    account_fence: String,
    agent_id: String,
    accepted_at_ms: f64,
    armed: bool,
}

impl AckSendGuard {
    pub fn disarm(&mut self) {
        self.armed = false;
    }

    pub fn is_armed(&self) -> bool {
        self.armed
    }
}

impl Drop for AckSendGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let _ = self.obligations.ensure_send_obligation_if_missing(
            &self.account_fence,
            &self.agent_id,
            self.accepted_at_ms,
        );
    }
}

pub struct AckObligations {
    store: SandAckObligationStore,
    mutation_guard: Mutex<()>,
    reservations: Mutex<HashMap<String, ReservationState>>,
    redrive_schedules: Mutex<HashMap<(String, String), AckRedriveSchedule>>,
}

impl AckObligations {
    pub fn new(root_dir: impl AsRef<Path>) -> Self {
        Self {
            store: SandAckObligationStore::new(root_dir),
            mutation_guard: Mutex::new(()),
            reservations: Mutex::new(HashMap::new()),
            redrive_schedules: Mutex::new(HashMap::new()),
        }
    }

    /// Arm only after the canonical user transcript echo is durable.
    ///
    /// This closes the process-death/error window between transcript persistence and
    /// acknowledgement-obligation persistence. If the normal record_send call succeeds,
    /// callers disarm the guard. If it fails or unwinds, Drop recreates the missing
    /// obligation best-effort without coalescing an already-persisted obligation.
    pub fn arm_send_guard(
        self: &Arc<Self>,
        account_fence: &str,
        agent_id: &str,
        accepted_at_ms: f64,
        owes_ack: bool,
    ) -> AckSendGuard {
        AckSendGuard {
            obligations: Arc::clone(self),
            account_fence: account_fence.to_string(),
            agent_id: agent_id.to_string(),
            accepted_at_ms,
            armed: owes_ack,
        }
    }

    fn ensure_send_obligation_if_missing(
        &self,
        account_fence: &str,
        agent_id: &str,
        at_ms: f64,
    ) -> io::Result<bool> {
        let _mutation = self
            .mutation_guard
            .lock()
            .map_err(|_| io::Error::other("ack obligation mutation lock poisoned"))?;
        if self.store.get(account_fence, agent_id).is_some() {
            return Ok(false);
        }
        self.clear_redrive_timer(account_fence, agent_id);
        self.store
            .record_send(account_fence, agent_id, at_ms)
            .map(|_| true)
    }

    /// Recover a missing acknowledgement obligation from canonical durable transcript state.
    ///
    /// Only current-format visible user sends carry all four acceptance fields below. Older
    /// transcript shapes, hidden internal prompts, foreign-account entries, and already-settled
    /// turns are intentionally ignored. Existing obligations are never incremented here: boot
    /// reconciliation repairs a torn two-store commit, it does not replay the original send.
    pub fn reconcile_from_transcript(
        &self,
        account_fence: &str,
        entries: &[Value],
    ) -> io::Result<usize> {
        let account_fence = account_fence.trim();
        if account_fence.is_empty() {
            return Ok(0);
        }

        let assistant_ids = entries
            .iter()
            .filter(|entry| {
                entry.get("kind").and_then(Value::as_str) == Some("message")
                    && entry.get("role").and_then(Value::as_str) == Some("assistant")
            })
            .filter_map(|entry| entry.get("id").and_then(Value::as_str))
            .collect::<std::collections::HashSet<_>>();

        let mut candidates = HashMap::<String, f64>::new();
        for entry in entries {
            if entry.get("kind").and_then(Value::as_str) != Some("message")
                || entry.get("role").and_then(Value::as_str) != Some("user")
                || entry.get("accountFence").and_then(Value::as_str) != Some(account_fence)
            {
                continue;
            }
            let Some(agent_id) = entry
                .get("agentId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let Some(operation_id) = entry
                .get("operationId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let has_client_nonce = entry
                .get("clientNonce")
                .and_then(Value::as_str)
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());
            if !has_client_nonce
                || assistant_ids.contains(format!("assistant:{operation_id}").as_str())
            {
                continue;
            }
            let accepted_at_ms = entry
                .get("timestampMs")
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .unwrap_or(0.0);
            candidates
                .entry(agent_id.to_string())
                .and_modify(|current| *current = current.max(accepted_at_ms))
                .or_insert(accepted_at_ms);
        }

        let mut repaired = 0usize;
        for (agent_id, accepted_at_ms) in candidates {
            if self.ensure_send_obligation_if_missing(
                account_fence,
                &agent_id,
                accepted_at_ms,
            )? {
                repaired = repaired.saturating_add(1);
            }
        }
        Ok(repaired)
    }

    pub fn record_send(&self, account_fence: &str, agent_id: &str, at_ms: f64) -> io::Result<RecordSendOutcome> {
        let _mutation = self
            .mutation_guard
            .lock()
            .map_err(|_| io::Error::other("ack obligation mutation lock poisoned"))?;
        self.clear_redrive_timer(account_fence, agent_id);
        self.store.record_send(account_fence, agent_id, at_ms)
    }

    pub fn record_send_and_mint_token(
        &self,
        account_fence: &str,
        agent_id: &str,
        at_ms: f64,
    ) -> io::Result<AckReservation> {
        let _mutation = self
            .mutation_guard
            .lock()
            .map_err(|_| io::Error::other("ack obligation mutation lock poisoned"))?;
        self.clear_redrive_timer(account_fence, agent_id);
        let previous = self.store.get(account_fence, agent_id);
        let outcome = self.store.record_send(account_fence, agent_id, at_ms)?;
        let ack_token = Uuid::new_v4().to_string();
        self.reservations
            .lock()
            .map_err(|_| io::Error::other("ack reservation registry poisoned"))?
            .insert(
                ack_token.clone(),
                ReservationState {
                    account_fence: account_fence.to_string(),
                    agent_id: agent_id.to_string(),
                    previous,
                    recorded: outcome.obligation.clone(),
                },
            );
        Ok(AckReservation {
            ack_token,
            obligation: outcome.obligation,
            created: outcome.created,
        })
    }

    pub fn mint_ack_run_token(
        &self,
        account_fence: &str,
        agent_id: &str,
    ) -> io::Result<Option<String>> {
        let _mutation = self
            .mutation_guard
            .lock()
            .map_err(|_| io::Error::other("ack obligation mutation lock poisoned"))?;
        let Some(recorded) = self.store.get(account_fence, agent_id) else {
            return Ok(None);
        };
        let ack_token = Uuid::new_v4().to_string();
        self.reservations
            .lock()
            .map_err(|_| io::Error::other("ack reservation registry poisoned"))?
            .insert(
                ack_token.clone(),
                ReservationState {
                    account_fence: account_fence.to_string(),
                    agent_id: agent_id.to_string(),
                    previous: Some(recorded.clone()),
                    recorded,
                },
            );
        Ok(Some(ack_token))
    }

    /// Fulfill only the exact obligation snapshot reserved for this turn.
    /// A later coalesced user send changes the durable row, so an older turn
    /// cannot accidentally clear the newer reply obligation.
    pub fn fulfill_ack_obligation(
        &self,
        account_fence: &str,
        agent_id: &str,
        ack_token: &str,
    ) -> io::Result<bool> {
        let _mutation = self
            .mutation_guard
            .lock()
            .map_err(|_| io::Error::other("ack obligation mutation lock poisoned"))?;
        let reservation = {
            let mut reservations = self
                .reservations
                .lock()
                .map_err(|_| io::Error::other("ack reservation registry poisoned"))?;
            let Some(reservation) = reservations.get(ack_token) else {
                return Ok(false);
            };
            if reservation.account_fence != account_fence || reservation.agent_id != agent_id {
                return Ok(false);
            }
            reservations.remove(ack_token).expect("reservation existed")
        };
        if self.store.get(account_fence, agent_id).as_ref() != Some(&reservation.recorded) {
            return Ok(false);
        }
        self.clear_redrive_timer(account_fence, agent_id);
        self.store.clear(account_fence, agent_id)
    }

    pub fn rollback_ack_reservation(
        &self,
        account_fence: &str,
        agent_id: &str,
        ack_token: &str,
    ) -> io::Result<bool> {
        let _mutation = self
            .mutation_guard
            .lock()
            .map_err(|_| io::Error::other("ack obligation mutation lock poisoned"))?;
        let reservation = {
            let mut reservations = self
                .reservations
                .lock()
                .map_err(|_| io::Error::other("ack reservation registry poisoned"))?;
            let Some(reservation) = reservations.get(ack_token) else {
                return Ok(false);
            };
            if reservation.account_fence != account_fence || reservation.agent_id != agent_id {
                return Ok(false);
            }
            reservations.remove(ack_token).expect("reservation existed")
        };
        if self.store.get(account_fence, agent_id).as_ref() != Some(&reservation.recorded) {
            return Ok(false);
        }
        self.store.restore(reservation.previous, account_fence, agent_id)?;
        Ok(true)
    }

    pub fn retire_ack_run_token(
        &self,
        account_fence: &str,
        agent_id: &str,
        ack_token: Option<&str>,
    ) -> bool {
        let Some(ack_token) = ack_token else {
            return false;
        };
        let Ok(mut reservations) = self.reservations.lock() else {
            return false;
        };
        if reservations.get(ack_token).is_some_and(|reservation| {
            reservation.account_fence == account_fence && reservation.agent_id == agent_id
        }) {
            reservations.remove(ack_token);
            true
        } else {
            false
        }
    }

    /// Reserved for externally reconciled transcript proof. Live turn completion
    /// must use fulfill_ack_obligation with the turn's token.
    pub fn fulfill_reconciled(&self, account_fence: &str, agent_id: &str) -> io::Result<bool> {
        let _mutation = self
            .mutation_guard
            .lock()
            .map_err(|_| io::Error::other("ack obligation mutation lock poisoned"))?;
        self.clear_redrive_timer(account_fence, agent_id);
        self.store.clear(account_fence, agent_id)
    }

    pub fn record_interrupt(&self, account_fence: &str, agent_id: &str, at_ms: f64) -> io::Result<bool> {
        let _mutation = self
            .mutation_guard
            .lock()
            .map_err(|_| io::Error::other("ack obligation mutation lock poisoned"))?;
        self.store.record_interrupt(account_fence, agent_id, at_ms)
    }

    pub fn defer_unfulfilled_completion(
        &self,
        account_fence: &str,
        agent_id: &str,
        now_ms: u64,
    ) -> io::Result<bool> {
        let pending = self.record_interrupt(account_fence, agent_id, now_ms as f64)?;
        if pending {
            self.schedule_after_idle(account_fence, agent_id, now_ms);
        }
        Ok(pending)
    }

    pub fn defer_unfulfilled_for_token(
        &self,
        account_fence: &str,
        agent_id: &str,
        ack_token: &str,
        now_ms: u64,
    ) -> io::Result<bool> {
        let _mutation = self
            .mutation_guard
            .lock()
            .map_err(|_| io::Error::other("ack obligation mutation lock poisoned"))?;
        let reservation = {
            let mut reservations = self
                .reservations
                .lock()
                .map_err(|_| io::Error::other("ack reservation registry poisoned"))?;
            let Some(reservation) = reservations.get(ack_token) else {
                return Ok(false);
            };
            if reservation.account_fence != account_fence || reservation.agent_id != agent_id {
                return Ok(false);
            }
            reservations.remove(ack_token).expect("reservation existed")
        };
        if self.store.get(account_fence, agent_id).as_ref() != Some(&reservation.recorded) {
            return Ok(false);
        }
        let pending = self
            .store
            .record_interrupt(account_fence, agent_id, now_ms as f64)?;
        if pending {
            self.schedule_after_idle(account_fence, agent_id, now_ms);
        }
        Ok(pending)
    }

    pub fn pending_for_account(&self, account_fence: &str) -> Vec<AckObligation> {
        self.store.list_for_account(account_fence)
    }

    pub fn schedule_after_idle(&self, account_fence: &str, agent_id: &str, now_ms: u64) -> bool {
        if self.store.get(account_fence, agent_id).is_none() { return false; }
        let Ok(mut schedules) = self.redrive_schedules.lock() else { return false; };
        let key = (account_fence.to_string(), agent_id.to_string());
        if schedules.contains_key(&key) { return false; }
        schedules.insert(key, AckRedriveSchedule {
            trigger: AckRedriveTrigger::Idle,
            due_at_ms: now_ms.saturating_add(ACK_REDRIVE_IDLE_DELAY_MS),
        });
        true
    }

    pub fn arm_boot_redrives(&self, account_fence: &str, now_ms: u64) -> usize {
        let mut armed = 0usize;
        for obligation in self.store.list_for_account(account_fence) {
            let Ok(mut schedules) = self.redrive_schedules.lock() else { break; };
            let key = (account_fence.to_string(), obligation.agent_id.clone());
            if schedules.insert(key, AckRedriveSchedule {
                trigger: AckRedriveTrigger::Boot,
                due_at_ms: now_ms.saturating_add(ACK_REDRIVE_IDLE_DELAY_MS),
            }).is_none() { armed = armed.saturating_add(1); }
        }
        armed
    }

    pub fn take_due_redrive(&self, account_fence: &str, agent_id: &str, now_ms: u64) -> Option<AckRedriveTrigger> {
        let mut schedules = self.redrive_schedules.lock().ok()?;
        let key = (account_fence.to_string(), agent_id.to_string());
        let schedule = schedules.get(&key).copied()?;
        if schedule.due_at_ms > now_ms { return None; }
        schedules.remove(&key);
        Some(schedule.trigger)
    }

    pub fn prepare_redrive(&self, account_fence: &str, agent_id: &str, agent_exists: bool) -> io::Result<AckRedrivePreparation> {
        let _mutation = self
            .mutation_guard
            .lock()
            .map_err(|_| io::Error::other("ack obligation mutation lock poisoned"))?;
        let Some(obligation) = self.store.get(account_fence, agent_id) else {
            return Ok(AckRedrivePreparation::Missing);
        };
        if !agent_exists {
            self.store.clear(account_fence, agent_id)?;
            return Ok(AckRedrivePreparation::LostAgentDeleted(obligation));
        }
        if obligation.redrive_attempts >= MAX_ACK_REDRIVES as f64 {
            self.store.clear(account_fence, agent_id)?;
            return Ok(AckRedrivePreparation::LostMaxRedrives(obligation));
        }
        Ok(match self.store.record_redrive_attempt(account_fence, agent_id)? {
            Some(next) => AckRedrivePreparation::Ready(next),
            None => AckRedrivePreparation::Missing,
        })
    }

    /// Restore a redrive budget only if no newer durable mutation replaced the prepared row.
    pub fn rollback_redrive_attempt_if_unchanged(
        &self,
        account_fence: &str,
        agent_id: &str,
        previous: &AckObligation,
        prepared: &AckObligation,
    ) -> io::Result<bool> {
        let _mutation = self
            .mutation_guard
            .lock()
            .map_err(|_| io::Error::other("ack obligation mutation lock poisoned"))?;
        if previous.account_fence != account_fence
            || previous.agent_id != agent_id
            || prepared.account_fence != account_fence
            || prepared.agent_id != agent_id
        {
            return Ok(false);
        }
        if self.store.get(account_fence, agent_id).as_ref() != Some(prepared) {
            return Ok(false);
        }
        self.store
            .restore(Some(previous.clone()), account_fence, agent_id)?;
        Ok(true)
    }

    pub fn clear_account(&self, account_fence: &str) -> io::Result<usize> {
        let _mutation = self
            .mutation_guard
            .lock()
            .map_err(|_| io::Error::other("ack obligation mutation lock poisoned"))?;
        if let Ok(mut schedules) = self.redrive_schedules.lock() {
            schedules.retain(|(fence, _), _| fence != account_fence);
        }
        if let Ok(mut reservations) = self.reservations.lock() {
            reservations.retain(|_, reservation| reservation.account_fence != account_fence);
        }
        self.store.clear_account(account_fence)
    }

    pub fn clear_agent(&self, account_fence: &str, agent_id: &str) -> io::Result<bool> {
        let _mutation = self
            .mutation_guard
            .lock()
            .map_err(|_| io::Error::other("ack obligation mutation lock poisoned"))?;
        self.clear_redrive_timer(account_fence, agent_id);
        if let Ok(mut reservations) = self.reservations.lock() {
            reservations.retain(|_, reservation| {
                reservation.account_fence != account_fence || reservation.agent_id != agent_id
            });
        }
        self.store.clear(account_fence, agent_id)
    }

    fn clear_redrive_timer(&self, account_fence: &str, agent_id: &str) -> bool {
        self.redrive_schedules.lock().map(|mut schedules| {
            schedules.remove(&(account_fence.to_string(), agent_id.to_string())).is_some()
        }).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_guard_recovers_missing_obligation_after_durable_user_echo() {
        let root = tempfile::tempdir().unwrap();
        let owner = Arc::new(AckObligations::new(root.path()));
        {
            let guard = owner.arm_send_guard("acct-a", "agent-a", 10.0, true);
            assert!(guard.is_armed());
            assert!(owner.store.get("acct-a", "agent-a").is_none());
        }
        let recovered = owner
            .store
            .get("acct-a", "agent-a")
            .expect("armed guard must restore the missing durable obligation");
        assert_eq!(recovered.account_fence, "acct-a");
        assert_eq!(recovered.agent_id, "agent-a");
        assert!(owner.store.get("acct-b", "agent-a").is_none());
    }

    #[test]
    fn send_guard_disarm_or_existing_obligation_never_double_counts() {
        let root = tempfile::tempdir().unwrap();
        let owner = Arc::new(AckObligations::new(root.path()));

        {
            let mut guard = owner.arm_send_guard("acct-a", "agent-a", 10.0, true);
            owner.record_send("acct-a", "agent-a", 10.0).unwrap();
            guard.disarm();
        }
        assert_eq!(
            owner
                .store
                .get("acct-a", "agent-a")
                .expect("normal send must persist obligation")
                .coalesced_count,
            1.0
        );

        {
            let _guard = owner.arm_send_guard("acct-a", "agent-a", 20.0, true);
        }
        assert_eq!(
            owner
                .store
                .get("acct-a", "agent-a")
                .expect("existing obligation must survive guard drop")
                .coalesced_count,
            1.0
        );
    }

    #[test]
    fn boot_reconcile_repairs_only_current_account_unanswered_visible_sends() {
        let root = tempfile::tempdir().unwrap();
        let owner = AckObligations::new(root.path());
        let transcript = vec![
            json!({
                "id":"request-a",
                "kind":"message",
                "role":"user",
                "agentId":"agent-a",
                "operationId":"op-a",
                "accountFence":"acct-a",
                "clientNonce":"nonce-a",
                "timestampMs":10,
            }),
            json!({
                "id":"request-b",
                "kind":"message",
                "role":"user",
                "agentId":"agent-b",
                "operationId":"op-b",
                "accountFence":"acct-a",
                "clientNonce":"nonce-b",
                "timestampMs":20,
            }),
            json!({
                "id":"assistant:op-b",
                "kind":"message",
                "role":"assistant",
                "agentId":"agent-b",
                "operationId":"op-b",
                "content":"done",
                "timestampMs":21,
            }),
            json!({
                "id":"request-old-account",
                "kind":"message",
                "role":"user",
                "agentId":"agent-c",
                "operationId":"op-c",
                "accountFence":"acct-b",
                "clientNonce":"nonce-c",
                "timestampMs":30,
            }),
            json!({
                "id":"legacy-without-nonce",
                "kind":"message",
                "role":"user",
                "agentId":"agent-d",
                "operationId":"op-d",
                "accountFence":"acct-a",
                "timestampMs":40,
            }),
        ];

        assert_eq!(owner.reconcile_from_transcript("acct-a", &transcript).unwrap(), 1);
        let pending = owner.pending_for_account("acct-a");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].agent_id, "agent-a");
        assert_eq!(pending[0].coalesced_count, 1.0);
        assert!(owner.pending_for_account("acct-b").is_empty());
    }

    #[test]
    fn boot_reconcile_never_double_counts_existing_obligation() {
        let root = tempfile::tempdir().unwrap();
        let owner = AckObligations::new(root.path());
        owner.record_send("acct-a", "agent-a", 5.0).unwrap();
        let transcript = vec![json!({
            "id":"request-a",
            "kind":"message",
            "role":"user",
            "agentId":"agent-a",
            "operationId":"op-a",
            "accountFence":"acct-a",
            "clientNonce":"nonce-a",
            "timestampMs":10,
        })];

        assert_eq!(owner.reconcile_from_transcript("acct-a", &transcript).unwrap(), 0);
        let pending = owner.pending_for_account("acct-a");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].coalesced_count, 1.0);
    }

    #[test]
    fn obligation_survives_reopen_and_is_account_fenced() {
        let root = tempfile::tempdir().unwrap();
        let first = AckObligations::new(root.path());
        first.record_send("acct-a", "agent-a", 10.0).unwrap();
        first.record_send("acct-a", "agent-a", 20.0).unwrap();
        first.record_send("acct-b", "agent-a", 30.0).unwrap();
        drop(first);
        let reopened = AckObligations::new(root.path());
        let a = reopened.pending_for_account("acct-a");
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].coalesced_count, 2.0);
        assert_eq!(reopened.pending_for_account("acct-b").len(), 1);
        assert!(reopened.fulfill_reconciled("acct-a", "agent-a").unwrap());
        assert!(reopened.pending_for_account("acct-a").is_empty());
        assert_eq!(reopened.pending_for_account("acct-b").len(), 1);
    }

    #[test]
    fn redrive_is_bounded_and_never_crosses_account_fence() {
        let root = tempfile::tempdir().unwrap();
        let owner = AckObligations::new(root.path());
        owner.record_send("acct-a", "agent-a", 1.0).unwrap();
        assert_eq!(owner.arm_boot_redrives("acct-a", 10), 1);
        assert_eq!(owner.take_due_redrive("acct-a", "agent-a", 4_999), None);
        assert_eq!(owner.take_due_redrive("acct-a", "agent-a", 5_010), Some(AckRedriveTrigger::Boot));
        assert_eq!(owner.take_due_redrive("acct-b", "agent-a", 99_999), None);
        for _ in 0..MAX_ACK_REDRIVES {
            assert!(matches!(owner.prepare_redrive("acct-a", "agent-a", true).unwrap(), AckRedrivePreparation::Ready(_)));
        }
        assert!(matches!(owner.prepare_redrive("acct-a", "agent-a", true).unwrap(), AckRedrivePreparation::LostMaxRedrives(_)));
        assert!(owner.pending_for_account("acct-a").is_empty());
    }

    #[test]
    fn completed_without_visible_delivery_stays_redrivable() {
        let root = tempfile::tempdir().unwrap();
        let owner = AckObligations::new(root.path());
        owner.record_send("acct-a", "agent-a", 1.0).unwrap();
        assert!(owner
            .defer_unfulfilled_completion("acct-a", "agent-a", 2)
            .unwrap());
        assert_eq!(
            owner.take_due_redrive(
                "acct-a",
                "agent-a",
                2 + ACK_REDRIVE_IDLE_DELAY_MS - 1,
            ),
            None,
        );
        assert_eq!(
            owner.take_due_redrive("acct-a", "agent-a", 2 + ACK_REDRIVE_IDLE_DELAY_MS),
            Some(AckRedriveTrigger::Idle),
        );
        assert_eq!(owner.pending_for_account("acct-a").len(), 1);
    }

    #[test]
    fn failed_redrive_dispatch_restores_budget_only_if_prepared_snapshot_is_current() {
        let root = tempfile::tempdir().unwrap();
        let owner = AckObligations::new(root.path());
        let previous = owner
            .record_send("acct-a", "agent-a", 1.0)
            .unwrap()
            .obligation;
        let prepared = match owner
            .prepare_redrive("acct-a", "agent-a", true)
            .unwrap()
        {
            AckRedrivePreparation::Ready(prepared) => prepared,
            other => panic!("expected ready redrive, got {other:?}"),
        };
        assert_eq!(prepared.redrive_attempts, 1.0);
        assert!(
            owner
                .rollback_redrive_attempt_if_unchanged(
                    "acct-a",
                    "agent-a",
                    &previous,
                    &prepared,
                )
                .unwrap()
        );
        assert_eq!(
            owner.pending_for_account("acct-a")[0].redrive_attempts,
            0.0
        );
    }

    #[test]
    fn redrive_rollback_never_overwrites_newer_or_foreign_account_state() {
        let root = tempfile::tempdir().unwrap();
        let owner = AckObligations::new(root.path());
        let previous = owner
            .record_send("acct-a", "agent-a", 1.0)
            .unwrap()
            .obligation;
        let prepared = match owner
            .prepare_redrive("acct-a", "agent-a", true)
            .unwrap()
        {
            AckRedrivePreparation::Ready(prepared) => prepared,
            other => panic!("expected ready redrive, got {other:?}"),
        };
        owner
            .record_interrupt("acct-a", "agent-a", 2.0)
            .unwrap();
        assert!(
            !owner
                .rollback_redrive_attempt_if_unchanged(
                    "acct-a",
                    "agent-a",
                    &previous,
                    &prepared,
                )
                .unwrap()
        );
        let newer = owner.pending_for_account("acct-a")[0].clone();
        assert_eq!(newer.redrive_attempts, 1.0);
        assert_eq!(newer.last_interrupt_at_ms, Some(2.0));
        assert!(
            !owner
                .rollback_redrive_attempt_if_unchanged(
                    "acct-b",
                    "agent-a",
                    &previous,
                    &prepared,
                )
                .unwrap()
        );
        assert_eq!(owner.pending_for_account("acct-a")[0], newer);
    }

    #[test]
    fn token_defer_cannot_interrupt_a_newer_coalesced_obligation() {
        let root = tempfile::tempdir().unwrap();
        let owner = AckObligations::new(root.path());
        let first = owner
            .record_send_and_mint_token("acct-a", "agent-a", 1.0)
            .unwrap();
        owner.record_send("acct-a", "agent-a", 2.0).unwrap();
        assert!(
            !owner
                .defer_unfulfilled_for_token(
                    "acct-a",
                    "agent-a",
                    &first.ack_token,
                    3,
                )
                .unwrap()
        );
        let current = owner.pending_for_account("acct-a")[0].clone();
        assert_eq!(current.coalesced_count, 2.0);
        assert_eq!(current.last_interrupt_at_ms, None);
    }

    #[test]
    fn token_defer_marks_only_its_exact_snapshot_and_schedules_redrive() {
        let root = tempfile::tempdir().unwrap();
        let owner = AckObligations::new(root.path());
        let reserved = owner
            .record_send_and_mint_token("acct-a", "agent-a", 1.0)
            .unwrap();
        assert!(
            owner
                .defer_unfulfilled_for_token(
                    "acct-a",
                    "agent-a",
                    &reserved.ack_token,
                    10,
                )
                .unwrap()
        );
        let current = owner.pending_for_account("acct-a")[0].clone();
        assert_eq!(current.last_interrupt_at_ms, Some(10.0));
        assert_eq!(
            owner.take_due_redrive("acct-a", "agent-a", 5_010),
            Some(AckRedriveTrigger::Idle)
        );
    }

    #[test]
    fn redrive_never_reappends_user_message() {
        let root = tempfile::tempdir().unwrap();
        let owner = AckObligations::new(root.path());
        let obligation = owner.record_send("acct-a", "agent-a", 1.0).unwrap().obligation;
        let command = build_ack_redrive_send_args("agent-a", &obligation, AckRedriveTrigger::Boot, 9);
        assert_eq!(command["appendUserMessage"], false);
        assert_eq!(command["hidden"], true);
        assert_eq!(command["skipAckObligation"], true);
        assert_eq!(command["ackRedrive"], true);
    }

    #[test]
    fn older_turn_cannot_clear_later_coalesced_obligation() {
        let root = tempfile::tempdir().unwrap();
        let owner = AckObligations::new(root.path());
        let first = owner
            .record_send_and_mint_token("acct-a", "agent-a", 10.0)
            .unwrap();
        let second = owner
            .record_send_and_mint_token("acct-a", "agent-a", 20.0)
            .unwrap();
        assert!(!owner
            .fulfill_ack_obligation("acct-a", "agent-a", &first.ack_token)
            .unwrap());
        assert_eq!(
            owner
                .store
                .get("acct-a", "agent-a")
                .unwrap()
                .coalesced_count,
            2.0
        );
        assert!(owner
            .fulfill_ack_obligation("acct-a", "agent-a", &second.ack_token)
            .unwrap());
        assert!(owner.store.get("acct-a", "agent-a").is_none());
    }

    #[test]
    fn ack_token_never_crosses_account_fence() {
        let root = tempfile::tempdir().unwrap();
        let owner = AckObligations::new(root.path());
        let reservation = owner
            .record_send_and_mint_token("acct-a", "agent-a", 10.0)
            .unwrap();
        assert!(!owner
            .fulfill_ack_obligation("acct-b", "agent-a", &reservation.ack_token)
            .unwrap());
        assert!(owner.store.get("acct-a", "agent-a").is_some());
    }

}
