use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const SEARCH_INDEX_HEALTH_EVENT: &str = "sand.search_index.health";
pub const SEARCH_INDEX_WARN_KINDS: &[&str] =
    &["dispose_drain_cut", "worker_terminate_failed", "job_retry"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchIndexHealthReport {
    pub kind: String,
    pub stage: Option<String>,
    pub error_class: Option<String>,
    pub count: Option<i64>,
}

pub fn search_index_health_telemetry(
    report: &SearchIndexHealthReport,
) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([("kind".into(), report.kind.clone())]);
    if let Some(value) = &report.stage {
        metadata.insert("stage".into(), value.clone());
    }
    if let Some(value) = &report.error_class {
        metadata.insert("error_class".into(), value.clone());
    }
    if let Some(value) = report.count {
        metadata.insert("count".into(), value.to_string());
    }
    HostTelemetryProjection {
        level: Some(if SEARCH_INDEX_WARN_KINDS.contains(&report.kind.as_str()) {
            "warn"
        } else {
            "error"
        }),
        event: Some(SEARCH_INDEX_HEALTH_EVENT),
        metadata,
    }
}
