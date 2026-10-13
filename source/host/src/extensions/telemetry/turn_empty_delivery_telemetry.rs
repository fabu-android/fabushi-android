use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const TURN_EMPTY_DELIVERY_EVENT: &str = "sand.turn.empty_delivery";

#[derive(Debug, Clone, PartialEq)]
pub struct TurnEmptyDeliveryReport {
    pub conversation_id: String,
    pub request_id: Option<String>,
    pub source: String,
    pub request_source: Option<String>,
    pub reply_nudge_attempts: Option<i64>,
    pub redrive_attempts: Option<i64>,
    pub tool_call_count: i64,
    pub stream_output_produced: bool,
    pub duration_ms: f64,
    pub ack_outstanding: bool,
}

pub fn turn_empty_delivery_telemetry(
    report: &TurnEmptyDeliveryReport,
) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("conversation_id".into(), report.conversation_id.clone()),
        ("source".into(), report.source.clone()),
        ("tool_call_count".into(), report.tool_call_count.to_string()),
        (
            "stream_output_produced".into(),
            report.stream_output_produced.to_string(),
        ),
        ("duration_ms".into(), report.duration_ms.round().to_string()),
        ("ack_outstanding".into(), report.ack_outstanding.to_string()),
    ]);
    if let Some(value) = &report.request_id {
        metadata.insert("request_id".into(), value.clone());
    }
    if let Some(value) = &report.request_source {
        metadata.insert("request_source".into(), value.clone());
    }
    if let Some(value) = report.reply_nudge_attempts {
        metadata.insert("reply_nudge_attempts".into(), value.to_string());
    }
    if let Some(value) = report.redrive_attempts {
        metadata.insert("redrive_attempts".into(), value.to_string());
    }
    HostTelemetryProjection {
        level: Some("warn"),
        event: Some(TURN_EMPTY_DELIVERY_EVENT),
        metadata,
    }
}
