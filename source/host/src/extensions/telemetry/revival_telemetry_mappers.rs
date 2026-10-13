use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const SUBAGENT_REVIVAL_EVENT: &str = "sand.subagent.revival";
pub const SHELL_REVIVAL_EVENT: &str = "sand.shell.revival";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentRevivalReport {
    pub parent_agent_id: String,
    pub outcome: String,
    pub completion_count: i64,
    pub subagent_type: Option<String>,
    pub subagent_agent_id: Option<String>,
    pub reason: Option<String>,
    pub sent_message_count: Option<i64>,
    pub is_quiet_origin: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellRevivalReport {
    pub conversation_id: String,
    pub outcome: String,
    pub completion_count: i64,
    pub sent_message_count: Option<i64>,
    pub is_quiet_origin: Option<bool>,
    pub reason: Option<String>,
}

fn optional_metadata(
    metadata: &mut BTreeMap<String, String>,
    key: &str,
    value: Option<impl ToString>,
) {
    if let Some(value) = value {
        metadata.insert(key.into(), value.to_string());
    }
}

pub fn subagent_revival_telemetry(
    report: &SubagentRevivalReport,
) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("conversation_id".into(), report.parent_agent_id.clone()),
        ("outcome".into(), report.outcome.clone()),
        ("completion_count".into(), report.completion_count.to_string()),
    ]);
    optional_metadata(&mut metadata, "subagent_type", report.subagent_type.as_deref());
    optional_metadata(
        &mut metadata,
        "subagent_agent_id",
        report.subagent_agent_id.as_deref(),
    );
    optional_metadata(&mut metadata, "reason", report.reason.as_deref());
    optional_metadata(
        &mut metadata,
        "sent_message_count",
        report.sent_message_count,
    );
    optional_metadata(&mut metadata, "quiet_origin", report.is_quiet_origin);
    HostTelemetryProjection {
        level: Some(
            if report.outcome == "dropped" && report.reason.as_deref() != Some("agent_deleted") {
                "warn"
            } else {
                "info"
            },
        ),
        event: Some(SUBAGENT_REVIVAL_EVENT),
        metadata,
    }
}

pub fn shell_revival_telemetry(report: &ShellRevivalReport) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("conversation_id".into(), report.conversation_id.clone()),
        ("outcome".into(), report.outcome.clone()),
        ("completion_count".into(), report.completion_count.to_string()),
    ]);
    optional_metadata(
        &mut metadata,
        "sent_message_count",
        report.sent_message_count,
    );
    optional_metadata(&mut metadata, "quiet_origin", report.is_quiet_origin);
    optional_metadata(&mut metadata, "reason", report.reason.as_deref());
    HostTelemetryProjection {
        level: Some(
            if report.outcome == "dropped" && report.reason.as_deref() != Some("agent_gone") {
                "warn"
            } else {
                "info"
            },
        ),
        event: Some(SHELL_REVIVAL_EVENT),
        metadata,
    }
}
