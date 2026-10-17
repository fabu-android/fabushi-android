use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::runner::sand_auto_review::{SandAutoReviewApproval, SandAutoReviewEvent};

pub const SAND_AUTO_REVIEW_AWAITING_TAB_ID: &str = "auto-review";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandAutoReviewAwaitingState {
    pub tab_id: String,
    pub reason: String,
    pub since: u64,
}

pub trait SandAutoReviewAwaitingSink {
    fn try_set_for_tab(
        &self,
        agent_id: &str,
        state: SandAutoReviewAwaitingState,
    );

    fn clear_for_tab(
        &self,
        agent_id: &str,
        tab_id: &str,
    );
}

pub fn build_sand_auto_review_awaiting_reason(summary: &str) -> String {
    format!("Approval needed: {summary}")
}

pub struct SandAutoReviewAwaitingBridge<S> {
    sink: S,
    pending_by_agent: HashMap<String, Vec<(String, String)>>,
    now: Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl<S> SandAutoReviewAwaitingBridge<S>
where
    S: SandAutoReviewAwaitingSink,
{
    pub fn new(sink: S) -> Self {
        Self::with_now(sink, Arc::new(now_ms))
    }

    pub fn with_now(
        sink: S,
        now: Arc<dyn Fn() -> u64 + Send + Sync>,
    ) -> Self {
        Self {
            sink,
            pending_by_agent: HashMap::new(),
            now,
        }
    }

    pub fn handle_event(&mut self, event: &SandAutoReviewEvent) {
        match event {
            SandAutoReviewEvent::Created(approval) => {
                self.handle_created(approval);
            }
            SandAutoReviewEvent::Resolved(approval)
            | SandAutoReviewEvent::Expired { approval, .. } => {
                self.handle_finished(approval);
            }
        }
    }

    pub fn pending_awaiting_state(
        &self,
        agent_id: &str,
    ) -> Option<SandAutoReviewAwaitingState> {
        let reason = self
            .pending_by_agent
            .get(agent_id)?
            .last()?
            .1
            .clone();

        Some(SandAutoReviewAwaitingState {
            tab_id: SAND_AUTO_REVIEW_AWAITING_TAB_ID.into(),
            reason,
            since: (self.now)(),
        })
    }

    pub fn pending_count(&self, agent_id: &str) -> usize {
        self.pending_by_agent
            .get(agent_id)
            .map(Vec::len)
            .unwrap_or_default()
    }

    fn handle_created(&mut self, approval: &SandAutoReviewApproval) {
        let pending = self
            .pending_by_agent
            .entry(approval.agent_id.clone())
            .or_default();
        let reason = build_sand_auto_review_awaiting_reason(&approval.summary);

        if let Some((_, existing_reason)) =
            pending.iter_mut().find(|(id, _)| id == &approval.id)
        {
            *existing_reason = reason;
        } else {
            pending.push((approval.id.clone(), reason));
        }

        self.assert_agent(&approval.agent_id);
    }

    fn handle_finished(&mut self, approval: &SandAutoReviewApproval) {
        let Some(pending) = self.pending_by_agent.get_mut(&approval.agent_id)
        else {
            return;
        };

        let original_len = pending.len();
        pending.retain(|(id, _)| id != &approval.id);
        if pending.len() == original_len {
            return;
        }

        if pending.is_empty() {
            self.pending_by_agent.remove(&approval.agent_id);
            self.sink.clear_for_tab(
                &approval.agent_id,
                SAND_AUTO_REVIEW_AWAITING_TAB_ID,
            );
            return;
        }

        self.assert_agent(&approval.agent_id);
    }

    fn assert_agent(&self, agent_id: &str) {
        let Some(reason) = self
            .pending_by_agent
            .get(agent_id)
            .and_then(|pending| pending.last())
            .map(|(_, reason)| reason.clone())
        else {
            return;
        };

        self.sink.try_set_for_tab(
            agent_id,
            SandAutoReviewAwaitingState {
                tab_id: SAND_AUTO_REVIEW_AWAITING_TAB_ID.into(),
                reason,
                since: (self.now)(),
            },
        );
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
