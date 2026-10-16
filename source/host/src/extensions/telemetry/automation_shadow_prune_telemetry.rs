use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const AUTOMATION_SHADOW_PRUNE_EVENT: &str = "sand.automation.shadow_prune";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomationShadowPruneReport {
    pub outcome: String,
    pub conversation_id: String,
    pub automation_id: String,
    pub local_definition_state: String,
    pub local_definition_count: i64,
    pub desired_count: i64,
    pub remote_shadow_count: i64,
    pub box_uptime_ms: Option<i64>,
}

pub fn automation_shadow_prune_telemetry(
    report: &AutomationShadowPruneReport,
) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("conversation_id".into(), report.conversation_id.clone()),
        ("automation_id".into(), report.automation_id.clone()),
        ("outcome".into(), report.outcome.clone()),
        (
            "local_definition_state".into(),
            report.local_definition_state.clone(),
        ),
        (
            "local_definition_count".into(),
            report.local_definition_count.to_string(),
        ),
        ("desired_count".into(), report.desired_count.to_string()),
        (
            "remote_shadow_count".into(),
            report.remote_shadow_count.to_string(),
        ),
    ]);
    if let Some(value) = report.box_uptime_ms {
        metadata.insert("box_uptime_ms".into(), value.to_string());
    }
    HostTelemetryProjection {
        level: Some(if report.outcome == "failed" {
            "warn"
        } else {
            "info"
        }),
        event: Some(AUTOMATION_SHADOW_PRUNE_EVENT),
        metadata,
    }
}
