use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const SEND_DISPATCH_EVENT: &str = "sand.send_dispatch";
pub const QUEUE_ACCEPTED_EVENT: &str = "sand.queue.accepted";
pub const QUEUE_DEQUEUED_EVENT: &str = "sand.queue.dequeued";
pub const QUEUE_WATCHDOG_EVENT: &str = "sand.queue.watchdog";
pub const ACK_OBLIGATION_EVENT: &str = "sand.ack.obligation";
pub const PENDING_WAKE_EVENT: &str = "sand.pending_wake";

#[derive(Debug, Clone, PartialEq)]
pub struct SendDispatchReport {
    pub conversation_id: String,
    pub dispatch_ms: Option<f64>,
    pub host_dispatch_ms: f64,
    pub skew: String,
    pub skew_reason: Option<String>,
    pub skew_bucket: Option<String>,
    pub is_fork: bool,
    pub model_id: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueAcceptedReport {
    pub conversation_id: String,
    pub lane: String,
    pub source: String,
    pub position: i64,
    pub depth_user: i64,
    pub depth_agent: i64,
    pub depth_background: i64,
    pub has_active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueueDequeuedReport {
    pub conversation_id: String,
    pub lane: String,
    pub source: String,
    pub queue_wait_ms: f64,
    pub accepted_to_run_ms: Option<f64>,
    pub jumped_background: i64,
    pub depth_user: i64,
    pub depth_agent: i64,
    pub depth_background: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueueWatchdogReport {
    pub conversation_id: String,
    pub stage: String,
    pub active_lane: Option<String>,
    pub active_source: Option<String>,
    pub active_runtime_ms: f64,
    pub waiting_user_age_ms: Option<f64>,
    pub interrupted: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AckObligationReport {
    pub conversation_id: String,
    pub outcome: String,
    pub age_ms: Option<f64>,
    pub coalesced_count: Option<i64>,
    pub redrive_attempts: Option<i64>,
    pub time_to_first_visible_ack_ms: Option<f64>,
    pub interrupt_to_replacement_ack_ms: Option<f64>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingWakeReport {
    pub conversation_id: String,
    pub outcome: String,
    pub kind: Option<String>,
    pub work_id: Option<String>,
    pub age_ms: Option<f64>,
    pub reason: Option<String>,
    pub is_quiet_origin: Option<bool>,
}

fn put<T: ToString>(
    metadata: &mut BTreeMap<String, String>,
    key: &str,
    value: Option<T>,
) {
    if let Some(value) = value {
        metadata.insert(key.into(), value.to_string());
    }
}

pub fn send_dispatch_telemetry(report: &SendDispatchReport) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("conversation_id".into(), report.conversation_id.clone()),
        (
            "send_dispatch_host_ms".into(),
            report.host_dispatch_ms.round().to_string(),
        ),
        ("skew".into(), report.skew.clone()),
        ("is_fork".into(), report.is_fork.to_string()),
    ]);
    put(
        &mut metadata,
        "send_dispatch_ms",
        report.dispatch_ms.map(f64::round),
    );
    put(&mut metadata, "skew_reason", report.skew_reason.as_deref());
    put(&mut metadata, "skew_bucket", report.skew_bucket.as_deref());
    put(&mut metadata, "model_id", report.model_id.as_deref());
    put(&mut metadata, "trace_id", report.trace_id.as_deref());
    put(&mut metadata, "span_id", report.span_id.as_deref());
    HostTelemetryProjection {
        level: Some("info"),
        event: Some(SEND_DISPATCH_EVENT),
        metadata,
    }
}

pub fn queue_accepted_telemetry(report: &QueueAcceptedReport) -> HostTelemetryProjection {
    HostTelemetryProjection {
        level: Some("info"),
        event: Some(QUEUE_ACCEPTED_EVENT),
        metadata: BTreeMap::from([
            ("conversation_id".into(), report.conversation_id.clone()),
            ("lane".into(), report.lane.clone()),
            ("source".into(), report.source.clone()),
            ("position".into(), report.position.to_string()),
            ("depth_user".into(), report.depth_user.to_string()),
            ("depth_agent".into(), report.depth_agent.to_string()),
            ("depth_background".into(), report.depth_background.to_string()),
            ("has_active".into(), report.has_active.to_string()),
        ]),
    }
}

pub fn queue_dequeued_telemetry(report: &QueueDequeuedReport) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("conversation_id".into(), report.conversation_id.clone()),
        ("lane".into(), report.lane.clone()),
        ("source".into(), report.source.clone()),
        ("queue_wait_ms".into(), report.queue_wait_ms.round().to_string()),
        ("jumped_background".into(), report.jumped_background.to_string()),
        ("depth_user".into(), report.depth_user.to_string()),
        ("depth_agent".into(), report.depth_agent.to_string()),
        ("depth_background".into(), report.depth_background.to_string()),
    ]);
    put(
        &mut metadata,
        "accepted_to_run_ms",
        report.accepted_to_run_ms.map(f64::round),
    );
    HostTelemetryProjection {
        level: Some("info"),
        event: Some(QUEUE_DEQUEUED_EVENT),
        metadata,
    }
}

pub fn queue_watchdog_telemetry(report: &QueueWatchdogReport) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("conversation_id".into(), report.conversation_id.clone()),
        ("stage".into(), report.stage.clone()),
        (
            "active_runtime_ms".into(),
            report.active_runtime_ms.round().to_string(),
        ),
    ]);
    put(&mut metadata, "active_lane", report.active_lane.as_deref());
    put(&mut metadata, "active_source", report.active_source.as_deref());
    put(
        &mut metadata,
        "waiting_user_age_ms",
        report.waiting_user_age_ms.map(f64::round),
    );
    put(&mut metadata, "interrupted", report.interrupted);
    HostTelemetryProjection {
        level: Some(if report.stage == "late_settle" {
            "info"
        } else {
            "warn"
        }),
        event: Some(QUEUE_WATCHDOG_EVENT),
        metadata,
    }
}

pub fn ack_obligation_telemetry(report: &AckObligationReport) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("conversation_id".into(), report.conversation_id.clone()),
        ("outcome".into(), report.outcome.clone()),
    ]);
    put(&mut metadata, "age_ms", report.age_ms.map(f64::round));
    put(&mut metadata, "coalesced_count", report.coalesced_count);
    put(&mut metadata, "redrive_attempts", report.redrive_attempts);
    put(
        &mut metadata,
        "time_to_first_visible_ack_ms",
        report.time_to_first_visible_ack_ms.map(f64::round),
    );
    put(
        &mut metadata,
        "interrupt_to_replacement_ack_ms",
        report.interrupt_to_replacement_ack_ms.map(f64::round),
    );
    put(&mut metadata, "reason", report.reason.as_deref());
    HostTelemetryProjection {
        level: Some(if report.outcome == "lost" {
            "warn"
        } else {
            "info"
        }),
        event: Some(ACK_OBLIGATION_EVENT),
        metadata,
    }
}

pub fn pending_wake_telemetry(report: &PendingWakeReport) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("conversation_id".into(), report.conversation_id.clone()),
        ("outcome".into(), report.outcome.clone()),
    ]);
    put(&mut metadata, "kind", report.kind.as_deref());
    put(&mut metadata, "work_id", report.work_id.as_deref());
    put(&mut metadata, "age_ms", report.age_ms.map(f64::round));
    put(&mut metadata, "reason", report.reason.as_deref());
    put(&mut metadata, "quiet_origin", report.is_quiet_origin);
    HostTelemetryProjection {
        level: Some(
            if matches!(
                report.outcome.as_str(),
                "persist_failed" | "rearm_failed" | "pruned"
            ) {
                "warn"
            } else {
                "info"
            },
        ),
        event: Some(PENDING_WAKE_EVENT),
        metadata,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watchdog_late_settle_is_informational_but_stall_is_warning() {
        let base = QueueWatchdogReport {
            conversation_id: "agent-1".into(),
            stage: "late_settle".into(),
            active_lane: Some("user".into()),
            active_source: Some("composer".into()),
            active_runtime_ms: 12.6,
            waiting_user_age_ms: Some(3.4),
            interrupted: Some(false),
        };
        let settled = queue_watchdog_telemetry(&base);
        assert_eq!(settled.level, Some("info"));
        assert_eq!(settled.metadata.get("active_runtime_ms").map(String::as_str), Some("13"));
        assert_eq!(settled.metadata.get("waiting_user_age_ms").map(String::as_str), Some("3"));

        let mut stalled = base;
        stalled.stage = "active_stall".into();
        assert_eq!(queue_watchdog_telemetry(&stalled).level, Some("warn"));
    }

    #[test]
    fn lost_ack_is_warning_and_preserves_bounded_optional_fields() {
        let projection = ack_obligation_telemetry(&AckObligationReport {
            conversation_id: "agent-2".into(),
            outcome: "lost".into(),
            age_ms: Some(9.6),
            coalesced_count: Some(2),
            redrive_attempts: Some(1),
            time_to_first_visible_ack_ms: Some(12.2),
            interrupt_to_replacement_ack_ms: None,
            reason: Some("process_reopen".into()),
        });
        assert_eq!(projection.level, Some("warn"));
        assert_eq!(projection.metadata.get("age_ms").map(String::as_str), Some("10"));
        assert_eq!(projection.metadata.get("reason").map(String::as_str), Some("process_reopen"));
        assert!(!projection.metadata.contains_key("interrupt_to_replacement_ack_ms"));
    }

    #[test]
    fn pending_wake_failure_outcomes_are_warnings() {
        for outcome in ["persist_failed", "rearm_failed", "pruned"] {
            let projection = pending_wake_telemetry(&PendingWakeReport {
                conversation_id: "agent-3".into(),
                outcome: outcome.into(),
                kind: Some("shell".into()),
                work_id: Some("work-1".into()),
                age_ms: None,
                reason: None,
                is_quiet_origin: Some(true),
            });
            assert_eq!(projection.level, Some("warn"), "{outcome}");
            assert_eq!(projection.metadata.get("quiet_origin").map(String::as_str), Some("true"));
        }
    }

    #[test]
    fn send_dispatch_rounds_durations_and_omits_absent_tags() {
        let projection = send_dispatch_telemetry(&SendDispatchReport {
            conversation_id: "agent-4".into(),
            dispatch_ms: Some(4.6),
            host_dispatch_ms: 8.4,
            skew: "none".into(),
            skew_reason: None,
            skew_bucket: None,
            is_fork: false,
            model_id: Some("model-a".into()),
            trace_id: None,
            span_id: None,
        });
        assert_eq!(projection.metadata.get("send_dispatch_ms").map(String::as_str), Some("5"));
        assert_eq!(projection.metadata.get("send_dispatch_host_ms").map(String::as_str), Some("8"));
        assert!(!projection.metadata.contains_key("trace_id"));
        assert!(!projection.metadata.contains_key("span_id"));
    }
}
