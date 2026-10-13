use chrono::DateTime;
use serde_json::{Value, json};
use url::Url;

use crate::extensions::trays::trays_service::PushErrorOptions;

pub const PROVIDER_OVERLOAD_ERROR_TITLE: &str = "Model provider is overloaded";
pub const PROVIDER_OVERLOAD_ERROR_DETAIL: &str =
    "The model provider is under heavy load right now. This is usually temporary — retry, or switch to another model.";
pub const SAND_INCLUDED_LIMIT_REASON: &str = "sand_included_limit";
pub const MAX_TRAY_ACTIONS: usize = 3;
pub const CURSOR_WEBSITE_ORIGIN: &str = "https://cursor.com";
pub const SUPPORTED_DASHBOARD_ACTION_VERBS: &[&str] = &["requestLimitIncrease"];

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
    Some(BackendConnectErrorEnvelope {
        message,
        code,
        details,
        cause,
        errors,
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
    let title = object
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_string);
    let detail = object
        .get("detail")
        .and_then(Value::as_str)
        .map(str::to_string);
    let buttons = object
        .get("buttons")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let additional_info = object
        .get("additionalInfo")
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

pub fn provider_failure_tray(
    agent_id: &str,
    message: &str,
    now_ms: i64,
) -> PushErrorOptions {
    let formatted = format_agent_run_error(message);
    let detail = backend_detail_from_error_message(message);
    let described = describe_agent_run_error(&formatted, detail.as_ref(), now_ms);
    PushErrorOptions {
        agent_id: Some(agent_id.to_string()),
        title: described
            .title
            .unwrap_or_else(|| "Agent run failed".to_string()),
        detail: described.detail,
        error_kind: Some("agent_run_error".into()),
        raw_detail: Some(message.to_string()),
        actions: described.actions,
        dedupe_key: Some(format!("agent-run-error:{agent_id}")),
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
