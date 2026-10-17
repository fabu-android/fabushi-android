use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const AUTOMATION_LIFECYCLE_EVENT: &str = "sand.automation.lifecycle";

#[derive(Debug, Clone, PartialEq)]
pub struct AutomationLifecycleReport {
    pub conversation_id: String,
    pub automation_id: String,
    pub action: String,
    pub source: String,
    pub trigger_type: String,
    pub age_ms: f64,
    pub recorded_run_count: u64,
    pub scheduled_fires_next_7_days: Option<u64>,
    pub fires_on_weekend: Option<bool>,
    pub fires_overnight: Option<bool>,
}

pub fn automation_lifecycle_telemetry(
    report: &AutomationLifecycleReport,
) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("conversation_id".into(), report.conversation_id.clone()),
        ("automation_id".into(), report.automation_id.clone()),
        ("action".into(), report.action.clone()),
        ("source".into(), report.source.clone()),
        ("trigger_type".into(), report.trigger_type.clone()),
        ("age_ms".into(), report.age_ms.to_string()),
        (
            "recorded_run_count".into(),
            report.recorded_run_count.to_string(),
        ),
    ]);
    if let Some(value) = report.scheduled_fires_next_7_days {
        metadata.insert("scheduled_fires_next_7_days".into(), value.to_string());
    }
    if let Some(value) = report.fires_on_weekend {
        metadata.insert("fires_on_weekend".into(), value.to_string());
    }
    if let Some(value) = report.fires_overnight {
        metadata.insert("fires_overnight".into(), value.to_string());
    }
    HostTelemetryProjection {
        level: Some("info"),
        event: Some(AUTOMATION_LIFECYCLE_EVENT),
        metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_report_preserves_desktop_wire_projection() {
        let event = automation_lifecycle_telemetry(&AutomationLifecycleReport {
            conversation_id: "agent-a".into(),
            automation_id: "daily-brief".into(),
            action: "created".into(),
            source: "android".into(),
            trigger_type: "schedule".into(),
            age_ms: 12.0,
            recorded_run_count: 3,
            scheduled_fires_next_7_days: Some(7),
            fires_on_weekend: Some(true),
            fires_overnight: Some(false),
        });
        assert_eq!(event.event, Some(AUTOMATION_LIFECYCLE_EVENT));
        assert_eq!(event.metadata.get("conversation_id").map(String::as_str), Some("agent-a"));
        assert_eq!(event.metadata.get("recorded_run_count").map(String::as_str), Some("3"));
        assert_eq!(event.metadata.get("scheduled_fires_next_7_days").map(String::as_str), Some("7"));
    }
}
