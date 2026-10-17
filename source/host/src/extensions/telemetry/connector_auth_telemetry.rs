use std::collections::BTreeMap;

use super::HostTelemetryProjection;
use super::sand_error_tags::{SandErrorValue, sand_error_tags};

pub const CONNECTOR_AUTH_EVENT: &str = "sand.connector_auth";

pub const KNOWN_CONNECTOR_TAGS: &[&str] = &[
    "asana", "atlassian", "buildkite", "confluence", "context7", "databricks", "datadog",
    "deepwiki", "dock", "figma", "filesystem", "github", "gmail", "google", "googlecalendar",
    "googledocs", "googledrive", "googlesheets", "googleworkspace", "huggingface", "jira",
    "linear", "memory", "notion", "playwright", "salesforce", "sentry", "sequentialthinking",
    "slack", "stripe", "telegram", "todoist", "zoominfo",
];

#[derive(Debug, Clone, PartialEq)]
pub struct ConnectorAuthReport {
    pub phase: String,
    pub outcome: String,
    pub server_name: Option<String>,
    pub server_id: Option<String>,
    pub reauth: Option<bool>,
    pub error: Option<SandErrorValue>,
}

pub fn bounded_connector_tag(server_name: Option<&str>) -> String {
    let Some(server_name) = server_name else {
        return "unknown".into();
    };
    let normalized = server_name
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
        .collect::<String>();
    if normalized.is_empty() {
        "unknown".into()
    } else if KNOWN_CONNECTOR_TAGS.contains(&normalized.as_str()) {
        normalized
    } else {
        "other".into()
    }
}

fn branded_id(value: Option<&str>) -> Option<String> {
    let value = value?;
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'|' | b'-')
        })
    {
        return None;
    }
    Some(value.to_string())
}

pub fn connector_auth_telemetry(
    report: &ConnectorAuthReport,
    surface: &'static str,
) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("phase".into(), report.phase.clone()),
        ("connector".into(), bounded_connector_tag(report.server_name.as_deref())),
        ("outcome".into(), report.outcome.clone()),
        ("surface".into(), surface.to_string()),
    ]);
    if let Some(server_id) = branded_id(report.server_id.as_deref()) {
        metadata.insert("server_id".into(), server_id);
    }
    if let Some(reauth) = report.reauth {
        metadata.insert("reauth".into(), reauth.to_string());
    }
    if let Some(error) = report.error.as_ref() {
        metadata.extend(sand_error_tags(error));
    }
    HostTelemetryProjection {
        level: Some(if matches!(report.outcome.as_str(), "failed" | "timeout") {
            "warn"
        } else {
            "info"
        }),
        event: Some(CONNECTOR_AUTH_EVENT),
        metadata,
    }
}
