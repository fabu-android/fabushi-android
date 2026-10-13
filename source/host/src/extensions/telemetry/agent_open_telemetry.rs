use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const AGENT_OPEN_EVENT: &str = "sand.agent.open";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentOpenReport {
    pub conversation_id: String,
    pub duration_ms: u64,
    pub entry_count: usize,
    pub was_active: bool,
}

pub fn agent_open_telemetry(report: &AgentOpenReport) -> HostTelemetryProjection {
    HostTelemetryProjection {
        level: Some("info"),
        event: Some(AGENT_OPEN_EVENT),
        metadata: BTreeMap::from([
            ("conversation_id".into(), report.conversation_id.clone()),
            ("duration_ms".into(), report.duration_ms.to_string()),
            ("entry_count".into(), report.entry_count.to_string()),
            ("was_active".into(), report.was_active.to_string()),
        ]),
    }
}
