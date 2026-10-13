use std::collections::BTreeMap;

use super::HostTelemetryProjection;

#[derive(Debug, Clone, PartialEq)]
pub struct AutoReviewApprovalReport {
    pub event_type: String,
    pub conversation_id: String,
    pub approval_id: String,
    pub surface: String,
    pub status: String,
    pub age_ms: f64,
    pub ttl_ms: Option<f64>,
    pub cause: Option<String>,
}

fn non_negative_rounded(value: f64) -> String {
    value.round().max(0.0).to_string()
}

pub fn auto_review_approval_telemetry(
    report: &AutoReviewApprovalReport,
) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("event_type".into(), report.event_type.clone()),
        ("conversation_id".into(), report.conversation_id.clone()),
        ("approval_id".into(), report.approval_id.clone()),
        ("surface".into(), report.surface.clone()),
        ("status".into(), report.status.clone()),
        ("age_ms".into(), non_negative_rounded(report.age_ms)),
    ]);
    if let Some(value) = report.ttl_ms {
        metadata.insert("ttl_ms".into(), non_negative_rounded(value));
    }
    if let Some(value) = &report.cause {
        metadata.insert("cause".into(), value.clone());
    }
    HostTelemetryProjection {
        level: None,
        event: Some("sand.auto_review.approval"),
        metadata,
    }
}
