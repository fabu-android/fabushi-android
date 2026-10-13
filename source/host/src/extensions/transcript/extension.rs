use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::android_agent_roster::AndroidAgentRoster;
use crate::host_event_bus::{
    HostEvent, HostEventBus, HostEventFailureMode, HostEventSubscription,
};

use super::replica_writer::HostReplicaWriter;
use super::roster_emit::{ProductionRosterEmit, RosterEventSink};

pub const TRANSCRIPT_EXTENSION_ID: &str = "transcript";
pub const TRANSCRIPT_PROFILE_CHANGED_TOPIC: &str = "transcript.profile-changed";
pub const TRANSCRIPT_TIMELINE_EVENT_TOPIC: &str = "transcript.timeline-event";
pub const TRANSCRIPT_AUTOMATION_CONFIG_CHANGED_TOPIC: &str =
    "transcript.automation-config-changed";
pub const TRANSCRIPT_CHANNEL_CONFIG_CHANGED_TOPIC: &str =
    "transcript.channel-config-changed";
pub const TRANSCRIPT_LISTENER_CONNECT_CARD_TOPIC: &str =
    "transcript.listener-connect-card";
pub const TRANSCRIPT_RUN_STARTED_TOPIC: &str = "transcript.run-started";
pub const TRANSCRIPT_RUN_ENDED_TOPIC: &str = "transcript.run-ended";

#[derive(Clone)]
pub struct TranscriptExtensionEventBridge {
    events: HostEventBus,
    live_account_fence: Arc<Mutex<Option<String>>>,
    projection_events: Arc<Mutex<VecDeque<Value>>>,
}

impl TranscriptExtensionEventBridge {
    pub fn new(
        events: HostEventBus,
        live_account_fence: Arc<Mutex<Option<String>>>,
        projection_events: Arc<Mutex<VecDeque<Value>>>,
    ) -> Self {
        Self {
            events,
            live_account_fence,
            projection_events,
        }
    }

    fn current_fence(&self) -> Option<String> {
        self.live_account_fence
            .lock()
            .ok()
            .and_then(|fence| fence.clone())
            .filter(|fence| !fence.trim().is_empty())
    }

    fn emit(&self, topic: &str, mut payload: Value) -> bool {
        let Some(account_fence) = self.current_fence() else {
            return false;
        };
        if let Some(object) = payload.as_object_mut() {
            object.insert("accountFence".into(), json!(account_fence.clone()));
        }
        let _ = self
            .events
            .emit_topic(topic, &payload, HostEventFailureMode::Continue);
        if let Ok(mut queue) = self.projection_events.lock() {
            queue.push_back(json!({
                "type":topic,
                "accountFence":account_fence,
                "payload":payload,
            }));
        }
        true
    }

    pub fn profile_changed(&self, agent_id: &str) -> bool {
        self.emit(TRANSCRIPT_PROFILE_CHANGED_TOPIC, json!({"agentId":agent_id}))
    }

    pub fn timeline_event(&self, agent_id: &str, event: Value) -> bool {
        self.emit(
            TRANSCRIPT_TIMELINE_EVENT_TOPIC,
            json!({"agentId":agent_id,"event":event}),
        )
    }

    pub fn automation_config_changed(&self) -> bool {
        self.emit(TRANSCRIPT_AUTOMATION_CONFIG_CHANGED_TOPIC, json!({}))
    }

    pub fn channel_config_changed(&self) -> bool {
        self.emit(TRANSCRIPT_CHANNEL_CONFIG_CHANGED_TOPIC, json!({}))
    }

    pub fn listener_connect_card(&self, agent_id: &str, platform: &str) -> bool {
        self.emit(
            TRANSCRIPT_LISTENER_CONNECT_CARD_TOPIC,
            json!({"agentId":agent_id,"platform":platform}),
        )
    }

    pub fn run_started(&self, request_id: &str) -> bool {
        self.emit(TRANSCRIPT_RUN_STARTED_TOPIC, json!({"requestId":request_id}))
    }

    pub fn run_ended(&self, request_id: &str) -> bool {
        self.emit(TRANSCRIPT_RUN_ENDED_TOPIC, json!({"requestId":request_id}))
    }
}

pub struct TranscriptExtension {
    roster_emit: Arc<ProductionRosterEmit>,
    events: TranscriptExtensionEventBridge,
    subscriptions: Vec<HostEventSubscription>,
}

impl TranscriptExtension {
    pub fn roster_emit(&self) -> Arc<ProductionRosterEmit> {
        Arc::clone(&self.roster_emit)
    }

    pub fn event_bridge(&self) -> TranscriptExtensionEventBridge {
        self.events.clone()
    }

    pub fn subscription_count(&self) -> usize {
        self.subscriptions.len()
    }
}

impl Drop for TranscriptExtension {
    fn drop(&mut self) {
        self.subscriptions.clear();
        self.roster_emit.dispose();
    }
}

pub fn start_production_transcript_extension(
    agents: Arc<Mutex<AndroidAgentRoster>>,
    replica_writer: Arc<HostReplicaWriter>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    projection_events: Arc<Mutex<VecDeque<Value>>>,
    events: HostEventBus,
) -> TranscriptExtension {
    let roster_projection_events = Arc::clone(&projection_events);
    let roster_live_fence = Arc::clone(&live_account_fence);
    let roster_sink: RosterEventSink = Arc::new(move |event| {
        let event_fence = event
            .get("accountFence")
            .and_then(Value::as_str)
            .map(str::to_string);
        let current = roster_live_fence
            .lock()
            .ok()
            .and_then(|fence| fence.clone());
        if event_fence.is_none() || event_fence != current {
            return;
        }
        if let Ok(mut queue) = roster_projection_events.lock() {
            queue.push_back(event);
        }
    });
    let roster_emit = Arc::new(ProductionRosterEmit::new(
        agents,
        replica_writer,
        Arc::clone(&live_account_fence),
        roster_sink,
    ));
    let bridge = TranscriptExtensionEventBridge::new(
        events.clone(),
        live_account_fence,
        projection_events,
    );

    let mut subscriptions = Vec::new();

    let profile_roster = Arc::clone(&roster_emit);
    subscriptions.push(events.on(TRANSCRIPT_PROFILE_CHANGED_TOPIC, move |payload| {
        let Some(agent_id) = payload.get("agentId").and_then(Value::as_str) else {
            return Ok(());
        };
        profile_roster.publish_profile_changed(agent_id);
        Ok(())
    }));

    let timeline_roster = Arc::clone(&roster_emit);
    subscriptions.push(events.on(TRANSCRIPT_TIMELINE_EVENT_TOPIC, move |payload| {
        let Some(agent_id) = payload.get("agentId").and_then(Value::as_str) else {
            return Ok(());
        };
        let Some(event) = payload.get("event") else {
            return Ok(());
        };
        timeline_roster.publish_timeline_event(agent_id, event.clone());
        Ok(())
    }));

    subscriptions.push(bind_runner_outline_stream_events(
        &events,
        Arc::clone(&roster_emit),
    ));

    TranscriptExtension {
        roster_emit,
        events: bridge,
        subscriptions,
    }
}

pub fn bind_runner_outline_stream_events(
    events: &HostEventBus,
    roster: Arc<ProductionRosterEmit>,
) -> HostEventSubscription {
    events.subscribe_listener(move |event: &HostEvent| {
        if event.family != "runner-inference" {
            return;
        }
        let Ok(payload) = serde_json::from_str::<Value>(&event.payload_json) else {
            return;
        };
        let Some(stream_id) = payload.get("streamId").and_then(Value::as_str) else {
            return;
        };
        match payload.get("type").and_then(Value::as_str) {
            Some("delta") => {
                let Some(agent_id) = payload.get("agentId").and_then(Value::as_str) else {
                    return;
                };
                let Some(content) = payload.get("content").and_then(Value::as_str) else {
                    return;
                };
                roster.apply_runner_text_delta(agent_id, stream_id, content);
            }
            Some("completed" | "failed" | "cancelled") => {
                roster.finish_runner_outline_stream(stream_id);
            }
            _ => {}
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (
        tempfile::TempDir,
        Arc<Mutex<AndroidAgentRoster>>,
        Arc<Mutex<Option<String>>>,
        Arc<Mutex<VecDeque<Value>>>,
        HostEventBus,
        TranscriptExtension,
    ) {
        let root = tempfile::tempdir().unwrap();
        let agents = Arc::new(Mutex::new(
            AndroidAgentRoster::open(root.path().join("agents.json")).unwrap(),
        ));
        let fence = Arc::new(Mutex::new(Some("account:test".into())));
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let bus = HostEventBus::default();
        let extension = start_production_transcript_extension(
            Arc::clone(&agents),
            Arc::new(HostReplicaWriter::with_epoch("test-epoch")),
            Arc::clone(&fence),
            Arc::clone(&queue),
            bus.clone(),
        );
        (root, agents, fence, queue, bus, extension)
    }

    #[test]
    fn production_composition_projects_profile_timeline_run_and_configuration_families() {
        let (_root, agents, _fence, queue, _bus, extension) = fixture();
        let agent = agents.lock().unwrap().create("Agent A", "desc").unwrap();
        assert!(extension.event_bridge().profile_changed(&agent.id));
        assert!(extension
            .event_bridge()
            .timeline_event(&agent.id, json!({"type":"message"})));
        assert!(extension.event_bridge().automation_config_changed());
        assert!(extension.event_bridge().channel_config_changed());
        assert!(extension
            .event_bridge()
            .listener_connect_card(&agent.id, "android"));
        assert!(extension.event_bridge().run_started("request-a"));
        assert!(extension.event_bridge().run_ended("request-a"));

        let rows = queue.lock().unwrap().iter().cloned().collect::<Vec<_>>();
        for family in [
            TRANSCRIPT_PROFILE_CHANGED_TOPIC,
            TRANSCRIPT_TIMELINE_EVENT_TOPIC,
            TRANSCRIPT_AUTOMATION_CONFIG_CHANGED_TOPIC,
            TRANSCRIPT_CHANNEL_CONFIG_CHANGED_TOPIC,
            TRANSCRIPT_LISTENER_CONNECT_CARD_TOPIC,
            TRANSCRIPT_RUN_STARTED_TOPIC,
            TRANSCRIPT_RUN_ENDED_TOPIC,
        ] {
            assert!(rows.iter().any(|event| event["type"] == family));
        }
        assert!(rows.iter().any(|event| {
            event["type"] == "transcript.roster" && event["channel"] == "profile-changed"
        }));
        assert!(rows.iter().any(|event| {
            event["type"] == "transcript.roster" && event["channel"] == "timeline"
        }));
    }

    #[test]
    fn account_switch_fences_stale_projection_and_dispose_unsubscribes() {
        let (_root, agents, fence, queue, bus, extension) = fixture();
        let agent = agents.lock().unwrap().create("Agent A", "desc").unwrap();
        assert_eq!(extension.subscription_count(), 3);
        assert!(extension.event_bridge().profile_changed(&agent.id));
        let before = queue.lock().unwrap().len();

        *fence.lock().unwrap() = Some("account:other".into());
        let stale_payload = json!({
            "agentId":agent.id,
            "accountFence":"account:test",
        });
        let _ = bus.emit_topic(
            TRANSCRIPT_PROFILE_CHANGED_TOPIC,
            &stale_payload,
            HostEventFailureMode::Continue,
        );
        let rows = queue.lock().unwrap().len();
        assert_eq!(rows, before);

        drop(extension);
        let after_drop = queue.lock().unwrap().len();
        let _ = bus.emit_topic(
            TRANSCRIPT_PROFILE_CHANGED_TOPIC,
            &json!({"agentId":"agent-missing","accountFence":"account:other"}),
            HostEventFailureMode::Continue,
        );
        assert_eq!(queue.lock().unwrap().len(), after_drop);
    }

    #[test]
    fn runner_outline_subscription_accepts_current_event_family_and_ignores_bad_payload() {
        let (_root, _agents, _fence, queue, bus, extension) = fixture();
        bus.emit(HostEvent {
            family:"runner-inference".into(),
            session_id:"android-process".into(),
            payload_json:json!({
                "type":"delta",
                "streamId":"stream-a",
                "agentId":"agent-a",
                "content":"hello",
            }).to_string(),
        });
        bus.emit(HostEvent {
            family:"runner-inference".into(),
            session_id:"android-process".into(),
            payload_json:"not-json".into(),
        });
        assert!(queue.lock().unwrap().iter().any(|event| {
            event["channel"] == "outline"
                && event["payload"]["type"] == "appended"
                && event["payload"]["item"]["text"] == "hello"
        }));
        drop(extension);
    }
}
