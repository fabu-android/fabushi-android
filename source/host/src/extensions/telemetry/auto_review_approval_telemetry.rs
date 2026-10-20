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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_review_projection_clamps_rounds_and_preserves_optional_fields() {
        let projection = auto_review_approval_telemetry(&AutoReviewApprovalReport {
            event_type: "resolved".into(),
            conversation_id: "agent-1".into(),
            approval_id: "approval-7".into(),
            surface: "turn".into(),
            status: "approved".into(),
            age_ms: -2.4,
            ttl_ms: Some(1_234.6),
            cause: Some("user".into()),
        });
        assert_eq!(projection.level, None);
        assert_eq!(projection.event, Some("sand.auto_review.approval"));
        assert_eq!(projection.metadata.get("age_ms").map(String::as_str), Some("0"));
        assert_eq!(projection.metadata.get("ttl_ms").map(String::as_str), Some("1235"));
        assert_eq!(projection.metadata.get("cause").map(String::as_str), Some("user"));
        assert_eq!(
            projection.metadata.get("approval_id").map(String::as_str),
            Some("approval-7"),
        );
    }

    #[test]
    fn auto_review_projection_omits_absent_optional_fields() {
        let projection = auto_review_approval_telemetry(&AutoReviewApprovalReport {
            event_type: "expired".into(),
            conversation_id: "agent-2".into(),
            approval_id: "approval-8".into(),
            surface: "queue".into(),
            status: "expired".into(),
            age_ms: 10.0,
            ttl_ms: None,
            cause: None,
        });
        assert!(!projection.metadata.contains_key("ttl_ms"));
        assert!(!projection.metadata.contains_key("cause"));
    }
}
