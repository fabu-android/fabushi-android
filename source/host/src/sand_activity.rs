use serde::Serialize;
use serde_json::Value;
use url::Url;

pub const MAX_ACTIVITY_DETAIL_CHARS: usize = 80;
pub const NAMED_ACTIVITY_MAX_HOLD_MS: u64 = 2_500;
pub const SEND_MESSAGE_TOOL_CALL_OUTLINE_NAME: &str = "SendMessage";
pub const SAND_BOX_SHELL_TOOL_NAME: &str = "Shell";
pub const SAND_EXTERNAL_SHELL_TOOL_NAME: &str = "ExternalShell";
pub const SAND_BOX_READ_TOOL_NAME: &str = "Read";
pub const SAND_EXTERNAL_READ_TOOL_NAME: &str = "ExternalRead";
pub const SAND_BOX_AWAIT_SHELL_TOOL_NAME: &str = "AwaitShell";
pub const SAND_EXTERNAL_AWAIT_SHELL_TOOL_NAME: &str = "ExternalAwaitShell";

/// Android keeps the legacy activity variants for backward compatibility while adding the
/// structured Desktop activity shape consumed by the canonical run-lifecycle projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value")]
pub enum AgentActivity {
    #[serde(rename = "thinking")]
    Thinking,
    #[serde(rename = "tool")]
    Tool {
        tool: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        #[serde(rename = "callId")]
        call_id: String,
    },
    #[serde(rename = "legacy-running-tool")]
    RunningTool(String),
    #[serde(rename = "legacy-waiting")]
    Waiting,
    #[serde(rename = "legacy-completed")]
    Completed,
    #[serde(rename = "legacy-failed")]
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActivityUpdate {
    ThinkingDelta,
    TextDelta { text: String },
    ToolCall {
        id: String,
        name: String,
        status: String,
        args: Option<String>,
        summary: Option<String>,
    },
    SendMessage,
    TurnEnded,
    Retrying,
    Other { update_type: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityTransition {
    Keep,
    Clear,
    Set(AgentActivity),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NamedActivityHoldState {
    pub held_activity: Option<AgentActivity>,
    pub held_since_ms: u64,
}

pub fn derive_activity_from_update(update: &ActivityUpdate) -> ActivityTransition {
    match update {
        ActivityUpdate::ThinkingDelta | ActivityUpdate::TextDelta { .. } => {
            ActivityTransition::Set(AgentActivity::Thinking)
        }
        ActivityUpdate::SendMessage | ActivityUpdate::TurnEnded => ActivityTransition::Clear,
        ActivityUpdate::ToolCall {
            id,
            name,
            status,
            args,
            summary,
        } => {
            if name == SEND_MESSAGE_TOOL_CALL_OUTLINE_NAME
                || status != "pending"
                || matches!(name.as_str(), "shellToolCall" | "readToolCall" | "awaitToolCall")
            {
                ActivityTransition::Keep
            } else {
                ActivityTransition::Set(derive_structured_tool_call_activity(
                    id,
                    name,
                    args.as_deref(),
                    summary.as_deref(),
                ))
            }
        }
        ActivityUpdate::Retrying | ActivityUpdate::Other { .. } => ActivityTransition::Keep,
    }
}

pub fn resolve_named_activity_hold(
    update: &ActivityUpdate,
    state: &NamedActivityHoldState,
    now_ms: u64,
    max_hold_ms: u64,
) -> (ActivityTransition, NamedActivityHoldState) {
    let base = derive_activity_from_update(update);
    match update {
        ActivityUpdate::ThinkingDelta | ActivityUpdate::TextDelta { .. } => {
            if state.held_activity.is_some()
                && now_ms.saturating_sub(state.held_since_ms) < max_hold_ms
            {
                (ActivityTransition::Keep, state.clone())
            } else {
                (base, state.clone())
            }
        }
        ActivityUpdate::ToolCall { .. } => match base {
            ActivityTransition::Set(ref activity) => (
                base.clone(),
                NamedActivityHoldState {
                    held_activity: Some(activity.clone()),
                    held_since_ms: now_ms,
                },
            ),
            _ if state.held_activity.is_some() => (
                base,
                NamedActivityHoldState {
                    held_activity: state.held_activity.clone(),
                    held_since_ms: now_ms,
                },
            ),
            _ => (base, state.clone()),
        },
        _ if base == ActivityTransition::Clear => {
            (base, NamedActivityHoldState::default())
        }
        _ => (base, state.clone()),
    }
}

#[derive(Debug, Default)]
pub struct GroupMemberActivityTracker {
    streaming: bool,
}

impl GroupMemberActivityTracker {
    pub fn update(&mut self, update: &ActivityUpdate) -> ActivityTransition {
        match update {
            ActivityUpdate::TextDelta { text } => {
                if text.is_empty() {
                    return ActivityTransition::Keep;
                }
                self.streaming = true;
                return ActivityTransition::Clear;
            }
            ActivityUpdate::ThinkingDelta => {
                return if self.streaming {
                    ActivityTransition::Keep
                } else {
                    ActivityTransition::Set(AgentActivity::Thinking)
                };
            }
            ActivityUpdate::ToolCall { name, status, .. } if status == "pending" => {
                self.streaming = false;
                if name == SEND_MESSAGE_TOOL_CALL_OUTLINE_NAME {
                    return ActivityTransition::Clear;
                }
            }
            ActivityUpdate::SendMessage | ActivityUpdate::TurnEnded => {
                self.streaming = false;
            }
            _ => {}
        }
        derive_activity_from_update(update)
    }
}

/// Legacy Android helper retained so older callers do not change ABI/source behavior.
pub fn derive_tool_call_activity(tool_name: &str, target: Option<&str>) -> AgentActivity {
    let detail = match target {
        Some(value) if !value.is_empty() => format!("{tool_name} {value}"),
        _ => tool_name.to_string(),
    };
    AgentActivity::RunningTool(detail)
}

/// Legacy Android syntax check retained for compatibility.
pub fn parse_args_json(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with('{') && trimmed.ends_with('}')
}

/// Legacy basename helper retained for compatibility.
pub fn file_basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

pub fn parse_activity_args_json(raw: Option<&str>) -> Option<serde_json::Map<String, Value>> {
    let parsed = serde_json::from_str::<Value>(raw?).ok()?;
    parsed.as_object().cloned()
}

pub fn activity_file_basename(path: Option<&str>) -> Option<String> {
    path?
        .split(['/', '\\'])
        .filter(|value| !value.is_empty())
        .next_back()
        .map(ToOwned::to_owned)
}

pub fn url_hostname(raw: Option<&str>) -> Option<String> {
    Url::parse(raw?).ok()?.host_str().map(ToOwned::to_owned)
}

pub fn extract_shell_edit_target(command: &str) -> Option<String> {
    let tokens = command
        .split_whitespace()
        .map(strip_token)
        .collect::<Vec<_>>();

    for (index, token) in tokens.iter().enumerate() {
        let candidate = if token == ">" || token == ">>" {
            tokens.get(index + 1).cloned()
        } else if token.starts_with(">>") && token.len() > 2 {
            Some(token[2..].to_string())
        } else if token.starts_with('>')
            && token.len() > 1
            && !token.as_bytes().first().is_some_and(u8::is_ascii_digit)
        {
            Some(token[1..].to_string())
        } else {
            None
        };
        if let Some(candidate) = candidate.filter(|candidate| !is_discard_target(candidate)) {
            if let Some(base) = activity_file_basename(Some(&candidate)) {
                return Some(base);
            }
        }
    }

    if let Some(tee_index) = tokens.iter().position(|token| token == "tee") {
        if let Some(candidate) = tokens
            .iter()
            .skip(tee_index + 1)
            .find(|token| !token.starts_with('-') && !contains_shell_control(token))
        {
            if !is_discard_target(candidate) {
                if let Some(base) = activity_file_basename(Some(candidate)) {
                    return Some(base);
                }
            }
        }
    }

    let has_sed = tokens.iter().any(|token| token == "sed");
    let has_in_place = tokens
        .iter()
        .any(|token| token == "-i" || token.starts_with("-i"));
    if has_sed && has_in_place {
        if let Some(candidate) = tokens.iter().rev().find(|token| {
            !token.starts_with('-')
                && (token.contains('/') || has_extension_like_suffix(token))
        }) {
            return activity_file_basename(Some(candidate));
        }
    }
    None
}

pub fn derive_structured_tool_call_activity(
    id: &str,
    name: &str,
    raw_args: Option<&str>,
    summary: Option<&str>,
) -> AgentActivity {
    let args = parse_activity_args_json(raw_args);
    let mut tool = name.to_string();
    let mut detail: Option<String> = None;
    let mut target: Option<String> = None;

    match name {
        "shellToolCall" | SAND_BOX_SHELL_TOOL_NAME => {
            tool = SAND_BOX_SHELL_TOOL_NAME.to_string();
            detail = extract_shell_edit_target(
                read_string(args.as_ref(), "command").unwrap_or_default(),
            );
        }
        SAND_EXTERNAL_SHELL_TOOL_NAME => {
            detail = extract_shell_edit_target(
                read_string(args.as_ref(), "command").unwrap_or_default(),
            );
        }
        "readToolCall" | SAND_BOX_READ_TOOL_NAME => {
            tool = SAND_BOX_READ_TOOL_NAME.to_string();
            detail = activity_file_basename(read_string(args.as_ref(), "path"));
        }
        SAND_EXTERNAL_READ_TOOL_NAME => {
            detail = activity_file_basename(read_string(args.as_ref(), "path"));
        }
        "webSearchToolCall" => {
            tool = "WebSearch".to_string();
            detail = read_string(args.as_ref(), "searchTerm").map(ToOwned::to_owned);
        }
        "webFetchToolCall" => {
            tool = "WebFetch".to_string();
            detail = url_hostname(read_string(args.as_ref(), "url"));
        }
        "generateImageToolCall" => {
            tool = "GenerateImage".to_string();
            detail = read_string(args.as_ref(), "description").map(ToOwned::to_owned);
        }
        "mcpToolCall" => {
            tool = "CallMcpTool".to_string();
            detail = read_string(args.as_ref(), "providerIdentifier")
                .or_else(|| read_string(args.as_ref(), "serverIdentifier"))
                .map(ToOwned::to_owned);
        }
        "getMcpToolsToolCall" => {
            tool = "GetMcpTools".to_string();
            detail = read_string(args.as_ref(), "server").map(ToOwned::to_owned);
        }
        "mcpAuthToolCall" => {
            tool = "McpAuth".to_string();
            detail = read_string(args.as_ref(), "serverIdentifier").map(ToOwned::to_owned);
        }
        "awaitToolCall" | SAND_BOX_AWAIT_SHELL_TOOL_NAME => {
            tool = SAND_BOX_AWAIT_SHELL_TOOL_NAME.to_string();
        }
        SAND_EXTERNAL_AWAIT_SHELL_TOOL_NAME => {}
        "computerUseToolCall" => tool = "Computer".to_string(),
        "Task" => detail = summary.map(ToOwned::to_owned),
        "Screenshot" => {}
        "communicateUpdateToolCall" => {
            if let Some(current_step) = read_string(args.as_ref(), "currentStep") {
                if let Some(payload) = parse_activity_args_json(Some(current_step)) {
                    if payload.get("__sand_tool__").and_then(Value::as_bool) == Some(true) {
                        if let Some(value) = read_string(Some(&payload), "tool") {
                            tool = value.to_string();
                        }
                        detail = read_string(Some(&payload), "detail").map(ToOwned::to_owned);
                        target = read_string(Some(&payload), "target").map(ToOwned::to_owned);
                    }
                }
            }
        }
        _ => {}
    }

    AgentActivity::Tool {
        tool,
        detail: detail.map(|value| clamp_line(&value, MAX_ACTIVITY_DETAIL_CHARS)),
        target,
        call_id: id.to_string(),
    }
}

fn read_string<'a>(
    args: Option<&'a serde_json::Map<String, Value>>,
    key: &str,
) -> Option<&'a str> {
    args?
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn clamp_line(raw: &str, max_chars: usize) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect()
}

fn strip_token(token: &str) -> String {
    token
        .trim_matches(|value| value == '\'' || value == '"')
        .to_string()
}

fn is_discard_target(target: &str) -> bool {
    matches!(target, "/dev/null" | "/dev/stdout" | "/dev/stderr")
}

fn contains_shell_control(token: &str) -> bool {
    token.chars().any(|value| matches!(value, ';' | '|' | '&' | '<' | '>'))
}

fn has_extension_like_suffix(token: &str) -> bool {
    activity_file_basename(Some(token))
        .and_then(|base| base.rsplit_once('.').map(|(_, suffix)| !suffix.is_empty()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_tool_activity_holds_across_short_text_delta_then_expires() {
        let tool = ActivityUpdate::ToolCall {
            id: "call-1".into(),
            name: "mcpToolCall".into(),
            status: "pending".into(),
            args: Some(r#"{"serverIdentifier":"github"}"#.into()),
            summary: None,
        };
        let (transition, state) =
            resolve_named_activity_hold(&tool, &NamedActivityHoldState::default(), 100, 2_500);
        assert!(matches!(
            transition,
            ActivityTransition::Set(AgentActivity::Tool { .. })
        ));

        let (held, state) = resolve_named_activity_hold(
            &ActivityUpdate::TextDelta { text: "partial".into() },
            &state,
            200,
            2_500,
        );
        assert_eq!(held, ActivityTransition::Keep);

        let (expired, _) = resolve_named_activity_hold(
            &ActivityUpdate::ThinkingDelta,
            &state,
            2_700,
            2_500,
        );
        assert_eq!(
            expired,
            ActivityTransition::Set(AgentActivity::Thinking)
        );
    }

    #[test]
    fn send_message_clears_activity_and_group_stream_hides_thinking() {
        assert_eq!(
            derive_activity_from_update(&ActivityUpdate::SendMessage),
            ActivityTransition::Clear
        );
        let mut tracker = GroupMemberActivityTracker::default();
        assert_eq!(
            tracker.update(&ActivityUpdate::TextDelta { text: "hello".into() }),
            ActivityTransition::Clear
        );
        assert_eq!(
            tracker.update(&ActivityUpdate::ThinkingDelta),
            ActivityTransition::Keep
        );
    }

    #[test]
    fn structured_shell_activity_extracts_edit_target() {
        let activity = derive_structured_tool_call_activity(
            "call-shell",
            "Shell",
            Some(r#"{"command":"printf hi > src/main.rs"}"#),
            None,
        );
        assert_eq!(
            activity,
            AgentActivity::Tool {
                tool: "Shell".into(),
                detail: Some("main.rs".into()),
                target: None,
                call_id: "call-shell".into(),
            }
        );
    }
}
