use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const GATEWAY_COMMAND_ERROR_EVENT: &str = "sand.gateway_command_error";
pub const GATEWAY_COMMAND_TIMING_EVENT: &str = "sand.gateway_command_timing";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidGatewayCommandReport {
    pub method: String,
    pub duration_ms: u64,
    pub request_id: Option<String>,
    pub reason: Option<String>,
    pub error_class: Option<String>,
}


pub fn classify_android_gateway_command_error(error: &str) -> Option<(&'static str, &'static str)> {
    if error.starts_with("unknown host method ")
        || error.contains(" is required")
        || error.contains(" must ")
        || error.contains("invalid")
        || error.contains("out of range")
        || error.contains("stale")
        || error.contains("fence")
    {
        // Desktop gateway reports these as 4xx and intentionally does not emit
        // sand.gateway_command_error. Keep expected policy/input rejections out
        // of the server-failure telemetry channel on Android as well.
        None
    } else {
        Some(("command-failed", "CommandError"))
    }
}

fn base_metadata(report: &AndroidGatewayCommandReport) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::from([
        ("method".into(), report.method.chars().take(128).collect()),
        ("duration_ms".into(), report.duration_ms.to_string()),
    ]);
    if let Some(value) = report.request_id.as_deref().filter(|value| !value.is_empty()) {
        metadata.insert("request_id".into(), value.chars().take(128).collect());
    }
    metadata
}

pub fn gateway_command_timing_telemetry(
    report: &AndroidGatewayCommandReport,
) -> HostTelemetryProjection {
    HostTelemetryProjection {
        level: Some("info"),
        event: Some(GATEWAY_COMMAND_TIMING_EVENT),
        metadata: base_metadata(report),
    }
}

pub fn gateway_command_error_telemetry(
    report: &AndroidGatewayCommandReport,
) -> HostTelemetryProjection {
    let mut metadata = base_metadata(report);
    if let Some(value) = report.reason.as_deref().filter(|value| !value.is_empty()) {
        metadata.insert("reason".into(), value.chars().take(128).collect());
    }
    if let Some(value) = report.error_class.as_deref().filter(|value| !value.is_empty()) {
        metadata.insert("error_class".into(), value.chars().take(64).collect());
    }
    HostTelemetryProjection {
        level: Some("error"),
        event: Some(GATEWAY_COMMAND_ERROR_EVENT),
        metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_gateway_projection_preserves_desktop_wire_fields() {
        let report = AndroidGatewayCommandReport {
            method: "feature.auth.status".into(),
            duration_ms: 17,
            request_id: Some("request-1".into()),
            reason: None,
            error_class: None,
        };
        let timing = gateway_command_timing_telemetry(&report);
        assert_eq!(timing.event, Some(GATEWAY_COMMAND_TIMING_EVENT));
        assert_eq!(timing.metadata.get("method").map(String::as_str), Some("feature.auth.status"));
        assert_eq!(timing.metadata.get("duration_ms").map(String::as_str), Some("17"));
        assert_eq!(timing.metadata.get("request_id").map(String::as_str), Some("request-1"));

        let failed = gateway_command_error_telemetry(&AndroidGatewayCommandReport {
            reason: Some("command-rejected".into()),
            error_class: Some("CommandError".into()),
            ..report
        });
        assert_eq!(failed.event, Some(GATEWAY_COMMAND_ERROR_EVENT));
        assert_eq!(failed.level, Some("error"));
        assert_eq!(failed.metadata.get("reason").map(String::as_str), Some("command-rejected"));
        assert_eq!(failed.metadata.get("error_class").map(String::as_str), Some("CommandError"));
    }

    #[test]
    fn expected_rejections_do_not_become_gateway_server_errors() {
        for error in [
            "unknown host method feature.nope",
            "agentId is required",
            "limit must be positive",
            "invalid approval",
            "value out of range",
            "account fence is stale",
        ] {
            assert_eq!(classify_android_gateway_command_error(error), None, "{error}");
        }
        assert_eq!(
            classify_android_gateway_command_error("backend transport failed"),
            Some(("command-failed", "CommandError")),
        );
        assert_eq!(
            classify_android_gateway_command_error("account telemetry storage backend failed"),
            Some(("command-failed", "CommandError")),
            "internal account-related failures must not be hidden as expected client rejection",
        );
    }

    #[test]
    fn metadata_is_bounded_before_structured_log_persistence() {
        let projection = gateway_command_error_telemetry(&AndroidGatewayCommandReport {
            method: "x".repeat(512),
            duration_ms: 1,
            request_id: Some("r".repeat(512)),
            reason: Some("e".repeat(512)),
            error_class: Some("c".repeat(512)),
        });
        assert_eq!(projection.metadata["method"].chars().count(), 128);
        assert_eq!(projection.metadata["request_id"].chars().count(), 128);
        assert_eq!(projection.metadata["reason"].chars().count(), 128);
        assert_eq!(projection.metadata["error_class"].chars().count(), 64);
    }
}
