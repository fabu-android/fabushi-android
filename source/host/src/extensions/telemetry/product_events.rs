use std::collections::BTreeMap;

const ACCESS_BLOCKED_MS_CAP: u64 = 7 * 24 * 60 * 60 * 1_000;
const ACCESS_BLOCKED_CAUSES: [&str; 5] = [
    "wrong_account_suspected",
    "seat_revoked",
    "plan_expired",
    "no_plan",
    "unknown",
];
const ACCESS_BLOCK_REASONS: [&str; 10] = [
    "unspecified",
    "none",
    "teamPrivacyMode",
    "teamSetupRequired",
    "teamAccessRequired",
    "notOffered",
    "freeTrialAvailable",
    "paywallIndividual",
    "paywallTeamMember",
    "paywallTeamAdmin",
];
const AGENT_LOAD_OUTCOMES: [&str; 3] = ["ready", "superseded", "failed"];
const AGENT_LOAD_BOX_STATES: [&str; 7] = [
    "pulling",
    "starting",
    "running",
    "local",
    "sleeping",
    "off",
    "unknown",
];
const AGENTS_UNREACHABLE_PHASES: [&str; 3] = ["entered", "retry", "recovered"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryLevel {
    Info,
    Warn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuredTelemetryEvent {
    pub level: TelemetryLevel,
    pub metadata: BTreeMap<String, String>,
}

pub fn access_blocked_event(
    edge: &str,
    cause: &str,
    access_reason: &str,
    ms_blocked: Option<f64>,
) -> Result<StructuredTelemetryEvent, String> {
    if !matches!(edge, "entered" | "recovered") {
        return Err("access-blocked edge is invalid".into());
    }
    if !ACCESS_BLOCKED_CAUSES.contains(&cause) {
        return Err("access-blocked cause is invalid".into());
    }
    if !ACCESS_BLOCK_REASONS.contains(&access_reason) {
        return Err("access-blocked reason is invalid".into());
    }
    let bounded_ms = match ms_blocked {
        Some(value) if value.is_finite() && value >= 0.0 => {
            Some((value.round() as u64).min(ACCESS_BLOCKED_MS_CAP))
        }
        Some(_) => return Err("access-blocked duration must be finite and nonnegative".into()),
        None => None,
    };
    let mut metadata = BTreeMap::from([
        ("edge".into(), edge.into()),
        ("cause".into(), cause.into()),
        ("access_reason".into(), access_reason.into()),
    ]);
    if let Some(value) = bounded_ms {
        metadata.insert("ms_blocked".into(), value.to_string());
    }
    Ok(StructuredTelemetryEvent {
        level: if edge == "entered" && cause != "no_plan" {
            TelemetryLevel::Warn
        } else {
            TelemetryLevel::Info
        },
        metadata,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn agent_load_event(
    duration_ms: f64,
    open_rpc_ms: Option<f64>,
    queue_ms: Option<f64>,
    render_ms: Option<f64>,
    cold: bool,
    stale: Option<bool>,
    box_state: &str,
    outcome: &str,
) -> Result<StructuredTelemetryEvent, String> {
    for (name, value) in [
        ("durationMs", Some(duration_ms)),
        ("openRpcMs", open_rpc_ms),
        ("queueMs", queue_ms),
        ("renderMs", render_ms),
    ] {
        if let Some(value) = value {
            if !value.is_finite() || value < 0.0 {
                return Err(format!("{name} must be finite and nonnegative"));
            }
        }
    }
    if !AGENT_LOAD_OUTCOMES.contains(&outcome) {
        return Err("agent-load outcome is invalid".into());
    }
    let mut metadata = BTreeMap::from([
        ("duration_ms".into(), duration_ms.round().to_string()),
        ("cold".into(), cold.to_string()),
        (
            "box_state".into(),
            if AGENT_LOAD_BOX_STATES.contains(&box_state) {
                box_state
            } else {
                "other"
            }
            .into(),
        ),
        ("outcome".into(), outcome.into()),
    ]);
    for (key, value) in [
        ("open_rpc_ms", open_rpc_ms),
        ("queue_ms", queue_ms),
        ("render_ms", render_ms),
    ] {
        if let Some(value) = value {
            metadata.insert(key.into(), value.round().to_string());
        }
    }
    if let Some(stale) = stale {
        metadata.insert("stale".into(), stale.to_string());
    }
    Ok(StructuredTelemetryEvent {
        level: if outcome == "failed" {
            TelemetryLevel::Warn
        } else {
            TelemetryLevel::Info
        },
        metadata,
    })
}

pub fn agents_unreachable_event(
    phase: &str,
    is_manual: bool,
    error_class: Option<&str>,
    ms_since_last_reached: Option<f64>,
) -> Result<StructuredTelemetryEvent, String> {
    if !AGENTS_UNREACHABLE_PHASES.contains(&phase) {
        return Err("agents-unreachable phase is invalid".into());
    }
    if let Some(value) = ms_since_last_reached {
        if !value.is_finite() || value < 0.0 {
            return Err("agents-unreachable duration must be finite and nonnegative".into());
        }
    }
    let mut metadata = BTreeMap::from([
        ("phase".into(), phase.into()),
        ("is_manual".into(), is_manual.to_string()),
    ]);
    if let Some(error_class) = error_class {
        metadata.insert("error_class".into(), error_class.into());
    }
    if let Some(value) = ms_since_last_reached {
        metadata.insert("ms_since_last_reached".into(), value.round().to_string());
    }
    Ok(StructuredTelemetryEvent {
        level: if phase == "entered" {
            TelemetryLevel::Warn
        } else {
            TelemetryLevel::Info
        },
        metadata,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AndroidErrorCode {
    E0601,
    E0602,
    E0603,
    E0604,
    E0605,
    E0606,
    E0607,
    E0610,
}

impl AndroidErrorCode {
    fn wire(self) -> &'static str {
        match self {
            Self::E0601 => "SAND-E0601",
            Self::E0602 => "SAND-E0602",
            Self::E0603 => "SAND-E0603",
            Self::E0604 => "SAND-E0604",
            Self::E0605 => "SAND-E0605",
            Self::E0606 => "SAND-E0606",
            Self::E0607 => "SAND-E0607",
            Self::E0610 => "SAND-E0610",
        }
    }

    fn retryable(self) -> bool {
        matches!(self, Self::E0602 | Self::E0605 | Self::E0606 | Self::E0607)
    }

    fn allowed_fields(self) -> &'static [&'static str] {
        match self {
            Self::E0601 | Self::E0602 => &["phase"],
            Self::E0605 => &["process", "reason"],
            Self::E0606 => &["leg"],
            Self::E0607 => &["timeoutMs"],
            Self::E0603 | Self::E0604 | Self::E0610 => &[],
        }
    }
}

pub fn android_error_tags(
    code: AndroidErrorCode,
    payload: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut tags = BTreeMap::from([
        ("error_code".into(), code.wire().into()),
        ("error_domain".into(), "android".into()),
        ("error_retryable".into(), code.retryable().to_string()),
    ]);
    for field in code.allowed_fields() {
        let Some(value) = payload.get(*field) else { continue };
        if !bounded_tag_value(value) {
            continue;
        }
        let tag = match *field {
            "timeoutMs" => "timeout_ms",
            value => value,
        };
        tags.insert(tag.into(), value.clone());
    }
    tags
}

fn bounded_tag_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'.' | b'_' | b'|' | b':' | b'/' | b' ' | b'-')
        })
}

const CLIENT_RESOURCE_STATES: [&str; 2] = ["failed", "recovered"];
const CLIENT_PERSISTENCE_SLICES: [&str; 12] = [
    "client-meta.account-slot", "composer-drafts", "host-settings.onboarding",
    "roster.agent-avatars", "roster.last-roster", "selection.last-agent", "send-journal",
    "sidebar.last-sections", "transcript.replicas", "ui-agent-refs", "ui-layout", "other",
];
const CLIENT_IO_ERRNOS: [&str; 22] = [
    "ECONNREFUSED", "ECONNRESET", "ECONNABORTED", "ETIMEDOUT", "EPIPE", "ENETRESET",
    "ENETDOWN", "ENETUNREACH", "EHOSTUNREACH", "EHOSTDOWN", "EAI_AGAIN", "ENOTFOUND",
    "EADDRINUSE", "EACCES", "EPERM", "ENOENT", "ENOSPC", "EDQUOT", "EROFS", "EBUSY",
    "EMFILE", "EIO",
];

#[allow(clippy::too_many_arguments)]
pub fn client_resource_event(
    domain: &str,
    operation: &str,
    state: &str,
    failure_code: &str,
    boundary: &str,
    retry_owner: Option<&str>,
    transport_kind: Option<&str>,
    failed_ms: Option<f64>,
) -> Result<StructuredTelemetryEvent, String> {
    for (name, value) in [
        ("domain", Some(domain)),
        ("operation", Some(operation)),
        ("failureCode", Some(failure_code)),
        ("boundary", Some(boundary)),
        ("retryOwner", retry_owner),
        ("transportKind", transport_kind),
    ] {
        if value.is_some_and(|value| !bounded_token(value)) {
            return Err(format!("{name} must be a bounded token"));
        }
    }
    if !CLIENT_RESOURCE_STATES.contains(&state) {
        return Err("client-resource state is invalid".into());
    }
    if failed_ms.is_some_and(|value| !value.is_finite() || value < 0.0) {
        return Err("client-resource failed duration must be finite and nonnegative".into());
    }
    let mut metadata = BTreeMap::from([
        ("domain".into(), domain.into()),
        ("operation".into(), operation.into()),
        ("state".into(), state.into()),
        ("failure_code".into(), failure_code.into()),
        ("boundary".into(), boundary.into()),
    ]);
    for (key, value) in [("retry_owner", retry_owner), ("transport_kind", transport_kind)] {
        if let Some(value) = value { metadata.insert(key.into(), value.into()); }
    }
    if let Some(value) = failed_ms {
        metadata.insert("failed_ms".into(), value.round().to_string());
    }
    Ok(StructuredTelemetryEvent {
        level: if state == "failed" { TelemetryLevel::Warn } else { TelemetryLevel::Info },
        metadata,
    })
}

pub fn client_persistence_event(
    op: &str,
    outcome: &str,
    slice: &str,
    bytes: Option<f64>,
    error_code: Option<&str>,
    errno: Option<&str>,
) -> Result<StructuredTelemetryEvent, String> {
    if !CLIENT_PERSISTENCE_SLICES.contains(&slice) {
        return Err("client-persistence slice is invalid".into());
    }
    if bytes.is_some_and(|value| !value.is_finite() || value < 0.0) {
        return Err("client-persistence bytes must be finite and nonnegative".into());
    }
    let valid = match op {
        "writeback" => outcome == "ok" && error_code.is_none() && bytes.is_none(),
        "load" => match outcome {
            "ok" => error_code.is_none(),
            "corrupt" => error_code == Some("SAND-E0700"),
            "io_error" => error_code == Some("SAND-E0701"),
            _ => false,
        },
        "persist" => match outcome {
            "quota" => error_code == Some("SAND-E0702"),
            "io_error" => error_code == Some("SAND-E0701"),
            _ => false,
        },
        _ => false,
    };
    if !valid {
        return Err("client-persistence op/outcome/error combination is invalid".into());
    }
    if errno.is_some() && error_code != Some("SAND-E0701") {
        return Err("errno is only valid for SAND-E0701".into());
    }
    let mut metadata = BTreeMap::from([
        ("op".into(), op.into()),
        ("outcome".into(), outcome.into()),
        ("slice".into(), slice.into()),
    ]);
    if let Some(value) = bytes { metadata.insert("bytes".into(), value.round().to_string()); }
    if let Some(code) = error_code {
        metadata.insert("error_code".into(), code.into());
        metadata.insert("error_domain".into(), "storage".into());
        metadata.insert("error_retryable".into(), (code == "SAND-E0701").to_string());
        if code == "SAND-E0701" {
            if let Some(errno) = errno {
                metadata.insert("errno".into(), if CLIENT_IO_ERRNOS.contains(&errno) { errno.into() } else { "E_OTHER".into() });
            }
        }
    }
    Ok(StructuredTelemetryEvent {
        level: if outcome == "ok" { TelemetryLevel::Info } else { TelemetryLevel::Warn },
        metadata,
    })
}

const QUEUED_FLUSH_NONCE_MISMATCH: &str = "SAND-E0703";
const QUEUED_FLUSH_CAPABILITY_UNAVAILABLE: &str = "SAND-E0704";
const QUEUED_FLUSH_HOST_REJECTED: &str = "SAND-E0705";
const QUEUED_FLUSH_SUPERSEDED: &str = "SAND-E0706";
const QUEUED_FLUSH_ACK_EXPIRED: &str = "SAND-E0707";

pub fn send_queued_flush_event(
    outcome: &str,
    queued_ms: f64,
    queue_depth: f64,
    cause: Option<&str>,
) -> Result<StructuredTelemetryEvent, String> {
    if !queued_ms.is_finite() || queued_ms < 0.0 || !queue_depth.is_finite() || queue_depth < 0.0 {
        return Err("queued-flush durations and depth must be finite and nonnegative".into());
    }
    let valid = match outcome {
        "delivered" => cause.is_none(),
        "superseded" => cause == Some(QUEUED_FLUSH_SUPERSEDED),
        "expired" => cause == Some(QUEUED_FLUSH_ACK_EXPIRED),
        "flush_failed" => matches!(
            cause,
            Some(QUEUED_FLUSH_NONCE_MISMATCH)
                | Some(QUEUED_FLUSH_CAPABILITY_UNAVAILABLE)
                | Some(QUEUED_FLUSH_HOST_REJECTED)
        ),
        _ => false,
    };
    if !valid {
        return Err("queued-flush outcome/cause combination is invalid".into());
    }
    let mut metadata = BTreeMap::from([
        ("outcome".into(), outcome.into()),
        ("queued_ms".into(), queued_ms.round().to_string()),
        ("queue_depth".into(), queue_depth.round().to_string()),
    ]);
    if let Some(cause) = cause {
        metadata.insert("cause".into(), cause.into());
    }
    Ok(StructuredTelemetryEvent {
        level: if matches!(outcome, "expired" | "flush_failed") {
            TelemetryLevel::Warn
        } else {
            TelemetryLevel::Info
        },
        metadata,
    })
}

pub fn connector_auth_clicked_event(
    server_name: Option<&str>,
    server_id: Option<&str>,
) -> Result<StructuredTelemetryEvent, String> {
    if server_name.is_some_and(|value| value.len() > 256) {
        return Err("connector server name exceeds 256 characters".into());
    }
    if server_id.is_some_and(|value| !bounded_token(value)) {
        return Err("connector server id must be a bounded token".into());
    }
    let connector = connector_tag(server_name.unwrap_or_default());
    let mut metadata = BTreeMap::from([
        ("phase".into(), "clicked".into()),
        ("connector".into(), connector),
        ("outcome".into(), "ok".into()),
        ("surface".into(), "android".into()),
    ]);
    if let Some(server_id) = server_id {
        metadata.insert("server_id".into(), server_id.into());
    }
    Ok(StructuredTelemetryEvent {
        level: TelemetryLevel::Info,
        metadata,
    })
}

fn bounded_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'.' | b'_' | b'/' | b':' | b'|' | b'-')
        })
}

fn connector_tag(value: &str) -> String {
    const KNOWN: [&str; 38] = [
        "asana", "atlassian", "buildkite", "confluence", "context7", "databricks", "datadog",
        "deepwiki", "dock", "figma", "filesystem", "github", "gmail", "google",
        "googlecalendar", "googledocs", "googledrive", "googlesheets", "googleworkspace",
        "huggingface", "jira", "linear", "memory", "notion", "playwright", "salesforce", "sentry",
        "sequentialthinking", "slack", "stripe", "telegram", "todoist", "zoominfo", "dropbox",
        "box", "onedrive", "sharepoint", "calendar",
    ];
    let normalized: String = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    if normalized.is_empty() {
        "unknown".into()
    } else if KNOWN.contains(&normalized.as_str()) {
        normalized
    } else {
        "other".into()
    }
}

pub fn gateway_reachability_event(
    outcome: &str,
    method: &str,
    latency_ms: f64,
    base_url_kind: &str,
    http_status: Option<u16>,
    cause_summary: Option<&str>,
) -> Result<StructuredTelemetryEvent, String> {
    if !latency_ms.is_finite() || latency_ms < 0.0 {
        return Err("gateway latency must be finite and nonnegative".into());
    }
    let mut metadata = BTreeMap::from([
        ("outcome".into(), outcome.into()),
        ("method".into(), method.into()),
        ("latency_ms".into(), latency_ms.round().to_string()),
        ("base_url_kind".into(), base_url_kind.into()),
    ]);
    if let Some(status) = http_status {
        metadata.insert("http_status".into(), status.to_string());
    }
    if let Some(cause) = cause_summary {
        metadata.insert("cause".into(), cause.into());
    }
    Ok(StructuredTelemetryEvent {
        level: if outcome == "ok" { TelemetryLevel::Info } else { TelemetryLevel::Warn },
        metadata,
    })
}

pub fn dns_diagnostic_metadata(
    cluster: &str,
    trigger: &str,
    diagnosis: &str,
    system_exact: Option<&str>,
    independent_exact: Option<&str>,
    independent_wildcard: Option<&str>,
    independent_general: Option<&str>,
) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::from([
        ("cluster".into(), cluster.into()),
        ("trigger".into(), trigger.into()),
        ("diagnosis".into(), diagnosis.into()),
    ]);
    for (key, value) in [
        ("system_exact", system_exact),
        ("independent_exact", independent_exact),
        ("independent_wildcard", independent_wildcard),
        ("independent_general", independent_general),
    ] {
        if let Some(value) = value {
            metadata.insert(key.into(), value.into());
        }
    }
    metadata
}

pub fn reaction_ack_event(
    ack_ms: f64,
    outcome: &str,
    conversation_id: &str,
    emoji: &str,
) -> Result<StructuredTelemetryEvent, String> {
    if !ack_ms.is_finite() || ack_ms < 0.0 {
        return Err("reaction ack duration must be finite and nonnegative".into());
    }
    if !matches!(outcome, "landed" | "timeout") {
        return Err("reaction ack outcome is invalid".into());
    }
    if conversation_id.is_empty() || conversation_id.len() > 256 {
        return Err("reaction conversation id must contain 1..=256 bytes".into());
    }
    if emoji.is_empty() || emoji.len() > 64 {
        return Err("reaction emoji must contain 1..=64 bytes".into());
    }
    let reaction_id = match emoji {
        "👍" => "thumbs_up",
        "👎" => "thumbs_down",
        "❤️" => "heart",
        "😂" => "laugh",
        "🎉" => "celebrate",
        "😮" => "surprise",
        _ => "other",
    };
    Ok(StructuredTelemetryEvent {
        level: if outcome == "timeout" { TelemetryLevel::Warn } else { TelemetryLevel::Info },
        metadata: BTreeMap::from([
            ("ack_ms".into(), ack_ms.round().to_string()),
            ("outcome".into(), outcome.into()),
            ("conversation_id".into(), conversation_id.into()),
            ("reaction_id".into(), reaction_id.into()),
        ]),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn send_ack_event(
    ack_ms: f64,
    outcome: &str,
    enter_epoch_ms: f64,
    is_fork: bool,
    attachment_count: f64,
    trace_id: Option<&str>,
    span_id: Option<&str>,
    client_nonce: Option<&str>,
    commit_ms: Option<f64>,
    conversation_id: Option<&str>,
    client_online_at_timeout: Option<bool>,
) -> Result<StructuredTelemetryEvent, String> {
    for (name, value) in [
        ("ackMs", Some(ack_ms)),
        ("enterEpochMs", Some(enter_epoch_ms)),
        ("attachmentCount", Some(attachment_count)),
        ("commitMs", commit_ms),
    ] {
        if let Some(value) = value {
            if !value.is_finite() || value < 0.0 {
                return Err(format!("{name} must be finite and nonnegative"));
            }
        }
    }
    if !matches!(outcome, "acked" | "failed" | "timeout" | "queued_offline") {
        return Err("send ack outcome is invalid".into());
    }
    let mut metadata = BTreeMap::from([
        ("ack_ms".into(), ack_ms.round().to_string()),
        ("outcome".into(), outcome.into()),
        ("attachment_count".into(), attachment_count.round().to_string()),
        ("is_fork".into(), is_fork.to_string()),
    ]);
    for (key, value) in [
        ("trace_id", trace_id),
        ("span_id", span_id),
        ("client_nonce", client_nonce),
        ("conversation_id", conversation_id),
    ] {
        if let Some(value) = value {
            metadata.insert(key.into(), value.into());
        }
    }
    if let Some(value) = commit_ms {
        metadata.insert("commit_ms".into(), value.round().to_string());
    }
    if let Some(value) = client_online_at_timeout {
        metadata.insert("client_online_at_timeout".into(), value.to_string());
    }
    Ok(StructuredTelemetryEvent {
        level: if outcome == "timeout" { TelemetryLevel::Warn } else { TelemetryLevel::Info },
        metadata,
    })
}

pub fn send_latency_event(
    duration_ms: f64,
    commit_ms: Option<f64>,
    attachment_count: f64,
    is_fork: bool,
    trace_id: Option<&str>,
    span_id: Option<&str>,
    conversation_id: Option<&str>,
) -> Result<StructuredTelemetryEvent, String> {
    for (name, value) in [
        ("durationMs", Some(duration_ms)),
        ("commitMs", commit_ms),
        ("attachmentCount", Some(attachment_count)),
    ] {
        if let Some(value) = value {
            if !value.is_finite() || value < 0.0 {
                return Err(format!("{name} must be finite and nonnegative"));
            }
        }
    }
    let mut metadata = BTreeMap::from([
        ("duration_ms".into(), duration_ms.round().to_string()),
        ("attachment_count".into(), attachment_count.round().to_string()),
        ("is_fork".into(), is_fork.to_string()),
    ]);
    if let Some(value) = commit_ms {
        metadata.insert("commit_ms".into(), value.round().to_string());
    }
    for (key, value) in [
        ("trace_id", trace_id),
        ("span_id", span_id),
        ("conversation_id", conversation_id),
    ] {
        if let Some(value) = value {
            metadata.insert(key.into(), value.into());
        }
    }
    Ok(StructuredTelemetryEvent { level: TelemetryLevel::Info, metadata })
}

pub fn event_loop_pressure_event(
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
    window_ms: f64,
) -> Result<Option<StructuredTelemetryEvent>, String> {
    for (name, value) in [
        ("p50Ms", p50_ms),
        ("p95Ms", p95_ms),
        ("maxMs", max_ms),
        ("windowMs", window_ms),
    ] {
        if !value.is_finite() || value < 0.0 {
            return Err(format!("{name} must be finite and nonnegative"));
        }
    }
    let severity = if p95_ms >= 1_000.0 {
        "severe"
    } else if p95_ms >= 100.0 {
        "elevated"
    } else {
        return Ok(None);
    };
    let cap = |value: f64| value.round().clamp(0.0, 60_000.0).to_string();
    Ok(Some(StructuredTelemetryEvent {
        level: TelemetryLevel::Warn,
        metadata: BTreeMap::from([
            ("trigger".into(), "pressure".into()),
            ("severity".into(), severity.into()),
            ("p50_ms".into(), cap(p50_ms)),
            ("p95_ms".into(), cap(p95_ms)),
            ("max_ms".into(), cap(max_ms)),
            ("window_ms".into(), window_ms.round().to_string()),
        ]),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_blocked_validates_and_caps_duration() {
        let event = access_blocked_event(
            "entered",
            "seat_revoked",
            "teamAccessRequired",
            Some((ACCESS_BLOCKED_MS_CAP + 50_000) as f64),
        )
        .unwrap();
        assert_eq!(event.level, TelemetryLevel::Warn);
        assert_eq!(
            event.metadata.get("ms_blocked").cloned(),
            Some(ACCESS_BLOCKED_MS_CAP.to_string()),
        );
        assert!(access_blocked_event("bad", "seat_revoked", "teamAccessRequired", None).is_err());
        assert!(access_blocked_event("entered", "bad", "teamAccessRequired", None).is_err());
        assert!(access_blocked_event("entered", "seat_revoked", "bad", None).is_err());
    }

    #[test]
    fn agent_load_bounds_unknown_box_state_without_rejecting_it() {
        let event = agent_load_event(
            125.4,
            Some(5.2),
            None,
            Some(80.6),
            true,
            Some(false),
            "future-state",
            "ready",
        )
        .unwrap();
        assert_eq!(event.level, TelemetryLevel::Info);
        assert_eq!(event.metadata.get("duration_ms").map(String::as_str), Some("125"));
        assert_eq!(event.metadata.get("render_ms").map(String::as_str), Some("81"));
        assert_eq!(event.metadata.get("box_state").map(String::as_str), Some("other"));
        assert!(agent_load_event(-1.0, None, None, None, false, None, "local", "ready").is_err());
        assert!(agent_load_event(1.0, None, None, None, false, None, "local", "bad").is_err());
    }

    #[test]
    fn unreachable_event_keeps_desktop_warn_edge_and_nonnegative_contract() {
        let entered = agents_unreachable_event("entered", false, Some("timeout"), Some(12.6)).unwrap();
        assert_eq!(entered.level, TelemetryLevel::Warn);
        assert_eq!(
            entered.metadata.get("ms_since_last_reached").map(String::as_str),
            Some("13"),
        );
        assert_eq!(
            agents_unreachable_event("recovered", true, None, None)
                .unwrap()
                .level,
            TelemetryLevel::Info,
        );
        assert!(agents_unreachable_event("bad", false, None, None).is_err());
        assert!(agents_unreachable_event("retry", false, None, Some(-1.0)).is_err());
    }

    #[test]
    fn android_error_tags_preserve_desktop_code_table_and_redact_unbounded_payload() {
        let cases = [
            (AndroidErrorCode::E0601, "SAND-E0601", false),
            (AndroidErrorCode::E0602, "SAND-E0602", true),
            (AndroidErrorCode::E0603, "SAND-E0603", false),
            (AndroidErrorCode::E0604, "SAND-E0604", false),
            (AndroidErrorCode::E0605, "SAND-E0605", true),
            (AndroidErrorCode::E0606, "SAND-E0606", true),
            (AndroidErrorCode::E0607, "SAND-E0607", true),
            (AndroidErrorCode::E0610, "SAND-E0610", false),
        ];
        for (code, wire, retryable) in cases {
            let tags = android_error_tags(code, &BTreeMap::new());
            assert_eq!(tags.get("error_code").map(String::as_str), Some(wire));
            assert_eq!(tags.get("error_domain").map(String::as_str), Some("android"));
            assert_eq!(
                tags.get("error_retryable").map(String::as_str),
                Some(if retryable { "true" } else { "false" }),
            );
        }

        let payload = BTreeMap::from([
            ("process".into(), "host".into()),
            ("reason".into(), "timeout".into()),
            ("ignored".into(), "secret".into()),
        ]);
        let tags = android_error_tags(AndroidErrorCode::E0605, &payload);
        assert_eq!(tags.get("process").map(String::as_str), Some("host"));
        assert_eq!(tags.get("reason").map(String::as_str), Some("timeout"));
        assert!(!tags.contains_key("ignored"));

        let timeout = android_error_tags(
            AndroidErrorCode::E0607,
            &BTreeMap::from([("timeoutMs".into(), "12000".into())]),
        );
        assert_eq!(timeout.get("timeout_ms").map(String::as_str), Some("12000"));
        assert!(!timeout.contains_key("timeoutMs"));

        let unsafe_payload = BTreeMap::from([("phase".into(), "bad\\nvalue".into())]);
        let tags = android_error_tags(AndroidErrorCode::E0601, &unsafe_payload);
        assert!(!tags.contains_key("phase"));
    }


    #[test]
    fn client_resource_preserves_failure_and_recovery_contract() {
        let failed = client_resource_event("host", "open", "failed", "SAND-E0605", "android-host-jni", Some("coordinator"), Some("jni"), Some(12.6)).unwrap();
        assert_eq!(failed.level, TelemetryLevel::Warn);
        assert_eq!(failed.metadata.get("failed_ms").map(String::as_str), Some("13"));
        assert_eq!(client_resource_event("host", "open", "recovered", "SAND-E0605", "android-host-jni", None, None, None).unwrap().level, TelemetryLevel::Info);
        assert!(client_resource_event("bad value", "open", "failed", "SAND-E0605", "android-host-jni", None, None, None).is_err());
        assert!(client_resource_event("host", "open", "unknown", "SAND-E0605", "android-host-jni", None, None, None).is_err());
    }

    #[test]
    fn client_persistence_preserves_storage_error_contract() {
        let load = client_persistence_event("load", "io_error", "composer-drafts", Some(42.4), Some("SAND-E0701"), Some("ENOSPC")).unwrap();
        assert_eq!(load.level, TelemetryLevel::Warn);
        assert_eq!(load.metadata.get("bytes").map(String::as_str), Some("42"));
        assert_eq!(load.metadata.get("errno").map(String::as_str), Some("ENOSPC"));
        let unknown_errno = client_persistence_event("persist", "io_error", "send-journal", None, Some("SAND-E0701"), Some("ESECRET")).unwrap();
        assert_eq!(unknown_errno.metadata.get("errno").map(String::as_str), Some("E_OTHER"));
        assert!(client_persistence_event("load", "corrupt", "unknown-slice", None, Some("SAND-E0700"), None).is_err());
        assert!(client_persistence_event("writeback", "ok", "ui-layout", Some(1.0), None, None).is_err());
    }

    #[test]
    fn queued_flush_preserves_outcome_cause_contract() {
        let delivered = send_queued_flush_event("delivered", 12.4, 2.0, None).unwrap();
        assert_eq!(delivered.level, TelemetryLevel::Info);
        assert_eq!(delivered.metadata.get("queued_ms").map(String::as_str), Some("12"));
        let failed = send_queued_flush_event(
            "flush_failed",
            25.0,
            3.0,
            Some(QUEUED_FLUSH_HOST_REJECTED),
        )
        .unwrap();
        assert_eq!(failed.level, TelemetryLevel::Warn);
        assert!(send_queued_flush_event("delivered", 1.0, 1.0, Some("unexpected")).is_err());
        assert!(send_queued_flush_event("expired", -1.0, 1.0, Some(QUEUED_FLUSH_ACK_EXPIRED)).is_err());
    }

    #[test]
    fn connector_auth_clicked_bounds_identity_metadata() {
        let event = connector_auth_clicked_event(Some("GitHub"), Some("server-1")).unwrap();
        assert_eq!(event.metadata.get("connector").map(String::as_str), Some("github"));
        assert_eq!(event.metadata.get("surface").map(String::as_str), Some("android"));
        assert!(connector_auth_clicked_event(Some("GitHub"), Some("bad server")).is_err());
        assert!(connector_auth_clicked_event(Some(&"x".repeat(257)), None).is_err());
    }


    #[test]
    fn message_ack_events_preserve_timeout_and_reaction_contracts() {
        let reaction = reaction_ack_event(14.6, "timeout", "conversation-1", "👍").unwrap();
        assert_eq!(reaction.level, TelemetryLevel::Warn);
        assert_eq!(reaction.metadata.get("reaction_id").map(String::as_str), Some("thumbs_up"));
        assert!(reaction_ack_event(-1.0, "landed", "conversation-1", "👍").is_err());

        let ack = send_ack_event(
            50.2,
            "acked",
            1000.0,
            false,
            2.0,
            Some("trace"),
            Some("span"),
            Some("nonce"),
            Some(25.0),
            Some("conversation-1"),
            None,
        )
        .unwrap();
        assert_eq!(ack.level, TelemetryLevel::Info);
        assert_eq!(ack.metadata.get("attachment_count").map(String::as_str), Some("2"));
        assert!(send_ack_event(1.0, "bad", 1.0, false, 0.0, None, None, None, None, None, None).is_err());
    }

    #[test]
    fn gateway_and_send_latency_reject_negative_durations() {
        assert!(gateway_reachability_event("ok", "GET", -1.0, "api", None, None).is_err());
        let gateway = gateway_reachability_event("failed", "GET", 12.0, "api", Some(503), Some("timeout")).unwrap();
        assert_eq!(gateway.level, TelemetryLevel::Warn);
        assert_eq!(gateway.metadata.get("http_status").map(String::as_str), Some("503"));
        assert!(send_latency_event(-1.0, None, 0.0, false, None, None, None).is_err());
    }


    #[test]
    fn event_loop_pressure_matches_desktop_thresholds_and_caps() {
        assert!(event_loop_pressure_event(20.0, 99.9, 100.0, 60_000.0)
            .unwrap()
            .is_none());
        let elevated = event_loop_pressure_event(20.0, 100.0, 500.0, 60_000.0)
            .unwrap()
            .unwrap();
        assert_eq!(elevated.metadata.get("severity").map(String::as_str), Some("elevated"));
        let severe = event_loop_pressure_event(20.0, 1_000.0, 90_000.0, 60_000.0)
            .unwrap()
            .unwrap();
        assert_eq!(severe.metadata.get("severity").map(String::as_str), Some("severe"));
        assert_eq!(severe.metadata.get("max_ms").map(String::as_str), Some("60000"));
        assert!(event_loop_pressure_event(0.0, -1.0, 0.0, 60_000.0).is_err());
    }

}
