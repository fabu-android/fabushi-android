use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use base64::Engine as _;
use serde_json::{Map, Value, json};
use uuid::Uuid;

pub const SNAPSHOT_TIMEOUT_MS: u64 = 5_000;
pub const MAX_DOMAIN_LENGTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HandoffTelemetry {
    pub reason: Option<String>,
    pub domain: Option<String>,
    pub idp_domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoffRequest {
    pub agent_id: String,
    pub instruction: String,
    pub telemetry: HandoffTelemetry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingHandoff {
    pub request_id: String,
    pub instruction: String,
    pub snapshot_data_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandoffTrigger {
    Name(String),
    Detailed {
        resolution: Option<String>,
        trigger: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndHandoffDecision {
    pub request_id: String,
    pub resolution: String,
    pub trigger: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandoffDecision {
    None,
    End(EndHandoffDecision),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandoffStartResult {
    Started { request_id: String },
    AlreadyPending {
        request_id: String,
        instruction: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoffStartedEvent {
    pub agent_id: String,
    pub instruction: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoffEndedEvent {
    pub agent_id: String,
    pub request_id: String,
    pub resolution: String,
    pub trigger: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenshotPayload {
    Base64(String),
    Bytes(Vec<u8>),
}

pub type PrepareHandoff = Arc<dyn Fn(&HandoffRequest) -> Result<(), String> + Send + Sync>;
pub type GrabScreenshot =
    Arc<dyn Fn(&str) -> Result<Option<ScreenshotPayload>, String> + Send + Sync>;
pub type HandoffStartedCallback = Arc<dyn Fn(HandoffStartedEvent) + Send + Sync>;
pub type HandoffEndedCallback =
    Arc<dyn Fn(HandoffEndedEvent) -> Result<(), String> + Send + Sync>;
pub type HandoffStatusCallback =
    Arc<dyn Fn(&str, Option<PendingHandoff>) + Send + Sync>;
pub type TelemetryReportCallback = Arc<dyn Fn(Value) + Send + Sync>;
pub type TrackEventCallback = Arc<dyn Fn(&str, Value) + Send + Sync>;

#[derive(Clone, Default)]
pub struct BoxHandoffDeps {
    pub grab_screenshot: Option<GrabScreenshot>,
    pub prepare: Option<PrepareHandoff>,
    pub on_started: Option<HandoffStartedCallback>,
    pub on_ended: Option<HandoffEndedCallback>,
    pub on_status_changed: Option<HandoffStatusCallback>,
    pub report_box_help: Option<TelemetryReportCallback>,
    pub track_event: Option<TrackEventCallback>,
    pub report: Option<TelemetryReportCallback>,
    pub timeout_ms: Option<u64>,
}

#[derive(Clone)]
pub struct BoxHandoffService {
    pending: Arc<Mutex<BTreeMap<String, PendingHandoff>>>,
    deps: BoxHandoffDeps,
}

impl BoxHandoffService {
    pub fn new(deps: BoxHandoffDeps) -> Self {
        Self {
            pending: Arc::new(Mutex::new(BTreeMap::new())),
            deps,
        }
    }

    pub fn get(&self, agent_id: &str) -> Option<PendingHandoff> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(agent_id)
            .cloned()
    }

    pub fn forget(&self, agent_id: &str) {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(agent_id);
    }

    pub fn pending_count(&self) -> usize {
        self.pending
            .lock()
            .map(|pending| pending.len())
            .unwrap_or_default()
    }

    /// Clear process-local handoff state during Host shutdown. Snapshot workers
    /// cannot resurrect entries once their request identity is gone.
    pub fn clear_all(&self) {
        let agent_ids = {
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let agent_ids = pending.keys().cloned().collect::<Vec<_>>();
            pending.clear();
            agent_ids
        };
        for agent_id in agent_ids {
            self.notify_status(&agent_id);
        }
    }

    pub fn start(&self, request: HandoffRequest) -> HandoffStartResult {
        let request_id = {
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(live) = pending.get(&request.agent_id) {
                return HandoffStartResult::AlreadyPending {
                    request_id: live.request_id.clone(),
                    instruction: live.instruction.clone(),
                };
            }
            let request_id = Uuid::new_v4().to_string();
            pending.insert(
                request.agent_id.clone(),
                PendingHandoff {
                    request_id: request_id.clone(),
                    instruction: request.instruction.clone(),
                    snapshot_data_url: None,
                },
            );
            request_id
        };

        if let Some(on_started) = self.deps.on_started.as_ref() {
            on_started(HandoffStartedEvent {
                agent_id: request.agent_id.clone(),
                instruction: request.instruction.clone(),
            });
        }
        self.notify_status(&request.agent_id);

        let service = self.clone();
        let capture_request = request.clone();
        let capture_request_id = request_id.clone();
        let _ = thread::Builder::new()
            .name("session-box-handoff-snapshot".into())
            .spawn(move || service.capture_snapshot(capture_request, capture_request_id));

        HandoffStartResult::Started { request_id }
    }

    pub fn end(&self, agent_id: &str, trigger: HandoffTrigger) -> Result<bool, String> {
        let decision = decide_box_hand_back(self.get(agent_id).as_ref(), &trigger);
        let HandoffDecision::End(decision) = decision else {
            return Ok(false);
        };
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(agent_id);

        if let Some(on_ended) = self.deps.on_ended.as_ref() {
            on_ended(HandoffEndedEvent {
                agent_id: agent_id.to_string(),
                request_id: decision.request_id,
                resolution: decision.resolution,
                trigger: decision.trigger,
            })?;
        }
        self.notify_status(agent_id);
        Ok(true)
    }

    fn notify_status(&self, agent_id: &str) {
        if let Some(on_status_changed) = self.deps.on_status_changed.as_ref() {
            on_status_changed(agent_id, self.get(agent_id));
        }
    }

    fn capture_snapshot(&self, request: HandoffRequest, request_id: String) {
        let mut captured = false;
        let _ = (|| -> Result<(), String> {
            if let Some(prepare) = self.deps.prepare.as_ref() {
                prepare(&request)?;
            }
            let Some(grab) = self.deps.grab_screenshot.as_ref().cloned() else {
                return Ok(());
            };
            let timeout_ms = self.deps.timeout_ms.unwrap_or(SNAPSHOT_TIMEOUT_MS);
            let (sender, receiver) = mpsc::sync_channel(1);
            let agent_id = request.agent_id.clone();
            let _ = thread::Builder::new()
                .name("session-box-handoff-grab".into())
                .spawn(move || {
                    let _ = sender.send(grab(&agent_id));
                });
            let screenshot = match receiver.recv_timeout(Duration::from_millis(timeout_ms)) {
                Ok(Ok(value)) => value,
                Ok(Err(_)) | Err(_) => None,
            };
            let Some(screenshot) = screenshot else {
                return Ok(());
            };
            let encoded = match screenshot {
                ScreenshotPayload::Base64(value) => value,
                ScreenshotPayload::Bytes(bytes) => {
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                }
            };
            if encoded.is_empty() {
                return Ok(());
            }

            let updated = {
                let mut pending = self
                    .pending
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let Some(live) = pending.get_mut(&request.agent_id) else {
                    return Ok(());
                };
                if live.request_id != request_id {
                    return Ok(());
                }
                live.snapshot_data_url =
                    Some(format!("data:image/webp;base64,{encoded}"));
                true
            };
            if updated {
                captured = true;
                self.notify_status(&request.agent_id);
            }
            Ok(())
        })();

        self.report_capture(&request, captured);
    }

    fn report_capture(&self, request: &HandoffRequest, captured: bool) {
        let reason = request.telemetry.reason.as_deref();
        let analytics_reason = match reason {
            Some("auth" | "captcha" | "payment") => reason.map(str::to_string),
            Some(_) => Some("other".to_string()),
            None => None,
        };

        if let Some(report) = self.deps.report_box_help.as_ref() {
            report(json!({
                "conversationId": request.agent_id,
                "snapshotCaptured": captured,
                "reason": reason,
            }));
        }

        if let Some(track) = self.deps.track_event.as_ref() {
            let mut properties = Map::from_iter([
                ("agent_id".to_string(), Value::String(request.agent_id.clone())),
                ("snapshot_captured".to_string(), Value::Bool(captured)),
            ]);
            if let Some(reason) = analytics_reason {
                properties.insert("reason".into(), Value::String(reason));
            }
            if let Some(domain) = request.telemetry.domain.as_deref() {
                properties.insert("domain".into(), Value::String(truncate(domain)));
            }
            if let Some(domain) = request.telemetry.idp_domain.as_deref() {
                properties.insert("idp_domain".into(), Value::String(truncate(domain)));
            }
            track("sand.box_help", Value::Object(properties));
        }

        if let Some(report) = self.deps.report.as_ref() {
            report(json!({
                "outcome": if captured { "ready" } else { "no_snapshot" },
                "agentId": request.agent_id,
            }));
        }
    }
}

pub fn decide_box_hand_back(
    pending: Option<&PendingHandoff>,
    trigger: &HandoffTrigger,
) -> HandoffDecision {
    let Some(pending) = pending else {
        return HandoffDecision::None;
    };
    let (resolution, trigger) = match trigger {
        HandoffTrigger::Name(trigger) => (
            if trigger == "cancel" {
                "cancelled".to_string()
            } else {
                "completed".to_string()
            },
            trigger.clone(),
        ),
        HandoffTrigger::Detailed {
            resolution,
            trigger,
        } => (
            resolution.clone().unwrap_or_else(|| "completed".into()),
            trigger.clone().unwrap_or_else(|| "unknown".into()),
        ),
    };
    HandoffDecision::End(EndHandoffDecision {
        request_id: pending.request_id.clone(),
        resolution,
        trigger,
    })
}

fn truncate(value: &str) -> String {
    value.chars().take(MAX_DOMAIN_LENGTH).collect()
}
