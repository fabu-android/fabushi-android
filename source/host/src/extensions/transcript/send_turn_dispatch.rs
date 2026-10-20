use super::run_scheduler::{
    QueueAccepted, QueueDequeued, QueueDiagnostics, RunLane, RunScheduler, RunSettlement,
    WatchdogEvent, WatchdogStage,
};

pub fn classify_send_dispatch(
    request_source: Option<&str>,
    ack_redrive: bool,
) -> (RunLane, &'static str) {
    let is_handoff_resume = request_source == Some("handoff-resume");
    let is_automation = request_source == Some("automation");
    let is_upgrade_resume = request_source == Some("upgrade-resume");
    let is_event = request_source == Some("event");
    let is_group_member = request_source == Some("group-member");
    let is_agent_inbound = request_source == Some("agent-inbound");
    let is_human_handoff = request_source == Some("human-handoff");
    let is_kickstart = request_source == Some("kickstart");
    (
        if is_handoff_resume || is_automation || is_upgrade_resume || is_event {
            RunLane::Background
        } else if is_agent_inbound {
            RunLane::Agent
        } else {
            RunLane::User
        },
        if ack_redrive && is_handoff_resume {
            "ack-redrive"
        } else if is_handoff_resume {
            "handoff-resume"
        } else if is_automation {
            "automation"
        } else if is_upgrade_resume {
            "upgrade-resume"
        } else if is_event {
            "event"
        } else if is_agent_inbound {
            "agent-inbound"
        } else if is_human_handoff {
            "human-handoff"
        } else if is_group_member {
            "group-member"
        } else if is_kickstart {
            "kickstart"
        } else {
            "turn"
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserTurnTicket {
    pub agent_id: String,
    pub task_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnWatchdogTick {
    pub event: WatchdogEvent,
    pub started_next: Option<QueueDequeued>,
}

/// Android-adapted port of Desktop ProductionTurnDispatch.
///
/// Prompt shaping/session-worker concerns remain owned by AndroidJsonHost and the typed Android
/// runtime. This owner preserves the platform-independent per-Agent queue, lane priority,
/// generation, watchdog and late-settlement semantics without introducing a second transcript
/// owner.
#[derive(Debug)]
pub struct ProductionTurnDispatch {
    scheduler: RunScheduler,
    next_task_seq: u64,
}

impl Default for ProductionTurnDispatch {
    fn default() -> Self {
        Self {
            scheduler: RunScheduler::default(),
            next_task_seq: 0,
        }
    }
}

impl ProductionTurnDispatch {
    pub fn with_watchdog(watchdog_ms: u64, watchdog_grace_ms: u64) -> Self {
        Self {
            scheduler: RunScheduler::new(watchdog_ms, watchdog_grace_ms),
            next_task_seq: 0,
        }
    }

    pub fn watchdog_wait_ms(&self, agent_id: &str, now_ms: u64) -> Option<u64> {
        self.scheduler.next_watchdog_delay_ms(agent_id, now_ms)
    }

    pub fn watchdog_tick(&mut self, agent_id: &str, now_ms: u64) -> Option<TurnWatchdogTick> {
        let event = self.scheduler.watchdog_tick(agent_id, now_ms)?;
        let started_next = if event.stage == WatchdogStage::Escape {
            self.scheduler.start_next(agent_id, now_ms)
        } else {
            None
        };
        Some(TurnWatchdogTick { event, started_next })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn enqueue_turn_with_start(
        &mut self,
        agent_id: &str,
        client_nonce: Option<&str>,
        accepted_at_ms: u64,
        now_ms: u64,
        lane: RunLane,
        source: &str,
        ack_token: Option<&str>,
    ) -> Result<(UserTurnTicket, QueueAccepted, Option<QueueDequeued>), &'static str> {
        self.next_task_seq = self.next_task_seq.saturating_add(1);
        let nonce = client_nonce
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("no-nonce");
        let task_id = format!("send:{agent_id}:{}:{nonce}", self.next_task_seq);
        let accepted = self.scheduler.enqueue(
            agent_id.to_string(),
            super::run_scheduler::QueuedRun {
                task_id: task_id.clone(),
                lane,
                source: source.to_string(),
                enqueued_at_ms: now_ms,
                accepted_at_ms: Some(accepted_at_ms),
                ack_token: ack_token.map(ToOwned::to_owned),
            },
        )?;
        let started = if self.scheduler.active(agent_id).is_none() {
            self.scheduler.start_next(agent_id, now_ms)
        } else {
            None
        };
        Ok((
            UserTurnTicket {
                agent_id: agent_id.to_string(),
                task_id,
            },
            accepted,
            started,
        ))
    }

    pub fn active_generation_for(&self, ticket: &UserTurnTicket) -> Option<u64> {
        let active = self.scheduler.active(&ticket.agent_id)?;
        (active.item.task_id == ticket.task_id).then_some(active.generation)
    }

    pub fn settle_and_start_next(
        &mut self,
        ticket: &UserTurnTicket,
        generation: u64,
        now_ms: u64,
    ) -> (RunSettlement, Option<QueueDequeued>) {
        let settlement = self.scheduler.settle(&ticket.agent_id, generation, now_ms);
        let next = self.scheduler.start_next(&ticket.agent_id, now_ms);
        (settlement, next)
    }

    pub fn queued_task_ids(&self, agent_id: &str) -> Vec<String> {
        self.scheduler.queued_task_ids(agent_id)
    }

    pub fn active_lane(&self, agent_id: &str) -> Option<RunLane> {
        self.scheduler.get_active_lane(agent_id)
    }

    pub fn active_source(&self, agent_id: &str) -> Option<&str> {
        self.scheduler.active(agent_id).map(|active| active.item.source.as_str())
    }

    pub fn diagnostics(&self, now_ms: u64) -> Vec<QueueDiagnostics> {
        self.scheduler.diagnostics(now_ms)
    }

    pub fn is_idle(&self, agent_id: &str) -> bool {
        self.scheduler.is_idle(agent_id)
    }

    pub fn dispose(&mut self) {
        self.scheduler.dispose();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_classification_matches_desktop_lanes_and_sources() {
        assert_eq!(classify_send_dispatch(Some("automation"), false), (RunLane::Background, "automation"));
        assert_eq!(classify_send_dispatch(Some("handoff-resume"), true), (RunLane::Background, "ack-redrive"));
        assert_eq!(classify_send_dispatch(Some("agent-inbound"), false), (RunLane::Agent, "agent-inbound"));
        assert_eq!(classify_send_dispatch(Some("group-member"), false), (RunLane::User, "group-member"));
        assert_eq!(classify_send_dispatch(Some("human-handoff"), false), (RunLane::User, "human-handoff"));
        assert_eq!(classify_send_dispatch(None, false), (RunLane::User, "turn"));
    }

    #[test]
    fn user_turn_preempts_waiting_background_after_active_settles() {
        let mut dispatch = ProductionTurnDispatch::with_watchdog(100, 20);
        let (background, _, started) = dispatch
            .enqueue_turn_with_start(
                "agent-a", Some("bg"), 1, 1, RunLane::Background, "automation", None,
            )
            .unwrap();
        let bg_generation = started.unwrap().generation;
        let (_queued_background, accepted_background, not_started) = dispatch
            .enqueue_turn_with_start(
                "agent-a", Some("bg2"), 2, 2, RunLane::Background, "automation", None,
            )
            .unwrap();
        assert!(not_started.is_none());
        assert_eq!(accepted_background.position, 1);
        let (user, accepted_user, not_started) = dispatch
            .enqueue_turn_with_start(
                "agent-a", Some("user"), 3, 3, RunLane::User, "composer", None,
            )
            .unwrap();
        assert!(not_started.is_none());
        assert_eq!(accepted_user.position, 1);

        let (settled, next) = dispatch.settle_and_start_next(&background, bg_generation, 10);
        assert!(matches!(settled, RunSettlement::ActiveSettled { .. }));
        let next = next.unwrap();
        assert_eq!(next.task_id, user.task_id);
        assert_eq!(next.lane, RunLane::User);
    }

    #[test]
    fn watchdog_escape_starts_waiting_user_and_late_settle_is_zombie() {
        let mut dispatch = ProductionTurnDispatch::with_watchdog(10, 5);
        let (active, _, started) = dispatch
            .enqueue_turn_with_start(
                "agent-a", Some("agent"), 1, 1, RunLane::Agent, "agent-message", Some("ack-a"),
            )
            .unwrap();
        let generation = started.unwrap().generation;
        let (_user, _, _) = dispatch
            .enqueue_turn_with_start(
                "agent-a", Some("user"), 2, 2, RunLane::User, "composer", None,
            )
            .unwrap();
        assert_eq!(dispatch.watchdog_tick("agent-a", 12).unwrap().event.stage, WatchdogStage::Trip);
        let escaped = dispatch.watchdog_tick("agent-a", 17).unwrap();
        assert_eq!(escaped.event.stage, WatchdogStage::Escape);
        assert_eq!(escaped.started_next.unwrap().lane, RunLane::User);
        let (settled, _) = dispatch.settle_and_start_next(&active, generation, 18);
        assert!(matches!(settled, RunSettlement::ZombieSettled { .. }));
    }
}
