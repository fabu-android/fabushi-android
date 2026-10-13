use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

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
pub struct AckObligations {
    store: SandAckObligationStore,
    redrive_schedules: Mutex<HashMap<(String, String), AckRedriveSchedule>>,
}

impl AckObligations {
    pub fn new(root_dir: impl AsRef<Path>) -> Self {
        Self {
            store: SandAckObligationStore::new(root_dir),
            redrive_schedules: Mutex::new(HashMap::new()),
        }
    }

    pub fn record_send(&self, account_fence: &str, agent_id: &str, at_ms: f64) -> io::Result<RecordSendOutcome> {
        self.clear_redrive_timer(account_fence, agent_id);
        self.store.record_send(account_fence, agent_id, at_ms)
    }

    pub fn fulfill(&self, account_fence: &str, agent_id: &str) -> io::Result<bool> {
        self.clear_redrive_timer(account_fence, agent_id);
        self.store.clear(account_fence, agent_id)
    }

    pub fn record_interrupt(&self, account_fence: &str, agent_id: &str, at_ms: f64) -> io::Result<bool> {
        self.store.record_interrupt(account_fence, agent_id, at_ms)
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

    pub fn clear_account(&self, account_fence: &str) -> io::Result<usize> {
        if let Ok(mut schedules) = self.redrive_schedules.lock() {
            schedules.retain(|(fence, _), _| fence != account_fence);
        }
        self.store.clear_account(account_fence)
    }

    pub fn clear_agent(&self, account_fence: &str, agent_id: &str) -> io::Result<bool> {
        self.clear_redrive_timer(account_fence, agent_id);
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
        assert!(reopened.fulfill("acct-a", "agent-a").unwrap());
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
}
