use chrono::DateTime;
use serde_json::{Value, json};
use url::Url;

use crate::extensions::trays::trays_service::PushErrorOptions;

pub const PROVIDER_OVERLOAD_ERROR_TITLE: &str = "Model provider is overloaded";
pub const INTRODUCTION_FAILED_TRAY_TITLE: &str = "Your agent couldn't introduce itself";
pub const PROVIDER_OVERLOAD_ERROR_DETAIL: &str =
    "The model provider is under heavy load right now. This is usually temporary — retry, or switch to another model.";
pub const SAND_INCLUDED_LIMIT_REASON: &str = "sand_included_limit";
pub const MAX_TRAY_ACTIONS: usize = 3;
pub const CURSOR_WEBSITE_ORIGIN: &str = "https://cursor.com";
pub const SUPPORTED_DASHBOARD_ACTION_VERBS: &[&str] = &["requestLimitIncrease"];

const AUTH_REQUIRED_ERROR_CODES: &[&str] = &[
    "NOT_LOGGED_IN", "AGENT_REQUIRES_LOGIN", "AUTH_TOKEN_NOT_FOUND", "AUTH_TOKEN_EXPIRED",
    "INVALID_AUTH_ID", "UNAUTHORIZED", "GITHUB_NO_USER_CREDENTIALS", "GITHUB_USER_NO_ACCESS",
];
const UPGRADE_REQUIRED_ERROR_CODES: &[&str] = &[
    "FREE_USER_USAGE_LIMIT", "FREE_USER_RATE_LIMIT_EXCEEDED", "PRO_USER_ONLY",
    "PRO_USER_USAGE_LIMIT", "PRO_USER_RATE_LIMIT_EXCEEDED", "RATE_LIMITED",
    "RATE_LIMITED_CHANGEABLE", "GENERIC_RATE_LIMIT_EXCEEDED",
];
const PAYMENT_REQUIRED_ERROR_CODES: &[&str] = &["USAGE_PRICING_REQUIRED", "USAGE_PRICING_REQUIRED_CHANGEABLE"];
const CONFIG_REQUIRED_ERROR_CODES: &[&str] = &["BAD_API_KEY", "BAD_USER_API_KEY", "OUTDATED_CLIENT"];
const CANCELLED_ERROR_CODES: &[&str] = &["USER_ABORTED_REQUEST", "DEBOUNCED"];
const CONNECT_CODE_CANCELLED: u16 = 1;
const CONNECT_CODE_ABORTED: u16 = 10;
const CONNECT_CODE_UNAUTHENTICATED: u16 = 16;
const DESKTOP_TRANSPORT_ERROR_TOKENS: &[&str] = &[
    "nghttp2", "econnreset", "econnrefused", "etimedout", "epipe", "socket hang up",
    "premature close", "err_stream", "protocol error", "http/2 stream",
    "err_http2_session_error", "session closed with error code", "connection aborted",
    "enotfound", "eai_again", "eai_fail", "enodata", "eservfail", "ehostunreach",
    "enetdown", "enetunreach", "network error:",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunErrorClass { Retriable, ActionRequired, NonRetriable, Cancelled }

impl AgentRunErrorClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Retriable => "retriable",
            Self::ActionRequired => "action-required",
            Self::NonRetriable => "non-retriable",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunErrorDisplayInfo {
    pub title: Option<String>,
    pub detail: Option<String>,
    pub is_retryable: Option<bool>,
    pub connect_code: Option<u16>,
    pub error_code: Option<String>,
    pub inference_request_error_type: Option<String>,
    pub error_details: Option<Value>,
}
impl AgentRunErrorDisplayInfo {
    pub fn to_json(&self) -> Value {
        json!({
            "title": self.title,
            "detail": self.detail,
            "isRetryable": self.is_retryable,
            "connectCode": self.connect_code,
            "errorCode": self.error_code,
            "inferenceRequestErrorType": self.inference_request_error_type,
            "errorDetails": self.error_details,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunErrorClassification {
    pub class: AgentRunErrorClass,
    pub request_id: Option<String>,
    pub display_info: AgentRunErrorDisplayInfo,
    pub action: Option<String>,
    pub is_transport: bool,
}

fn code_matches(code: Option<&str>, set: &[&str]) -> bool {
    code.is_some_and(|code| set.iter().any(|candidate| *candidate == code))
}
fn first_backend_error_detail_value(error: &BackendConnectErrorEnvelope) -> Option<&Value> {
    find_backend_connect_error(error, true)?.details.first()
}
fn error_detail_contract_parts(value: &Value) -> (&Value, &Value) {
    let outer = value.get("details").filter(|value| value.is_object()).unwrap_or(value);
    let nested = outer.get("details").filter(|value| value.is_object()).unwrap_or(outer);
    (outer, nested)
}
fn find_inference_request_error_type(error: &BackendConnectErrorEnvelope) -> Option<String> {
    if let Some(value) = error.inference_request_error_type.as_ref() { return Some(value.clone()); }
    if let Some(cause) = error.cause.as_deref() {
        if let Some(value) = find_inference_request_error_type(cause) { return Some(value); }
    }
    error.errors.iter().find_map(find_inference_request_error_type)
}
pub fn message_matches_agent_transport(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    DESKTOP_TRANSPORT_ERROR_TOKENS.iter().any(|token| lower.contains(token))
}
pub fn classify_agent_run_error(
    message: &str, request_id: Option<&str>, is_transport_hint: bool,
) -> AgentRunErrorClassification {
    let root = parse_backend_connect_error(message);
    let raw_error_details = root.as_ref().and_then(first_backend_error_detail_value).cloned();
    let (error_code, explicit_retryable, action_required) = raw_error_details.as_ref().map(|value| {
        let (outer, nested) = error_detail_contract_parts(value);
        let error_code = outer.get("error").or_else(|| nested.get("error"))
            .and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty()).map(str::to_string);
        let explicit_retryable = nested.get("isRetryable").or_else(|| outer.get("isRetryable")).and_then(Value::as_bool);
        let action_required = nested.get("analyticsMetadata").or_else(|| outer.get("analyticsMetadata"))
            .and_then(Value::as_object).and_then(|metadata| metadata.get("actionRequired"))
            .and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty()).map(str::to_string);
        (error_code, explicit_retryable, action_required)
    }).unwrap_or((None, None, None));
    let connect_code = root.as_ref().and_then(|value| value.code);
    let is_transport = is_transport_hint || message_matches_agent_transport(message);
    let lower = message.to_ascii_lowercase();
    let blob_not_found = lower.contains("blob not found:") || lower.contains("blobnotfounderror");
    let backend_detail = backend_detail_from_error_message(message);
    let (class, action) = if let Some(action) = action_required.clone() {
        (AgentRunErrorClass::ActionRequired, Some(action))
    } else if code_matches(error_code.as_deref(), AUTH_REQUIRED_ERROR_CODES) || connect_code == Some(CONNECT_CODE_UNAUTHENTICATED) {
        (AgentRunErrorClass::ActionRequired, Some("login".into()))
    } else if code_matches(error_code.as_deref(), UPGRADE_REQUIRED_ERROR_CODES) {
        (AgentRunErrorClass::ActionRequired, Some("upgrade".into()))
    } else if code_matches(error_code.as_deref(), PAYMENT_REQUIRED_ERROR_CODES) {
        (AgentRunErrorClass::ActionRequired, Some("payment".into()))
    } else if code_matches(error_code.as_deref(), CONFIG_REQUIRED_ERROR_CODES) {
        (AgentRunErrorClass::ActionRequired, Some("config".into()))
    } else if code_matches(error_code.as_deref(), CANCELLED_ERROR_CODES) {
        (AgentRunErrorClass::Cancelled, None)
    } else if connect_code == Some(CONNECT_CODE_CANCELLED) || connect_code == Some(CONNECT_CODE_ABORTED) {
        if is_transport { (AgentRunErrorClass::Retriable, None) } else { (AgentRunErrorClass::Cancelled, None) }
    } else if error_code.as_deref() == Some("CUSTOM_MESSAGE") && explicit_retryable != Some(true) {
        (AgentRunErrorClass::NonRetriable, None)
    } else if explicit_retryable == Some(false) || blob_not_found {
        (AgentRunErrorClass::NonRetriable, None)
    } else if !is_transport && matches!(lower.trim(), "cancelled" | "canceled" | "user cancelled" | "user canceled") {
        (AgentRunErrorClass::Cancelled, None)
    } else {
        (AgentRunErrorClass::Retriable, None)
    };
    let mut title = backend_detail.as_ref().and_then(|detail| detail.title.clone());
    let mut detail = backend_detail.as_ref().and_then(|detail| detail.detail.clone());
    let mut normalized_error_code = error_code;
    if blob_not_found {
        title = Some("Conversation data missing".into());
        detail = Some("Some conversation data is no longer available. Reopen or recreate the affected conversation instead of retrying the missing blob.".into());
        normalized_error_code = Some("BLOB_NOT_FOUND".into());
    }
    AgentRunErrorClassification {
        class,
        request_id: request_id.map(str::trim).filter(|value| !value.is_empty()).map(str::to_string),
        display_info: AgentRunErrorDisplayInfo {
            title, detail,
            is_retryable: explicit_retryable.or(Some(matches!(class, AgentRunErrorClass::Retriable))),
            connect_code,
            error_code: normalized_error_code,
            inference_request_error_type: root.as_ref().and_then(find_inference_request_error_type),
            error_details: raw_error_details,
        },
        action,
        is_transport,
    }
}
fn recovery_route_action(action: &str) -> Option<Value> {
    let label = match action {
        "login" => "Sign in", "upgrade" => "Upgrade", "payment" => "Review billing",
        "config" => "Open settings", _ => return None,
    };
    Some(json!({"kind":"recovery-route","label":label,"route":action}))
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendAdditionalInfo {
    pub rate_limit_reason: Option<String>,
    pub next_reset_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackendDetail {
    pub title: Option<String>,
    pub detail: Option<String>,
    pub buttons: Vec<Value>,
    pub additional_info: Option<BackendAdditionalInfo>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunErrorDescription {
    pub title: Option<String>,
    pub detail: String,
    pub actions: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackendConnectErrorEnvelope {
    pub message: String,
    pub code: Option<u16>,
    pub details: Vec<Value>,
    pub cause: Option<Box<BackendConnectErrorEnvelope>>,
    pub errors: Vec<BackendConnectErrorEnvelope>,
    pub inference_request_error_type: Option<String>,
}

fn backend_connect_error_from_value(
    value: &Value,
    fallback_message: &str,
) -> Option<BackendConnectErrorEnvelope> {
    let value = value.get("error").unwrap_or(value);
    let object = value.as_object()?;
    let message = object
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback_message)
        .to_string();
    let code = object.get("code").and_then(|value| {
        value
            .as_u64()
            .and_then(|value| u16::try_from(value).ok())
            .or_else(|| value.as_str().and_then(|value| value.parse::<u16>().ok()))
    });
    let mut details = object
        .get("details")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if details.is_empty()
        && ["title", "detail", "buttons", "additionalInfo"]
            .iter()
            .any(|key| object.contains_key(*key))
    {
        details.push(json!({"details": value.clone()}));
    }
    let cause = object
        .get("cause")
        .and_then(|cause| backend_connect_error_from_value(cause, "backend cause"))
        .map(Box::new);
    let errors: Vec<BackendConnectErrorEnvelope> = object
        .get("errors")
        .and_then(Value::as_array)
        .map(|errors| {
            errors
                .iter()
                .filter_map(|error| {
                    backend_connect_error_from_value(error, "backend aggregate error")
                })
                .collect()
        })
        .unwrap_or_default();
    if details.is_empty() && cause.is_none() && errors.is_empty()
        && !object.contains_key("message") && !object.contains_key("code")
    {
        return None;
    }
    let inference_request_error_type = object
        .get("inferenceRequestErrorType")
        .or_else(|| object.get("metadata").and_then(|value| value.get("x-cursor-inference-request-error-type")))
        .or_else(|| object.get("headers").and_then(|value| value.get("x-cursor-inference-request-error-type")))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Some(BackendConnectErrorEnvelope {
        message,
        code,
        details,
        cause,
        errors,
        inference_request_error_type,
    })
}

pub fn parse_backend_connect_error(
    message: &str,
) -> Option<BackendConnectErrorEnvelope> {
    for (index, ch) in message.char_indices() {
        if ch != '{' {
            continue;
        }
        let candidate = &message[index..];
        let mut values = serde_json::Deserializer::from_str(candidate)
            .into_iter::<Value>();
        let Some(Ok(value)) = values.next() else {
            continue;
        };
        if let Some(error) = backend_connect_error_from_value(&value, message) {
            return Some(error);
        }
    }
    None
}

fn walk_backend_connect_error<'a>(
    error: &'a BackendConnectErrorEnvelope,
    first: &mut Option<&'a BackendConnectErrorEnvelope>,
) -> Option<&'a BackendConnectErrorEnvelope> {
    if !error.details.is_empty() {
        return Some(error);
    }
    if first.is_none() {
        *first = Some(error);
    }
    if let Some(cause) = error.cause.as_deref() {
        if let Some(found) = walk_backend_connect_error(cause, first) {
            return Some(found);
        }
    }
    for inner in &error.errors {
        if let Some(found) = walk_backend_connect_error(inner, first) {
            return Some(found);
        }
    }
    None
}

pub fn find_backend_connect_error(
    error: &BackendConnectErrorEnvelope,
    require_details: bool,
) -> Option<&BackendConnectErrorEnvelope> {
    let mut first = None;
    walk_backend_connect_error(error, &mut first)
        .or_else(|| if require_details { None } else { first })
}

fn backend_detail_from_value(value: &Value) -> Option<BackendDetail> {
    let value = value.get("details").unwrap_or(value);
    let object = value.as_object()?;
    let display = object.get("details").and_then(Value::as_object).unwrap_or(object);
    let title = display
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_string);
    let detail = display
        .get("detail")
        .and_then(Value::as_str)
        .map(str::to_string);
    let buttons = display
        .get("buttons")
        .or_else(|| object.get("buttons"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let additional_info = display
        .get("additionalInfo")
        .or_else(|| object.get("additionalInfo"))
        .and_then(Value::as_object)
        .map(|info| BackendAdditionalInfo {
            rate_limit_reason: info
                .get("rateLimitReason")
                .and_then(Value::as_str)
                .map(str::to_string),
            next_reset_at: info
                .get("nextResetAt")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    if title.is_none() && detail.is_none() && buttons.is_empty() && additional_info.is_none() {
        None
    } else {
        Some(BackendDetail {
            title,
            detail,
            buttons,
            additional_info,
        })
    }
}

pub fn backend_detail_from_error_message(message: &str) -> Option<BackendDetail> {
    let root = parse_backend_connect_error(message)?;
    find_backend_connect_error(&root, true)?
        .details
        .first()
        .and_then(backend_detail_from_value)
}

pub fn get_backend_error_detail_message(message: &str) -> Option<String> {
    let detail = backend_detail_from_error_message(message)?;
    let title = detail.title.as_deref().map(str::trim).unwrap_or("");
    let body = detail.detail.as_deref().map(str::trim).unwrap_or("");
    if title.is_empty() {
        return (!body.is_empty()).then(|| body.to_string());
    }
    if body.is_empty() || body == title {
        Some(title.to_string())
    } else {
        Some(format!("{title}\n\n{body}"))
    }
}

pub fn format_agent_run_error(message: &str) -> String {
    get_backend_error_detail_message(message).unwrap_or_else(|| message.to_string())
}

pub fn format_sand_usage_reset_in(next_reset_at: &str, now_ms: i64) -> Option<String> {
    let reset_ms = DateTime::parse_from_rfc3339(next_reset_at)
        .ok()?
        .timestamp_millis();
    let ms = reset_ms.saturating_sub(now_ms);
    if ms <= 0 {
        return Some("less than a minute".into());
    }
    if ms >= 86_400_000 {
        let days = div_ceil(ms, 86_400_000);
        return Some(format!("{days} day{}", if days == 1 { "" } else { "s" }));
    }
    if ms >= 3_600_000 {
        let hours = div_ceil(ms, 3_600_000);
        return Some(format!(
            "{hours} hour{}",
            if hours == 1 { "" } else { "s" }
        ));
    }
    let minutes = div_ceil(ms, 60_000).max(1);
    Some(format!(
        "{minutes} minute{}",
        if minutes == 1 { "" } else { "s" }
    ))
}

pub fn with_relative_sand_included_limit_reset(
    detail: &str,
    next_reset_at: Option<&str>,
    now_ms: i64,
) -> String {
    let Some(next_reset_at) = next_reset_at else {
        return detail.to_string();
    };
    let Some(relative) = format_sand_usage_reset_in(next_reset_at, now_ms) else {
        return detail.to_string();
    };
    let replaced = replace_reset_at_clause(detail, &relative);
    replace_reset_in_clause(&replaced, &relative)
}

pub fn checkout_deep_control_url(action: &Value) -> String {
    let tier = action
        .get("membershipToUpgradeTo")
        .and_then(Value::as_str)
        .filter(|tier| matches!(*tier, "pro" | "pro_plus" | "ultra"))
        .unwrap_or("pro");
    let mut url = format!(
        "{CURSOR_WEBSITE_ORIGIN}/api/auth/checkoutDeepControl?tier={tier}"
    );
    if let Some(allow_trial) = action.get("allowTrial").and_then(Value::as_bool) {
        url.push_str(if allow_trial {
            "&allowTrial=true"
        } else {
            "&allowTrial=false"
        });
    }
    url
}

pub fn map_error_detail_buttons(buttons: &[Value]) -> Vec<Value> {
    let mut actions = Vec::new();
    let mut has_switch = false;
    for button in buttons {
        if actions.len() >= MAX_TRAY_ACTIONS {
            break;
        }
        let label = button.get("label").and_then(Value::as_str).unwrap_or("");
        let Some(action) = button.get("action").and_then(Value::as_object) else {
            continue;
        };
        let case = action.get("case").and_then(Value::as_str).unwrap_or("");
        let value = action.get("value").cloned().unwrap_or_else(|| json!({}));
        match case {
            "url" => {
                let Some(raw_url) = value.get("url").and_then(Value::as_str) else {
                    continue;
                };
                let Ok(parsed) = Url::parse(raw_url) else {
                    continue;
                };
                if matches!(parsed.scheme(), "http" | "https") && !label.is_empty() {
                    actions.push(json!({
                        "kind": "open-url",
                        "label": label,
                        "url": raw_url,
                    }));
                }
            }
            "upgrade" => actions.push(json!({
                "kind": "open-url",
                "label": if label.is_empty() { "Upgrade" } else { label },
                "url": checkout_deep_control_url(&value),
            })),
            "upgradeChoice" => actions.push(json!({
                "kind": "open-url",
                "label": if label.is_empty() { "Upgrade" } else { label },
                "url": format!("{CURSOR_WEBSITE_ORIGIN}/pricing"),
            })),
            "switchModel" if !has_switch => {
                has_switch = true;
                actions.push(json!({ "kind": "switch-model" }));
            }
            "dashboardAction" => {
                let verb = value.get("action").and_then(Value::as_str).unwrap_or("");
                if SUPPORTED_DASHBOARD_ACTION_VERBS.contains(&verb) && !label.is_empty() {
                    actions.push(json!({
                        "kind": "dashboard-action",
                        "label": label,
                        "action": verb,
                        "args": value.get("args").cloned().unwrap_or_else(|| json!({})),
                        "successMessage": value.get("successMessage").cloned().unwrap_or(Value::Null),
                    }));
                }
            }
            _ => {}
        }
    }
    actions
}

pub fn describe_agent_run_error(
    formatted: &str,
    detail: Option<&BackendDetail>,
    now_ms: i64,
) -> AgentRunErrorDescription {
    let title = detail
        .and_then(|detail| detail.title.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let actions = detail
        .map(|detail| map_error_detail_buttons(&detail.buttons))
        .unwrap_or_default();
    let mut shown = if title.is_some() {
        detail
            .and_then(|detail| detail.detail.as_deref())
            .unwrap_or("")
            .trim()
            .to_string()
    } else {
        formatted.to_string()
    };
    if let Some(info) = detail.and_then(|detail| detail.additional_info.as_ref()) {
        if info.rate_limit_reason.as_deref() == Some(SAND_INCLUDED_LIMIT_REASON) {
            shown = with_relative_sand_included_limit_reset(
                if shown.is_empty() { formatted } else { &shown },
                info.next_reset_at.as_deref(),
                now_ms,
            );
        }
    }
    if shown.is_empty() {
        shown = formatted.to_string();
    }
    AgentRunErrorDescription {
        title,
        detail: shown,
        actions,
    }
}

pub fn introduction_failure_tray(
    agent_id: &str,
    request_id: Option<&str>,
    message: &str,
    now_ms: i64,
) -> PushErrorOptions {
    let mut tray = provider_failure_tray_for_request(agent_id, request_id, message, now_ms);
    tray.title = INTRODUCTION_FAILED_TRAY_TITLE.to_string();
    tray.error_kind = Some("agent_introduction_failed".into());
    tray.dedupe_key = Some(format!("introduction-failed:{agent_id}"));
    tray
}

pub fn provider_failure_tray(agent_id: &str, message: &str, now_ms: i64) -> PushErrorOptions {
    provider_failure_tray_for_request(agent_id, None, message, now_ms)
}

pub fn provider_failure_tray_for_request(
    agent_id: &str,
    request_id: Option<&str>,
    message: &str,
    now_ms: i64,
) -> PushErrorOptions {
    let classification = classify_agent_run_error(message, request_id, false);
    let formatted = format_agent_run_error(message);
    let backend_detail = backend_detail_from_error_message(message);
    let described = describe_agent_run_error(&formatted, backend_detail.as_ref(), now_ms);
    let mut actions = Vec::new();
    if let Some(action) = classification.action.as_deref().and_then(recovery_route_action) { actions.push(action); }
    for action in described.actions {
        if actions.len() >= MAX_TRAY_ACTIONS { break; }
        if !actions.contains(&action) { actions.push(action); }
    }
    let title = classification.display_info.title.clone().or(described.title)
        .unwrap_or_else(|| "Agent run failed".to_string());
    let detail = if classification.display_info.error_code.as_deref() == Some("BLOB_NOT_FOUND") {
        classification.display_info.detail.clone().unwrap_or(described.detail)
    } else { described.detail };
    PushErrorOptions {
        agent_id: Some(agent_id.to_string()),
        title,
        detail,
        request_id: classification.request_id.clone(),
        error_kind: Some("agent_run_error".into()),
        raw_detail: Some(message.to_string()),
        actions,
        dedupe_key: Some(format!("agent-run-error:{agent_id}")),
        classification: Some(classification.class.as_str().into()),
        recovery_action: classification.action.clone(),
        is_transport: Some(classification.is_transport),
        display_info: Some(classification.display_info.to_json()),
        ..PushErrorOptions::default()
    }
}

fn div_ceil(value: i64, divisor: i64) -> i64 {
    value.saturating_add(divisor - 1) / divisor
}

fn replace_reset_at_clause(detail: &str, relative: &str) -> String {
    let marker = "It resets at ";
    let mut output = detail.to_string();
    let mut cursor = 0usize;
    while let Some(relative_start) = output[cursor..].find(marker) {
        let start = cursor + relative_start;
        let value_start = start + marker.len();
        let mut end = value_start;
        for (offset, ch) in output[value_start..].char_indices() {
            if ch.is_whitespace() {
                break;
            }
            end = value_start + offset + ch.len_utf8();
        }
        if end == value_start {
            break;
        }
        let replacement = format!("It resets in {relative}.");
        output.replace_range(start..end, &replacement);
        cursor = start + replacement.len();
    }
    output
}

fn replace_reset_in_clause(detail: &str, relative: &str) -> String {
    let marker = "It resets in ";
    let mut output = detail.to_string();
    let mut cursor = 0usize;
    while let Some(relative_start) = output[cursor..].find(marker) {
        let start = cursor + relative_start;
        let after_marker = start + marker.len();
        let end = output[after_marker..]
            .find('.')
            .map(|offset| after_marker + offset)
            .unwrap_or(output.len());
        let replacement = format!("It resets in {relative}");
        output.replace_range(start..end, &replacement);
        cursor = start + replacement.len();
    }
    output
}


#[cfg(test)]
mod android_parity_tests {
    use super::*;

    #[test]
    fn introduction_failure_tray_is_agent_scoped_and_deduped() {
        let tray = introduction_failure_tray(
            "agent-a",
            Some("agent-kickstart:agent-a:1"),
            "provider failed",
            1,
        );
        assert_eq!(tray.agent_id.as_deref(), Some("agent-a"));
        assert_eq!(tray.title, INTRODUCTION_FAILED_TRAY_TITLE);
        assert_eq!(tray.error_kind.as_deref(), Some("agent_introduction_failed"));
        assert_eq!(tray.dedupe_key.as_deref(), Some("introduction-failed:agent-a"));
        assert_eq!(
            tray.request_id.as_deref(),
            Some("agent-kickstart:agent-a:1")
        );
    }

    #[test]
    fn android_agent_run_error_actions_are_bounded_and_safe() {
        let buttons = vec![
            json!({
                "label":"Unsafe",
                "action":{"case":"url","value":{"url":"javascript:alert(1)"}}
            }),
            json!({
                "label":"Docs",
                "action":{"case":"url","value":{"url":"https://example.com/docs"}}
            }),
            json!({
                "label":"Switch",
                "action":{"case":"switchModel","value":{}}
            }),
            json!({
                "label":"Duplicate switch",
                "action":{"case":"switchModel","value":{}}
            }),
            json!({
                "label":"Limit",
                "action":{
                    "case":"dashboardAction",
                    "value":{"action":"requestLimitIncrease","args":{"source":"tray"}}
                }
            }),
            json!({
                "label":"Overflow",
                "action":{"case":"upgradeChoice","value":{}}
            }),
        ];
        let actions = map_error_detail_buttons(&buttons);
        assert_eq!(actions.len(), MAX_TRAY_ACTIONS);
        assert_eq!(actions[0]["kind"], "open-url");
        assert_eq!(actions[0]["url"], "https://example.com/docs");
        assert_eq!(actions[1]["kind"], "switch-model");
        assert_eq!(actions[2]["kind"], "dashboard-action");
        assert!(
            actions
                .iter()
                .all(|action| action.get("url").and_then(Value::as_str) != Some("javascript:alert(1)"))
        );
    }

    #[test]
    fn android_agent_run_error_included_limit_uses_relative_reset_without_auto_retry() {
        let detail = BackendDetail {
            title: Some("Usage limit reached".into()),
            detail: Some("Included usage is exhausted. It resets at 2026-10-13T02:00:00Z.".into()),
            buttons: Vec::new(),
            additional_info: Some(BackendAdditionalInfo {
                rate_limit_reason: Some(SAND_INCLUDED_LIMIT_REASON.into()),
                next_reset_at: Some("2026-10-13T02:00:00Z".into()),
            }),
        };
        let described = describe_agent_run_error(
            "fallback",
            Some(&detail),
            DateTime::parse_from_rfc3339("2026-10-13T01:30:00Z")
                .unwrap()
                .timestamp_millis(),
        );
        assert_eq!(described.title.as_deref(), Some("Usage limit reached"));
        assert!(described.detail.contains("It resets in 30 minutes."));
        assert!(described.actions.is_empty());
    }
}
