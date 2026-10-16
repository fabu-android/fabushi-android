use std::collections::BTreeMap;

use super::HostTelemetryProjection;
use super::sand_error_tags::{
    SandErrorPayloadValue, SandErrorValue, sand_error_tags,
};

pub const JOURNAL_OUTCOME_EVENT: &str = "sand.journal.outcome";

#[derive(Debug, Clone, PartialEq)]
pub struct JournalOutcomeReport {
    pub outcome: String,
    pub op: String,
    pub conversation_id: String,
    pub entry_count: Option<i64>,
    pub bytes: Option<i64>,
    pub duration_ms: f64,
    pub cause: Option<SandErrorValue>,
}

pub fn corrupt_tail_kind(cause: Option<&SandErrorValue>) -> Option<&SandErrorPayloadValue> {
    cause.and_then(|cause| cause.payload.get("tail"))
}

pub fn journal_outcome_level(report: &JournalOutcomeReport) -> &'static str {
    if report.outcome == "failed" {
        return "error";
    }
    if let Some(tail) = corrupt_tail_kind(report.cause.as_ref()) {
        if !matches!(tail, SandErrorPayloadValue::String(value) if value == "missing") {
            return "warn";
        }
    }
    "info"
}

pub fn journal_outcome_telemetry(report: &JournalOutcomeReport) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("op".into(), report.op.clone()),
        ("outcome".into(), report.outcome.clone()),
        ("conversation_id".into(), report.conversation_id.clone()),
        ("duration_ms".into(), report.duration_ms.round().to_string()),
    ]);
    if let Some(value) = report.entry_count {
        metadata.insert("entry_count".into(), value.to_string());
    }
    if let Some(value) = report.bytes {
        metadata.insert("bytes".into(), value.to_string());
    }
    if let Some(cause) = &report.cause {
        metadata.extend(sand_error_tags(cause));
    }
    HostTelemetryProjection {
        level: Some(journal_outcome_level(report)),
        event: Some(JOURNAL_OUTCOME_EVENT),
        metadata,
    }
}
