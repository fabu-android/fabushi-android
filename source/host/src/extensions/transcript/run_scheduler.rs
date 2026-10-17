use std::collections::{HashMap, HashSet, VecDeque};

pub const RUN_WATCHDOG_DEFAULT_MS: u64 = 120_000;
pub const RUN_WATCHDOG_GRACE_DEFAULT_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunLane {
    User,
    Agent,
    Background,
}

impl RunLane {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
            Self::Background => "background",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedRun {
    pub task_id: String,
    pub lane: RunLane,
    pub source: String,
    pub enqueued_at_ms: u64,
    pub accepted_at_ms: Option<u64>,
    pub ack_token: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePhase {
    Running,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveRun {
    pub item: QueuedRun,
    pub started_at_ms: u64,
    pub generation: u64,
    pub phase: ActivePhase,
    pub watchdog_tripped_at_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueAccepted {
    pub agent_id: String,
    pub task_id: String,
    pub lane: RunLane,
    pub source: String,
    pub position: usize,
    pub depth_user: usize,
    pub depth_agent: usize,
    pub depth_background: usize,
    pub has_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueDequeued {
    pub agent_id: String,
    pub task_id: String,
    pub lane: RunLane,
    pub source: String,
    pub queue_wait_ms: u64,
    pub accepted_to_run_ms: Option<u64>,
    pub jumped_background: usize,
    pub depth_user: usize,
    pub depth_agent: usize,
    pub depth_background: usize,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogStage {
    Trip,
    Escape,
    LateSettle,
}

impl WatchdogStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trip => "trip",
            Self::Escape => "escape",
            Self::LateSettle => "late_settle",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchdogEvent {
    pub agent_id: String,
    pub stage: WatchdogStage,
    pub active_lane: RunLane,
    pub active_source: String,
    pub active_runtime_ms: u64,
    pub waiting_user_age_ms: Option<u64>,
    pub ack_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunSettlement {
    ActiveSettled {
        agent_id: String,
        task_id: String,
        generation: u64,
    },
    ZombieSettled {
        agent_id: String,
        task_id: String,
        generation: u64,
        watchdog: WatchdogEvent,
    },
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueDiagnostics {
    pub agent_id: String,
    pub depth_user: usize,
    pub depth_agent: usize,
    pub depth_background: usize,
    pub depth_total: usize,
    pub oldest_pending_user_age_ms: Option<u64>,
    pub active: Option<ActiveDiagnostics>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveDiagnostics {
    pub lane: RunLane,
    pub source: String,
    pub runtime_ms: u64,
    pub phase: ActivePhase,
    pub generation: u64,
}

#[derive(Debug, Default)]
struct RunQueue {
    pending_user: VecDeque<QueuedRun>,
    pending_agent: VecDeque<QueuedRun>,
    pending_background: VecDeque<QueuedRun>,
    active: Option<ActiveRun>,
    generation_counter: u64,
    zombies: HashMap<u64, QueuedRun>,
}

impl RunQueue {
    fn pending_depth(&self) -> usize {
        self.pending_user.len() + self.pending_agent.len() + self.pending_background.len()
    }

    fn user_next(&mut self) -> Option<QueuedRun> {
        if self.pending_user.is_empty() {
            return None;
        }
        let index = self
            .pending_user
            .iter()
            .position(|item| item.source != "group-member")
            .unwrap_or(0);
        self.pending_user.remove(index)
    }

    fn next(&mut self) -> Option<QueuedRun> {
        self.user_next()
            .or_else(|| self.pending_agent.pop_front())
            .or_else(|| self.pending_background.pop_front())
    }
}

#[derive(Debug)]
pub struct RunScheduler {
    queues: HashMap<String, RunQueue>,
    watchdog_ms: u64,
    watchdog_grace_ms: u64,
    disposed: bool,
    seen_task_ids: HashSet<String>,
}

impl Default for RunScheduler {
    fn default() -> Self {
        Self::new(RUN_WATCHDOG_DEFAULT_MS, RUN_WATCHDOG_GRACE_DEFAULT_MS)
    }
}

impl RunScheduler {
    pub fn new(watchdog_ms: u64, watchdog_grace_ms: u64) -> Self {
        Self {
            queues: HashMap::new(),
            watchdog_ms,
            watchdog_grace_ms,
            disposed: false,
            seen_task_ids: HashSet::new(),
        }
    }

    pub fn enqueue(
        &mut self,
        agent_id: impl Into<String>,
        item: QueuedRun,
    ) -> Result<QueueAccepted, &'static str> {
        if self.disposed {
            return Err("run scheduler is disposed");
        }
        if item.task_id.trim().is_empty() || item.source.trim().is_empty() {
            return Err("run task identity is incomplete");
        }
        if !self.seen_task_ids.insert(item.task_id.clone()) {
            return Err("run task id is already known");
        }
        let agent_id = agent_id.into();
        if agent_id.trim().is_empty() {
            return Err("agent id is empty");
        }
        let queue = self.queues.entry(agent_id.clone()).or_default();
        match item.lane {
            RunLane::User => queue.pending_user.push_back(item.clone()),
            RunLane::Agent => queue.pending_agent.push_back(item.clone()),
            RunLane::Background => queue.pending_background.push_back(item.clone()),
        }
        let pending_ahead = match item.lane {
            RunLane::User => queue.pending_user.len().saturating_sub(1),
            RunLane::Agent => queue.pending_user.len() + queue.pending_agent.len().saturating_sub(1),
            RunLane::Background => queue.pending_depth().saturating_sub(1),
        };
        Ok(QueueAccepted {
            agent_id,
            task_id: item.task_id,
            lane: item.lane,
            source: item.source,
            position: usize::from(queue.active.is_some()) + pending_ahead,
            depth_user: queue.pending_user.len(),
            depth_agent: queue.pending_agent.len(),
            depth_background: queue.pending_background.len(),
            has_active: queue.active.is_some(),
        })
    }

    pub fn start_next(&mut self, agent_id: &str, now_ms: u64) -> Option<QueueDequeued> {
        if self.disposed {
            return None;
        }
        let queue = self.queues.get_mut(agent_id)?;
        if queue.active.is_some() {
            return None;
        }
        let next = queue.next()?;
        queue.generation_counter = queue.generation_counter.saturating_add(1);
        let generation = queue.generation_counter;
        let dequeued = QueueDequeued {
            agent_id: agent_id.to_string(),
            task_id: next.task_id.clone(),
            lane: next.lane,
            source: next.source.clone(),
            queue_wait_ms: now_ms.saturating_sub(next.enqueued_at_ms),
            accepted_to_run_ms: next.accepted_at_ms.map(|at| now_ms.saturating_sub(at)),
            jumped_background: if next.lane == RunLane::Background {
                0
            } else {
                queue.pending_background.len()
            },
            depth_user: queue.pending_user.len(),
            depth_agent: queue.pending_agent.len(),
            depth_background: queue.pending_background.len(),
            generation,
        };
        queue.active = Some(ActiveRun {
            item: next,
            started_at_ms: now_ms,
            generation,
            phase: ActivePhase::Running,
            watchdog_tripped_at_ms: None,
        });
        Some(dequeued)
    }

    pub fn active(&self, agent_id: &str) -> Option<&ActiveRun> {
        self.queues.get(agent_id)?.active.as_ref()
    }

    pub fn get_active_lane(&self, agent_id: &str) -> Option<RunLane> {
        self.active(agent_id).map(|active| active.item.lane)
    }

    pub fn next_watchdog_delay_ms(&self, agent_id: &str, now_ms: u64) -> Option<u64> {
        if self.disposed {
            return None;
        }
        let queue = self.queues.get(agent_id)?;
        let head = queue.pending_user.front()?;
        let active = queue.active.as_ref()?;
        let deadline = match active.watchdog_tripped_at_ms {
            Some(tripped_at) => tripped_at.saturating_add(self.watchdog_grace_ms),
            None => head
                .enqueued_at_ms
                .max(active.started_at_ms)
                .saturating_add(self.watchdog_ms),
        };
        Some(deadline.saturating_sub(now_ms))
    }

    pub fn watchdog_tick(&mut self, agent_id: &str, now_ms: u64) -> Option<WatchdogEvent> {
        if self.disposed {
            return None;
        }
        let queue = self.queues.get_mut(agent_id)?;
        let head = queue.pending_user.front()?;
        let active = queue.active.as_mut()?;
        let waited_behind_active_ms =
            now_ms.saturating_sub(head.enqueued_at_ms.max(active.started_at_ms));
        if waited_behind_active_ms < self.watchdog_ms {
            return None;
        }

        match active.watchdog_tripped_at_ms {
            None => {
                active.phase = ActivePhase::Interrupted;
                active.watchdog_tripped_at_ms = Some(now_ms);
                Some(WatchdogEvent {
                    agent_id: agent_id.to_string(),
                    stage: WatchdogStage::Trip,
                    active_lane: active.item.lane,
                    active_source: active.item.source.clone(),
                    active_runtime_ms: now_ms.saturating_sub(active.started_at_ms),
                    waiting_user_age_ms: Some(now_ms.saturating_sub(head.enqueued_at_ms)),
                    ack_token: None,
                })
            }
            Some(tripped_at)
                if now_ms.saturating_sub(tripped_at) >= self.watchdog_grace_ms =>
            {
                let escaped = queue.active.take().expect("active run");
                let event = WatchdogEvent {
                    agent_id: agent_id.to_string(),
                    stage: WatchdogStage::Escape,
                    active_lane: escaped.item.lane,
                    active_source: escaped.item.source.clone(),
                    active_runtime_ms: now_ms.saturating_sub(escaped.started_at_ms),
                    waiting_user_age_ms: queue
                        .pending_user
                        .front()
                        .map(|user| now_ms.saturating_sub(user.enqueued_at_ms)),
                    ack_token: escaped.item.ack_token.clone(),
                };
                queue.zombies.insert(escaped.generation, escaped.item);
                Some(event)
            }
            Some(_) => None,
        }
    }

    pub fn settle(
        &mut self,
        agent_id: &str,
        generation: u64,
        now_ms: u64,
    ) -> RunSettlement {
        let Some(queue) = self.queues.get_mut(agent_id) else {
            return RunSettlement::Unknown;
        };

        if queue.active.as_ref().is_some_and(|active| active.generation == generation) {
            let active = queue.active.take().expect("active");
            return RunSettlement::ActiveSettled {
                agent_id: agent_id.to_string(),
                task_id: active.item.task_id,
                generation,
            };
        }

        if let Some(item) = queue.zombies.remove(&generation) {
            let event = WatchdogEvent {
                agent_id: agent_id.to_string(),
                stage: WatchdogStage::LateSettle,
                active_lane: item.lane,
                active_source: item.source.clone(),
                active_runtime_ms: now_ms.saturating_sub(item.enqueued_at_ms),
                waiting_user_age_ms: None,
                ack_token: item.ack_token.clone(),
            };
            return RunSettlement::ZombieSettled {
                agent_id: agent_id.to_string(),
                task_id: item.task_id,
                generation,
                watchdog: event,
            };
        }

        RunSettlement::Unknown
    }

    pub fn diagnostics(&self, now_ms: u64) -> Vec<QueueDiagnostics> {
        let mut out = self
            .queues
            .iter()
            .filter_map(|(agent_id, queue)| {
                let depth_user = queue.pending_user.len();
                let depth_agent = queue.pending_agent.len();
                let depth_background = queue.pending_background.len();
                if depth_user + depth_agent + depth_background == 0 && queue.active.is_none() {
                    return None;
                }
                Some(QueueDiagnostics {
                    agent_id: agent_id.clone(),
                    depth_user,
                    depth_agent,
                    depth_background,
                    depth_total: depth_user
                        + depth_agent
                        + depth_background
                        + usize::from(queue.active.is_some()),
                    oldest_pending_user_age_ms: queue
                        .pending_user
                        .front()
                        .map(|item| now_ms.saturating_sub(item.enqueued_at_ms)),
                    active: queue.active.as_ref().map(|active| ActiveDiagnostics {
                        lane: active.item.lane,
                        source: active.item.source.clone(),
                        runtime_ms: now_ms.saturating_sub(active.started_at_ms),
                        phase: active.phase,
                        generation: active.generation,
                    }),
                })
            })
            .collect::<Vec<_>>();
        out.sort_by(|a, b| a.agent_id.cmp(&b.agent_id));
        out
    }

    pub fn queued_task_ids(&self, agent_id: &str) -> Vec<String> {
        let Some(queue) = self.queues.get(agent_id) else {
            return Vec::new();
        };
        queue
            .pending_user
            .iter()
            .chain(queue.pending_agent.iter())
            .chain(queue.pending_background.iter())
            .map(|item| item.task_id.clone())
            .collect()
    }

    pub fn is_idle(&self, agent_id: &str) -> bool {
        self.queues.get(agent_id).is_none_or(|queue| {
            queue.active.is_none()
                && queue.pending_user.is_empty()
                && queue.pending_agent.is_empty()
                && queue.pending_background.is_empty()
                && queue.zombies.is_empty()
        })
    }

    pub fn dispose(&mut self) {
        if self.disposed {
            return;
        }
        self.disposed = true;
        self.queues.clear();
        self.seen_task_ids.clear();
    }
}

pub fn take_next_user_task(pending: &mut VecDeque<QueuedRun>) -> Option<QueuedRun> {
    if pending.is_empty() {
        return None;
    }
    let index = pending
        .iter()
        .position(|item| item.source != "group-member")
        .unwrap_or(0);
    pending.remove(index)
}
