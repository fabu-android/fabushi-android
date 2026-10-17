use std::collections::BTreeMap;

use super::HostTelemetryProjection;
use super::sand_error_tags::{SandErrorValue, sand_error_tags};

pub const AGENT_ERROR_EVENT: &str = "sand.agent.error";
pub const AGENT_ERROR_DETAIL_EVENT: &str = "sand.agent.error.detail";
pub const MAX_ERROR_DETAIL_MESSAGE_LENGTH: usize = 1_024;
pub const MAX_ERROR_DETAIL_STACK_LENGTH: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentErrorDetail {
    pub message: String,
    pub stack: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentErrorReport {
    pub source: String,
    pub conversation_id: String,
    pub request_id: Option<String>,
    pub error: SandErrorValue,
    pub detail: Option<AgentErrorDetail>,
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_error_projection_uses_frozen_error_tags_and_bounded_identity() {
        let report = AgentErrorReport {
            source: "turn".into(),
            conversation_id: "agent-1".into(),
            request_id: Some("request-1".into()),
            error: SandErrorValue::new("SAND-E0401").with_string("connectCode", "ECONNRESET"),
            detail: None,
        };
        let projection = agent_error_telemetry(&report);
        assert_eq!(projection.level, Some("error"));
        assert_eq!(projection.event, Some(AGENT_ERROR_EVENT));
        assert_eq!(projection.metadata.get("conversation_id").map(String::as_str), Some("agent-1"));
        assert_eq!(projection.metadata.get("request_id").map(String::as_str), Some("request-1"));
        assert_eq!(projection.metadata.get("error_code").map(String::as_str), Some("SAND-E0401"));
        assert_eq!(projection.metadata.get("error_domain").map(String::as_str), Some("agent"));
        assert_eq!(projection.metadata.get("error_retryable").map(String::as_str), Some("true"));
    }

    #[test]
    fn detail_projection_truncates_message_and_stack_without_affecting_base_event() {
        let report = AgentErrorReport {
            source: "turn".into(),
            conversation_id: "agent-1".into(),
            request_id: None,
            error: SandErrorValue::new("unknown"),
            detail: Some(AgentErrorDetail {
                message: "m".repeat(MAX_ERROR_DETAIL_MESSAGE_LENGTH + 8),
                stack: Some("s".repeat(MAX_ERROR_DETAIL_STACK_LENGTH + 8)),
            }),
        };
        let detail = agent_error_detail_telemetry(&report).unwrap();
        assert_eq!(detail.event, Some(AGENT_ERROR_DETAIL_EVENT));
        assert_eq!(
            detail.metadata.get("error_message").unwrap().chars().count(),
            MAX_ERROR_DETAIL_MESSAGE_LENGTH,
        );
        assert_eq!(
            detail.metadata.get("error_stack").unwrap().chars().count(),
            MAX_ERROR_DETAIL_STACK_LENGTH,
        );
        assert_eq!(detail.metadata.get("error_code").map(String::as_str), Some("SAND-E0001"));
    }
}
