use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const AUTOMATION_LATE_FIRE_THRESHOLD_MS: f64 = 5.0 * 60_000.0;
pub const AUTOMATION_RUN_EVENT: &str = "sand.automation.run";
pub const AUTOMATION_FIRE_DROPPED_EVENT: &str = "sand.automation.fire_dropped";

#[derive(Debug, Clone, PartialEq)]
pub struct AutomationRunReport {
    pub conversation_id: String,
    pub automation_id: String,
    pub trigger: String,
    pub outcome: String,
    pub is_group: bool,
    pub duration_ms: f64,
    pub lateness_ms: Option<f64>,
    pub scheduled_for_ms: Option<f64>,
    pub sent_message_count: Option<i64>,
    pub event_batch_size: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutomationFireDroppedReport {
    pub conversation_id: String,
    pub trigger: String,
    pub reason: String,
    pub scheduled_for_ms: Option<f64>,
    pub lateness_ms: Option<f64>,
    pub error_type: Option<String>,
    pub error_code: Option<String>,
    pub run_uuid: Option<String>,
    pub fire_age_ms: Option<f64>,
    pub has_definition_revision: Option<bool>,
    pub box_uptime_ms: Option<f64>,
}

fn insert_optional<T: ToString>(
    metadata: &mut BTreeMap<String, String>,
    key: &str,
    value: Option<T>,
) {
    if let Some(value) = value {
        metadata.insert(key.into(), value.to_string());
    }
}

pub fn automation_run_telemetry(report: &AutomationRunReport) -> HostTelemetryProjection {
    let is_late = report
        .lateness_ms
        .is_some_and(|value| value >= AUTOMATION_LATE_FIRE_THRESHOLD_MS);
    let mut metadata = BTreeMap::from([
        ("conversation_id".into(), report.conversation_id.clone()),
        ("automation_id".into(), report.automation_id.clone()),
        ("trigger".into(), report.trigger.clone()),
        ("outcome".into(), report.outcome.clone()),
        ("is_group".into(), report.is_group.to_string()),
        ("duration_ms".into(), report.duration_ms.to_string()),
    ]);
    insert_optional(&mut metadata, "lateness_ms", report.lateness_ms);
    insert_optional(&mut metadata, "scheduled_for_ms", report.scheduled_for_ms);
    if report.lateness_ms.is_some() {
        metadata.insert("late".into(), is_late.to_string());
    }
    insert_optional(
        &mut metadata,
        "sent_message_count",
        report.sent_message_count,
    );
    insert_optional(&mut metadata, "event_batch_size", report.event_batch_size);
    HostTelemetryProjection {
        level: Some(if report.outcome == "ok" && !is_late {
            "info"
        } else {
            "warn"
        }),
        event: Some(AUTOMATION_RUN_EVENT),
        metadata,
    }
}

pub fn automation_fire_dropped_telemetry(
    report: &AutomationFireDroppedReport,
) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("conversation_id".into(), report.conversation_id.clone()),
        ("trigger".into(), report.trigger.clone()),
        ("reason".into(), report.reason.clone()),
    ]);
    insert_optional(&mut metadata, "scheduled_for_ms", report.scheduled_for_ms);
    insert_optional(&mut metadata, "lateness_ms", report.lateness_ms);
    insert_optional(&mut metadata, "error_type", report.error_type.as_deref());
    insert_optional(&mut metadata, "error_code", report.error_code.as_deref());
    insert_optional(&mut metadata, "run_uuid", report.run_uuid.as_deref());
    insert_optional(&mut metadata, "fire_age_ms", report.fire_age_ms);
    insert_optional(
        &mut metadata,
        "has_definition_revision",
        report.has_definition_revision,
    );
    insert_optional(&mut metadata, "box_uptime_ms", report.box_uptime_ms);
    HostTelemetryProjection {
        level: Some("warn"),
        event: Some(AUTOMATION_FIRE_DROPPED_EVENT),
        metadata,
    }
}
