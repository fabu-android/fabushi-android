use std::collections::BTreeMap;

use crate::extensions::transcript::automation_runtime::AutomationLifecycleEvent;

use super::HostTelemetryProjection;

pub const AUTOMATION_LIFECYCLE_EVENT: &str = "sand.automation.lifecycle";

pub fn automation_lifecycle_telemetry(
    report: &AutomationLifecycleEvent,
) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("conversation_id".into(), report.agent_id.clone()),
        ("automation_id".into(), report.stable_automation_id.clone()),
        ("action".into(), report.action.as_str().into()),
        ("source".into(), report.source.as_str().into()),
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
