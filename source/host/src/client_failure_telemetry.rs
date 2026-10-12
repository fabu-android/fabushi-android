use serde_json::Value;
use std::collections::BTreeMap;

pub const CLIENT_RESOURCE_EVENT: &str = "sand.client_resource";
pub const RENDERER_UNHANDLED_EVENT: &str = "sand.renderer_unhandled";
pub const RENDERER_BOUNDARY_CAUGHT_EVENT: &str = "sand.renderer.boundary_caught";
pub const ENTRYPOINT_ERROR_EVENT: &str = "sand.entrypoint_error";
pub const SEND_QUEUED_FLUSH_EVENT: &str = "sand.send.queued_flush";
pub const CLIENT_PERSISTENCE_EVENT: &str = "sand.client_persistence";
pub const CONNECTOR_AUTH_EVENT: &str = "sand.connector_auth";
pub const CONNECTOR_AUTH_CLICKED_SERVER_NAME_CAP: usize = 256;

const CLIENT_PERSISTENCE_SLICES: &[&str] = &[
    "client-meta.account-slot",
    "composer-drafts",
    "host-settings.onboarding",
    "roster.agent-avatars",
    "roster.last-roster",
    "selection.last-agent",
    "send-journal",
    "sidebar.last-sections",
    "transcript.replicas",
    "ui-agent-refs",
    "ui-layout",
    "other",
];
const IO_ERRNO_TAGS: &[&str] = &[
    "ECONNREFUSED", "ECONNRESET", "ECONNABORTED", "ETIMEDOUT", "EPIPE", "ENETRESET",
    "ENETDOWN", "ENETUNREACH", "EHOSTUNREACH", "EHOSTDOWN", "EAI_AGAIN", "ENOTFOUND",
    "EADDRINUSE", "EACCES", "EPERM", "ENOENT", "ENOSPC", "EDQUOT", "EROFS", "EBUSY",
    "EMFILE", "EIO", "E_OTHER",
];
const KNOWN_CONNECTORS: &[&str] = &[
    "asana", "atlassian", "buildkite", "confluence", "context7", "databricks", "datadog",
    "deepwiki", "dock", "figma", "filesystem", "github", "gmail", "google", "googlecalendar",
    "googledocs", "googledrive", "googlesheets", "googleworkspace", "huggingface", "jira",
    "linear", "memory", "notion", "playwright", "salesforce", "sentry", "sequentialthinking",
    "slack", "stripe", "telegram", "todoist", "zoominfo",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientFailureTelemetry {
    pub event: String,
    pub level: String,
    pub metadata: BTreeMap<String, String>,
}

fn object(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    value.as_object()
}

fn token(value: Option<&Value>) -> Option<&str> {
    let value = value?.as_str()?;
    if value.is_empty()
        || value.len() > 64
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'/' | b':' | b'|' | b'-')
        })
    {
        return None;
    }
    Some(value)
}

fn optional_token(value: Option<&Value>) -> bool {
    value.is_none() || token(value).is_some()
}

fn nonnegative(value: Option<&Value>) -> Option<f64> {
    let number = value?.as_f64()?;
    (number.is_finite() && number >= 0.0).then_some(number)
}

fn positive_integer(value: Option<&Value>) -> Option<u64> {
    value?.as_u64().filter(|value| *value >= 1)
}

fn error_code(value: Option<&Value>) -> Option<&str> {
    value?.as_object()?.get("code")?.as_str()
}

fn bounded_io_error(value: Option<&Value>) -> bool {
    let Some(error) = value.and_then(Value::as_object) else { return false };
    if error.get("code").and_then(Value::as_str) != Some("SAND-E0701") {
        return false;
    }
    error.get("errno").is_none_or(|errno| {
        errno.as_str().is_some_and(|errno| IO_ERRNO_TAGS.contains(&errno))
    })
}

pub fn is_valid_client_failure_report(value: &Value) -> bool {
    let Some(report) = object(value) else { return false };
    match report.get("kind").and_then(Value::as_str) {
        Some("client_resource") => {
            token(report.get("domain")).is_some()
                && token(report.get("operation")).is_some()
                && matches!(report.get("state").and_then(Value::as_str), Some("failed" | "recovered"))
                && token(report.get("failureCode")).is_some()
                && token(report.get("boundary")).is_some()
                && optional_token(report.get("retryOwner"))
                && optional_token(report.get("transportKind"))
                && report.get("failedMs").is_none_or(|value| nonnegative(Some(value)).is_some())
        }
        Some("renderer_unhandled") => {
            matches!(report.get("handler").and_then(Value::as_str), Some("error" | "unhandledrejection"))
                && token(report.get("errorClass")).is_some()
                && positive_integer(report.get("occurrence")).is_some()
        }
        Some("renderer_boundary_caught") => {
            matches!(
                report.get("boundary").and_then(Value::as_str),
                Some("app" | "chat_screen" | "info_pane" | "transcript_row" | "mermaid" | "other")
            ) && token(report.get("errorClass")).is_some()
                && positive_integer(report.get("occurrence")).is_some()
        }
        Some("entrypoint_error") => {
            token(report.get("entrypoint")).is_some()
                && matches!(report.get("surface").and_then(Value::as_str), Some("workspace" | "overlay" | "critical"))
                && matches!(report.get("phase").and_then(Value::as_str), Some("chunk_load" | "render"))
                && token(report.get("errorClass")).is_some()
        }
        Some("send_queued_flush") => {
            if nonnegative(report.get("queuedMs")).is_none() || nonnegative(report.get("queueDepth")).is_none() {
                return false;
            }
            match report.get("outcome").and_then(Value::as_str) {
                Some("delivered") => report.get("cause").is_none(),
                Some("superseded") => report.get("cause").and_then(Value::as_str) == Some("SAND-E0706"),
                Some("expired") => report.get("cause").and_then(Value::as_str) == Some("SAND-E0707"),
                Some("flush_failed") => matches!(
                    report.get("cause").and_then(Value::as_str),
                    Some("SAND-E0703" | "SAND-E0704" | "SAND-E0705")
                ),
                _ => false,
            }
        }
        Some("client_persistence") => {
            let Some(slice) = report.get("slice").and_then(Value::as_str) else { return false };
            if !CLIENT_PERSISTENCE_SLICES.contains(&slice)
                || report.get("bytes").is_some_and(|value| nonnegative(Some(value)).is_none())
            {
                return false;
            }
            match report.get("op").and_then(Value::as_str) {
                Some("writeback") => {
                    report.get("outcome").and_then(Value::as_str) == Some("ok")
                        && report.get("error").is_none()
                        && report.get("bytes").is_none()
                }
                Some("load") => match report.get("outcome").and_then(Value::as_str) {
                    Some("ok") => report.get("error").is_none(),
                    Some("corrupt") => error_code(report.get("error")) == Some("SAND-E0700"),
                    Some("io_error") => bounded_io_error(report.get("error")),
                    _ => false,
                },
                Some("persist") => match report.get("outcome").and_then(Value::as_str) {
                    Some("quota") => error_code(report.get("error")) == Some("SAND-E0702"),
                    Some("io_error") => bounded_io_error(report.get("error")),
                    _ => false,
                },
                _ => false,
            }
        }
        Some("connector_auth_clicked") => {
            report.get("serverName").is_none_or(|value| {
                value.as_str().is_some_and(|name| name.len() <= CONNECTOR_AUTH_CLICKED_SERVER_NAME_CAP)
            }) && optional_token(report.get("serverId"))
        }
        _ => false,
    }
}

fn rounded_string(value: f64) -> String {
    format!("{:.0}", value.round())
}

fn connector_tag(value: Option<&Value>) -> String {
    let Some(name) = value.and_then(Value::as_str) else { return "unknown".into() };
    let normalized: String = name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    if normalized.is_empty() {
        "unknown".into()
    } else if KNOWN_CONNECTORS.contains(&normalized.as_str()) {
        normalized
    } else {
        "other".into()
    }
}

fn insert_optional_string(metadata: &mut BTreeMap<String, String>, key: &str, value: Option<&Value>) {
    if let Some(value) = value.and_then(Value::as_str) {
        metadata.insert(key.into(), value.into());
    }
}

fn error_tags(error: Option<&Value>) -> BTreeMap<String, String> {
    let mut tags = BTreeMap::new();
    let record = error.and_then(Value::as_object);
    let code = record
        .and_then(|record| record.get("code"))
        .and_then(Value::as_str)
        .filter(|code| matches!(*code, "SAND-E0700" | "SAND-E0701" | "SAND-E0702"))
        .unwrap_or("SAND-E0001");
    tags.insert("error_code".into(), code.into());
    tags.insert("error_domain".into(), if code == "SAND-E0001" { "registry" } else { "storage" }.into());
    tags.insert("error_retryable".into(), (code == "SAND-E0701").to_string());
    if code == "SAND-E0701" {
        if let Some(errno) = record.and_then(|record| record.get("errno")).and_then(Value::as_str) {
            tags.insert("errno".into(), if IO_ERRNO_TAGS.contains(&errno) { errno } else { "E_OTHER" }.into());
        }
    }
    tags
}

pub fn client_failure_report_to_telemetry(value: &Value) -> Result<ClientFailureTelemetry, String> {
    if !is_valid_client_failure_report(value) {
        return Err("invalid client failure telemetry report".into());
    }
    let report = object(value).expect("validated report must be an object");
    let kind = report.get("kind").and_then(Value::as_str).unwrap_or_default();
    let mut metadata = BTreeMap::new();
    let (event, level) = match kind {
        "client_resource" => {
            for (source, target) in [("domain", "domain"), ("operation", "operation"), ("state", "state"), ("failureCode", "failure_code"), ("boundary", "boundary"), ("retryOwner", "retry_owner"), ("transportKind", "transport_kind")] {
                insert_optional_string(&mut metadata, target, report.get(source));
            }
            if let Some(value) = nonnegative(report.get("failedMs")) { metadata.insert("failed_ms".into(), rounded_string(value)); }
            (CLIENT_RESOURCE_EVENT, if report.get("state").and_then(Value::as_str) == Some("failed") { "warn" } else { "info" })
        }
        "renderer_unhandled" => {
            insert_optional_string(&mut metadata, "kind", report.get("handler"));
            insert_optional_string(&mut metadata, "error_class", report.get("errorClass"));
            metadata.insert("occurrence".into(), positive_integer(report.get("occurrence")).unwrap().to_string());
            (RENDERER_UNHANDLED_EVENT, "error")
        }
        "renderer_boundary_caught" => {
            insert_optional_string(&mut metadata, "boundary", report.get("boundary"));
            insert_optional_string(&mut metadata, "error_class", report.get("errorClass"));
            metadata.insert("occurrence".into(), positive_integer(report.get("occurrence")).unwrap().to_string());
            (RENDERER_BOUNDARY_CAUGHT_EVENT, "warn")
        }
        "entrypoint_error" => {
            for key in ["entrypoint", "surface", "phase"] { insert_optional_string(&mut metadata, key, report.get(key)); }
            insert_optional_string(&mut metadata, "error_class", report.get("errorClass"));
            (ENTRYPOINT_ERROR_EVENT, "warn")
        }
        "send_queued_flush" => {
            insert_optional_string(&mut metadata, "outcome", report.get("outcome"));
            metadata.insert("queued_ms".into(), rounded_string(nonnegative(report.get("queuedMs")).unwrap()));
            metadata.insert("queue_depth".into(), rounded_string(nonnegative(report.get("queueDepth")).unwrap()));
            insert_optional_string(&mut metadata, "cause", report.get("cause"));
            let level = if matches!(report.get("outcome").and_then(Value::as_str), Some("expired" | "flush_failed")) { "warn" } else { "info" };
            (SEND_QUEUED_FLUSH_EVENT, level)
        }
        "connector_auth_clicked" => {
            metadata.insert("phase".into(), "clicked".into());
            metadata.insert("connector".into(), connector_tag(report.get("serverName")));
            metadata.insert("outcome".into(), "ok".into());
            metadata.insert("surface".into(), "desktop".into());
            insert_optional_string(&mut metadata, "server_id", report.get("serverId"));
            (CONNECTOR_AUTH_EVENT, "info")
        }
        "client_persistence" => {
            for key in ["op", "outcome", "slice"] { insert_optional_string(&mut metadata, key, report.get(key)); }
            if let Some(value) = nonnegative(report.get("bytes")) { metadata.insert("bytes".into(), rounded_string(value)); }
            if report.get("error").is_some() { metadata.extend(error_tags(report.get("error"))); }
            (CLIENT_PERSISTENCE_EVENT, if report.get("outcome").and_then(Value::as_str) == Some("ok") { "info" } else { "warn" })
        }
        _ => unreachable!("validated report kind must be supported"),
    };
    Ok(ClientFailureTelemetry { event: event.into(), level: level.into(), metadata })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_and_normalizes_all_desktop_report_families() {
        let cases = [
            json!({"kind":"client_resource","domain":"host","operation":"connect","state":"failed","failureCode":"SAND-E0001","boundary":"jni","failedMs":12.6}),
            json!({"kind":"renderer_unhandled","handler":"error","errorClass":"TypeError","occurrence":2}),
            json!({"kind":"renderer_boundary_caught","boundary":"app","errorClass":"RenderError","occurrence":1}),
            json!({"kind":"entrypoint_error","entrypoint":"workspace","surface":"workspace","phase":"render","errorClass":"ChunkError"}),
            json!({"kind":"send_queued_flush","queuedMs":10,"queueDepth":2,"outcome":"flush_failed","cause":"SAND-E0704"}),
            json!({"kind":"client_persistence","slice":"send-journal","op":"load","outcome":"io_error","error":{"code":"SAND-E0701","errno":"ENOENT"},"bytes":42}),
            json!({"kind":"connector_auth_clicked","serverName":"GitHub","serverId":"17"}),
        ];
        for case in cases {
            assert!(is_valid_client_failure_report(&case), "case should validate: {case}");
            let normalized = client_failure_report_to_telemetry(&case).unwrap();
            assert!(normalized.event.starts_with("sand."));
            assert!(matches!(normalized.level.as_str(), "info" | "warn" | "error"));
        }
    }

    #[test]
    fn rejects_unbounded_or_unknown_shapes_and_preserves_error_taxonomy() {
        assert!(!is_valid_client_failure_report(&json!({"kind":"renderer_unhandled","handler":"panic","errorClass":"TypeError","occurrence":1})));
        assert!(!is_valid_client_failure_report(&json!({"kind":"send_queued_flush","queuedMs":1,"queueDepth":1,"outcome":"flush_failed","cause":"SAND-E9999"})));
        assert!(!is_valid_client_failure_report(&json!({"kind":"client_persistence","slice":"unknown-slice","op":"load","outcome":"ok"})));
        assert!(!is_valid_client_failure_report(&json!({"kind":"connector_auth_clicked","serverName":"x".repeat(CONNECTOR_AUTH_CLICKED_SERVER_NAME_CAP + 1)})));

        let report = json!({"kind":"client_persistence","slice":"send-journal","op":"load","outcome":"io_error","error":{"code":"SAND-E0701","errno":"NOT_A_REAL_ERRNO"}});
        assert!(!is_valid_client_failure_report(&report), "unknown errno must fail validation like Desktop");
    }

    #[test]
    fn connector_tag_is_bounded_to_known_other_or_unknown() {
        let known = client_failure_report_to_telemetry(&json!({"kind":"connector_auth_clicked","serverName":"Google Calendar","serverId":"a"})).unwrap();
        assert_eq!(known.metadata.get("connector").map(String::as_str), Some("googlecalendar"));
        let other = client_failure_report_to_telemetry(&json!({"kind":"connector_auth_clicked","serverName":"Private Connector"})).unwrap();
        assert_eq!(other.metadata.get("connector").map(String::as_str), Some("other"));
        let unknown = client_failure_report_to_telemetry(&json!({"kind":"connector_auth_clicked"})).unwrap();
        assert_eq!(unknown.metadata.get("connector").map(String::as_str), Some("unknown"));
    }
}
