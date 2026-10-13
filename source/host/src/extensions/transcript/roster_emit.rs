use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::android_agent_roster::AndroidAgentRoster;

use super::replica_writer::HostReplicaWriter;
use super::roster_projection::{
    OutlineQueueResult, OutlineStreamCoalescingPolicy, OutlineUpdate, RosterProjection,
};

pub const ROSTER_REPLICA_KEY: &str = "roster";
pub const OUTLINE_STREAM_COALESCE_MS: u64 = 250;

pub type RosterEventSink = Arc<dyn Fn(Value) + Send + Sync + 'static>;

struct OutlineStreamSchedulerState {
    policy: Option<OutlineStreamCoalescingPolicy<Value>>,
    shutdown: bool,
}

struct OutlineStreamScheduler {
    started_at: Instant,
    shared: Arc<(Mutex<OutlineStreamSchedulerState>, Condvar)>,
    worker: Mutex<Option<JoinHandle<()>>>,
    event_sink: RosterEventSink,
}

impl OutlineStreamScheduler {
    fn new(event_sink: RosterEventSink) -> Self {
        let started_at = Instant::now();
        let shared = Arc::new((
            Mutex::new(OutlineStreamSchedulerState {
                policy: Some(OutlineStreamCoalescingPolicy::new(
                    OUTLINE_STREAM_COALESCE_MS,
                    0,
                )),
                shutdown: false,
            }),
            Condvar::new(),
        ));
        let worker_shared = Arc::clone(&shared);
        let worker_sink = Arc::clone(&event_sink);
        let worker_started_at = started_at;
        let worker = thread::Builder::new()
            .name("fabushi-android-outline-coalescer".into())
            .spawn(move || {
                run_outline_stream_worker(worker_started_at, worker_shared, worker_sink)
            })
            .expect("Android transcript outline coalescer must start");
        Self {
            started_at,
            shared,
            worker: Mutex::new(Some(worker)),
            event_sink,
        }
    }

    fn now_ms(&self) -> u64 {
        self.started_at
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    }

    fn queue(&self, agent_id: &str, item_id: &str, item: Value) {
        let now_ms = self.now_ms();
        let (state, wake) = &*self.shared;
        let result = {
            let mut state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match state.policy.as_mut() {
                Some(policy) => policy.queue(now_ms, agent_id, item_id, item),
                None => OutlineQueueResult {
                    flushed: vec![OutlineUpdate {
                        agent_id: agent_id.to_string(),
                        item,
                    }],
                    deadline: None,
                },
            }
        };
        wake.notify_all();
        for update in result.flushed {
            emit_outline_update(&self.event_sink, update);
        }
    }

    fn flush(&self) {
        let now_ms = self.now_ms();
        let (state, wake) = &*self.shared;
        let update = {
            let mut state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state
                .policy
                .as_mut()
                .and_then(|policy| policy.flush(now_ms))
        };
        wake.notify_all();
        if let Some(update) = update {
            emit_outline_update(&self.event_sink, update);
        }
    }

    fn stop(&self) {
        let (state, wake) = &*self.shared;
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(policy) = state.policy.as_mut() {
            policy.stop();
        }
        state.policy = None;
        wake.notify_all();
    }

    fn has_pending(&self) -> bool {
        let (state, _) = &*self.shared;
        state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .policy
            .as_ref()
            .is_some_and(|policy| policy.has_pending())
    }
}

impl Drop for OutlineStreamScheduler {
    fn drop(&mut self) {
        let (state, wake) = &*self.shared;
        {
            let mut state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.shutdown = true;
            wake.notify_all();
        }
        if let Some(worker) = self
            .worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            let _ = worker.join();
        }
    }
}

fn run_outline_stream_worker(
    started_at: Instant,
    shared: Arc<(Mutex<OutlineStreamSchedulerState>, Condvar)>,
    event_sink: RosterEventSink,
) {
    loop {
        let update = {
            let (state, wake) = &*shared;
            let mut state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            loop {
                if state.shutdown {
                    return;
                }
                let Some(deadline) = state
                    .policy
                    .as_ref()
                    .and_then(|policy| policy.next_deadline())
                else {
                    state = wake
                        .wait(state)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    continue;
                };
                let now_ms = started_at
                    .elapsed()
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64;
                if now_ms >= deadline.at_ms {
                    break state
                        .policy
                        .as_mut()
                        .and_then(|policy| policy.flush_deadline(now_ms, deadline.generation));
                }
                let wait_for = Duration::from_millis(deadline.at_ms.saturating_sub(now_ms));
                let (next, _) = wake
                    .wait_timeout(state, wait_for)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state = next;
            }
        };
        if let Some(update) = update {
            emit_outline_update(&event_sink, update);
        }
    }
}

fn emit_outline_update(event_sink: &RosterEventSink, update: OutlineUpdate<Value>) {
    (event_sink)(json!({
        "type":"transcript.roster",
        "channel":"outline",
        "payload":{
            "type":"updated",
            "agentId":update.agent_id,
            "item":update.item,
        }
    }));
}

pub struct ProductionRosterEmit {
    agents: Arc<Mutex<AndroidAgentRoster>>,
    replica_writer: Arc<HostReplicaWriter>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    event_sink: RosterEventSink,
    outline_stream: OutlineStreamScheduler,
    projection: Mutex<RosterProjection<Value>>,
}

impl ProductionRosterEmit {
    pub fn new(
        agents: Arc<Mutex<AndroidAgentRoster>>,
        replica_writer: Arc<HostReplicaWriter>,
        live_account_fence: Arc<Mutex<Option<String>>>,
        event_sink: RosterEventSink,
    ) -> Self {
        Self {
            agents,
            replica_writer,
            live_account_fence,
            outline_stream: OutlineStreamScheduler::new(Arc::clone(&event_sink)),
            event_sink,
            projection: Mutex::new(RosterProjection::default()),
        }
    }

    fn current_fence(&self) -> Option<String> {
        self.live_account_fence
            .lock()
            .ok()
            .and_then(|fence| fence.clone())
            .filter(|fence| !fence.trim().is_empty())
    }

    fn emit_account_fenced(&self, mut event: Value) -> bool {
        let Some(account_fence) = self.current_fence() else {
            return false;
        };
        if let Some(object) = event.as_object_mut() {
            object.insert("accountFence".into(), json!(account_fence));
        }
        (self.event_sink)(event);
        true
    }

    pub fn emit_agents(&self) -> Result<bool, String> {
        let agents = self
            .agents
            .lock()
            .map_err(|_| "canonical Android agent roster lock poisoned".to_string())?
            .list()
            .into_iter()
            .map(|agent| agent.as_json())
            .collect::<Vec<_>>();
        let stamp = self.replica_writer.next_stamp(ROSTER_REPLICA_KEY);
        Ok(self.emit_account_fenced(json!({
            "type":"transcript.roster",
            "channel":"agents",
            "payload":{
                "agents":agents,
                "ordered":{
                    "replicaKey":stamp.replica_key,
                    "epoch":stamp.epoch,
                    "sequence":stamp.sequence,
                },
                "coverage":{"kind":"complete-roster"},
            }
        })))
    }

    pub fn publish_profile_changed(&self, agent_id: &str) -> bool {
        if agent_id.trim().is_empty() {
            return false;
        }
        let profile = self
            .agents
            .lock()
            .ok()
            .and_then(|agents| agents.get(agent_id))
            .map(|agent| agent.as_json());
        self.emit_account_fenced(json!({
            "type":"transcript.roster",
            "channel":"profile-changed",
            "payload":{"agentId":agent_id,"profile":profile},
        }))
    }

    pub fn publish_timeline_event(&self, agent_id: &str, event: Value) -> bool {
        if agent_id.trim().is_empty() {
            return false;
        }
        self.emit_account_fenced(json!({
            "type":"transcript.roster",
            "channel":"timeline",
            "payload":{"agentId":agent_id,"event":event},
        }))
    }

    pub fn apply_runner_text_delta(
        &self,
        agent_id: &str,
        stream_id: &str,
        accumulated: &str,
    ) {
        if agent_id.trim().is_empty() || stream_id.trim().is_empty() || accumulated.is_empty() {
            return;
        }
        if self.current_fence().is_none() {
            return;
        }
        let (item_id, is_first) = self
            .projection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .stream_outline_item_id_or_insert_with(stream_id, || uuid::Uuid::new_v4().to_string());
        let item = json!({"kind":"assistant-text","id":item_id,"text":accumulated});
        if is_first {
            let _ = self.emit_account_fenced(json!({
                "type":"transcript.roster",
                "channel":"outline",
                "payload":{
                    "type":"appended",
                    "agentId":agent_id,
                    "item":item,
                }
            }));
        } else {
            let fence = self.current_fence();
            let sink = Arc::clone(&self.event_sink);
            let agent_id = agent_id.to_string();
            let item_id = item_id.clone();
            let item = item.clone();
            // The scheduler owns only transient coalescing state. Account fencing is rechecked
            // before queueing; consumers recheck the accountFence carried by the projection.
            if fence.is_some() {
                self.outline_stream.queue(&agent_id, &item_id, item);
            }
            drop(sink);
        }
    }

    pub fn finish_runner_outline_stream(&self, stream_id: &str) {
        let removed = self
            .projection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .finish_outline_stream(stream_id);
        if removed {
            self.outline_stream.flush();
        }
    }

    pub fn has_pending_outline_stream_update(&self) -> bool {
        self.outline_stream.has_pending()
    }

    pub fn dispose(&self) {
        self.outline_stream.stop();
        self.projection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear_outline_streams();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn fixture() -> (
        tempfile::TempDir,
        Arc<Mutex<AndroidAgentRoster>>,
        Arc<Mutex<Option<String>>>,
        Arc<Mutex<Vec<Value>>>,
        ProductionRosterEmit,
    ) {
        let root = tempfile::tempdir().unwrap();
        let roster = Arc::new(Mutex::new(
            AndroidAgentRoster::open(root.path().join("agents.json")).unwrap(),
        ));
        let fence = Arc::new(Mutex::new(Some("account:test".into())));
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink_events = Arc::clone(&events);
        let sink: RosterEventSink = Arc::new(move |event| {
            sink_events.lock().unwrap().push(event);
        });
        let emit = ProductionRosterEmit::new(
            Arc::clone(&roster),
            Arc::new(HostReplicaWriter::with_epoch("epoch-test")),
            Arc::clone(&fence),
            sink,
        );
        (root, roster, fence, events, emit)
    }

    #[test]
    fn roster_projection_is_account_fenced_and_reconstructs_from_canonical_store() {
        let (root, roster, fence, events, emit) = fixture();
        let agent = roster.lock().unwrap().create("A", "desc").unwrap();
        assert!(emit.emit_agents().unwrap());
        let first = events.lock().unwrap().last().cloned().unwrap();
        assert_eq!(first["accountFence"], "account:test");
        assert_eq!(first["payload"]["agents"][0]["id"], agent.id);

        *fence.lock().unwrap() = None;
        assert!(!emit.publish_profile_changed(&agent.id));
        let count = events.lock().unwrap().len();
        drop(emit);

        let reopened = Arc::new(Mutex::new(
            AndroidAgentRoster::open(root.path().join("agents.json")).unwrap(),
        ));
        let reopened_events = Arc::new(Mutex::new(Vec::new()));
        let output = Arc::clone(&reopened_events);
        let reopened_fence = Arc::new(Mutex::new(Some("account:test-2".into())));
        let reopened_emit = ProductionRosterEmit::new(
            reopened,
            Arc::new(HostReplicaWriter::with_epoch("epoch-reopened")),
            reopened_fence,
            Arc::new(move |event| output.lock().unwrap().push(event)),
        );
        assert!(reopened_emit.emit_agents().unwrap());
        assert_eq!(events.lock().unwrap().len(), count);
        assert_eq!(
            reopened_events.lock().unwrap()[0]["payload"]["agents"][0]["id"],
            agent.id
        );
    }

    #[test]
    fn outline_stream_coalesces_and_dispose_fences_stale_deadline() {
        let (_root, _roster, _fence, events, emit) = fixture();
        emit.apply_runner_text_delta("agent-a", "stream-a", "a");
        emit.apply_runner_text_delta("agent-a", "stream-a", "ab");
        emit.apply_runner_text_delta("agent-a", "stream-a", "abc");
        assert!(emit.has_pending_outline_stream_update());
        thread::sleep(Duration::from_millis(350));
        let rows = events.lock().unwrap().clone();
        assert!(rows.iter().any(|event| {
            event["channel"] == "outline"
                && event["payload"]["type"] == "appended"
                && event["payload"]["item"]["text"] == "a"
        }));
        assert!(rows.iter().any(|event| {
            event["channel"] == "outline"
                && event["payload"]["type"] == "updated"
                && event["payload"]["item"]["text"] == "abc"
        }));
        emit.dispose();
        assert!(!emit.has_pending_outline_stream_update());
    }
}
