use std::collections::BTreeMap;

use crate::ports::telemetry::SandErrorDetail;

use super::HostTelemetryProjection;
use super::sand_error_tags::{SandErrorValue, sand_error_tags};

pub const AGENT_ERROR_EVENT: &str = "sand.agent.error";
pub const AGENT_ERROR_DETAIL_EVENT: &str = "sand.agent.error.detail";
pub const MAX_ERROR_DETAIL_MESSAGE_LENGTH: usize = 1_024;
pub const MAX_ERROR_DETAIL_STACK_LENGTH: usize = 4_096;

#[derive(Debug, Clone, PartialEq)]
pub struct AgentErrorReport {
    pub source: String,
    pub conversation_id: String,
    pub request_id: Option<String>,
    pub error: SandErrorValue,
    pub detail: Option<SandErrorDetail>,
}

pub fn agent_error_telemetry(report: &AgentErrorReport) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("source".to_string(), report.source.clone()),
        ("conversation_id".to_string(), report.conversation_id.clone()),
    ]);
    if let Some(request_id) = report
        .request_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("request_id".into(), request_id.to_string());
    }
    metadata.extend(sand_error_tags(&report.error));
    HostTelemetryProjection {
        level: Some("error"),
        event: Some(AGENT_ERROR_EVENT),
        metadata,
    }
}

pub fn agent_error_detail_telemetry(
    report: &AgentErrorReport,
) -> Option<HostTelemetryProjection> {
    let detail = report.detail.as_ref()?;
    let mut metadata = BTreeMap::from([
        ("source".to_string(), report.source.clone()),
        ("conversation_id".to_string(), report.conversation_id.clone()),
        (
            "error_code".to_string(),
            sand_error_tags(&report.error)
                .get("error_code")
                .cloned()
                .unwrap_or_else(|| "SAND-E0001".to_string()),
        ),
        (
            "error_message".to_string(),
            truncate(&detail.message, MAX_ERROR_DETAIL_MESSAGE_LENGTH),
        ),
    ]);
    if let Some(request_id) = report
        .request_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("request_id".into(), request_id.to_string());
    }
    if let Some(stack) = detail.stack.as_deref() {
        metadata.insert(
            "error_stack".into(),
            truncate(stack, MAX_ERROR_DETAIL_STACK_LENGTH),
        );
    }
    Some(HostTelemetryProjection {
        level: Some("error"),
        event: Some(AGENT_ERROR_DETAIL_EVENT),
        metadata,
    })
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}
