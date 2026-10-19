use serde::Serialize;
use serde_json::Value;

use super::sand_prompt_markers::{
    SAND_HIDDEN_PROMPT_MARKER, SAND_TRUSTED_AUTOMATION_PROMPT_MARKER,
};

pub const SEND_MESSAGE_TOOL_CALL_OUTLINE_NAME: &str = "sendMessageToolCall";
pub const MCP_TOOL_CALL_OUTLINE_NAME: &str = "mcpToolCall";
pub const MAX_TOOL_ACTIVITY_ARGS_CHARS: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind")]
pub enum OutlineItem {
    #[serde(rename = "user")]
    User {
        id: String,
        text: String,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        hidden: bool,
    },
    #[serde(rename = "assistant-text")]
    AssistantText {
        id: String,
        text: String,
    },
    #[serde(rename = "thinking")]
    Thinking {
        id: String,
        text: String,
        #[serde(rename = "durationMs", skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    #[serde(rename = "send-message")]
    SendMessage {
        id: String,
        message: Value,
    },
    #[serde(rename = "tool-call")]
    ToolCall {
        id: String,
        name: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutlineTurn {
    pub raw_user_text: String,
    pub user_message_id: String,
    pub items: Vec<OutlineItem>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TaskToolCall {
    pub description: Option<String>,
    pub prompt: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct OutlineToolCall {
    pub case: Option<String>,
    pub task: Option<TaskToolCall>,
    pub computer_action_cases: Vec<String>,
    pub send_message: Option<Value>,
    pub activity_args: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutlineStep {
    AssistantMessage { text: String },
    ThinkingMessage { text: String, duration_ms: u64 },
    ToolCall { tool_call: OutlineToolCall, event: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConversationTurnInput {
    Agent {
        raw_user_text: String,
        user_message_id: String,
        steps: Vec<OutlineStep>,
    },
    Shell {
        command: String,
    },
}

pub fn strip_hidden_marker(text: &str) -> String {
    let without_hidden = text
        .strip_prefix(SAND_HIDDEN_PROMPT_MARKER)
        .unwrap_or(text);
    without_hidden
        .strip_prefix(SAND_TRUSTED_AUTOMATION_PROMPT_MARKER)
        .unwrap_or(without_hidden)
        .to_string()
}

pub fn get_outline_tool_call_name(tool_call: &OutlineToolCall) -> String {
    match tool_call.case.as_deref() {
        Some("taskToolCall") => "Task".into(),
        Some("computerUseToolCall")
            if tool_call.computer_action_cases.len() == 1
                && tool_call.computer_action_cases[0] == "screenshot" =>
        {
            "Screenshot".into()
        }
        Some(case) if !case.is_empty() => case.to_string(),
        _ => "Tool".into(),
    }
}

pub fn get_task_summary(task: &TaskToolCall) -> Option<String> {
    if let Some(error) = task.error.as_deref().filter(|value| !value.is_empty()) {
        return Some(error.to_string());
    }
    if let Some(description) = task
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(description.to_string());
    }
    task.prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn get_outline_tool_call_summary(tool_call: &OutlineToolCall) -> Option<String> {
    if tool_call.case.as_deref() == Some("taskToolCall") {
        return tool_call.task.as_ref().and_then(get_task_summary);
    }
    None
}

pub fn get_tool_call_activity_args(tool_call: &OutlineToolCall) -> Option<String> {
    let raw = serde_json::to_string(tool_call.activity_args.as_ref()?).ok()?;
    if matches!(raw.as_str(), "{}" | "\"\"" | "[]" | "null") {
        return None;
    }
    if raw.chars().count() <= MAX_TOOL_ACTIVITY_ARGS_CHARS {
        return Some(raw);
    }
    let prefix = raw
        .chars()
        .take(MAX_TOOL_ACTIVITY_ARGS_CHARS)
        .collect::<String>();
    Some(format!("{prefix}\n… (truncated)"))
}

pub fn is_failed_task_tool_call(tool_call: &OutlineToolCall) -> bool {
    tool_call.case.as_deref() == Some("taskToolCall")
        && tool_call
            .task
            .as_ref()
            .and_then(|task| task.error.as_deref())
            .is_some()
}

pub fn get_outline_tool_call_status(event: &str, tool_call: &OutlineToolCall) -> &'static str {
    if event != "toolCallCompleted" {
        "pending"
    } else if is_failed_task_tool_call(tool_call) {
        "failed"
    } else {
        "done"
    }
}

pub fn send_message_from_tool_call(tool_call: &OutlineToolCall) -> Option<Value> {
    if tool_call.case.as_deref() != Some(SEND_MESSAGE_TOOL_CALL_OUTLINE_NAME) {
        return None;
    }
    let message = tool_call.send_message.as_ref()?;
    match message.get("type").and_then(Value::as_str) {
        Some("text") => {
            let content = message.get("content").and_then(Value::as_str)?;
            Some(serde_json::json!({"type":"text","content":content}))
        }
        Some("attachment") => {
            let url = message.get("url").and_then(Value::as_str)?;
            let alt = message
                .get("alt")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty());
            Some(match alt {
                Some(alt) => serde_json::json!({"type":"attachment","url":url,"alt":alt}),
                None => serde_json::json!({"type":"attachment","url":url}),
            })
        }
        _ => None,
    }
}

pub fn step_to_outline_item(step: &OutlineStep, id: &str) -> Option<OutlineItem> {
    match step {
        OutlineStep::AssistantMessage { text } => {
            (!text.is_empty()).then(|| OutlineItem::AssistantText {
                id: id.to_string(),
                text: text.clone(),
            })
        }
        OutlineStep::ThinkingMessage { text, duration_ms } => {
            (!text.is_empty()).then(|| OutlineItem::Thinking {
                id: id.to_string(),
                text: text.clone(),
                duration_ms: (*duration_ms > 0).then_some(*duration_ms),
            })
        }
        OutlineStep::ToolCall { tool_call, event } => {
            if tool_call.case.as_deref() == Some(SEND_MESSAGE_TOOL_CALL_OUTLINE_NAME) {
                return send_message_from_tool_call(tool_call).map(|message| {
                    OutlineItem::SendMessage {
                        id: id.to_string(),
                        message,
                    }
                });
            }
            Some(OutlineItem::ToolCall {
                id: id.to_string(),
                name: get_outline_tool_call_name(tool_call),
                status: get_outline_tool_call_status(event, tool_call).to_string(),
                summary: get_outline_tool_call_summary(tool_call),
            })
        }
    }
}

pub fn derive_outline_turns_from_conversation_state(
    turns: &[ConversationTurnInput],
) -> Vec<OutlineTurn> {
    let mut output = Vec::with_capacity(turns.len());
    for (turn_index, turn) in turns.iter().enumerate() {
        match turn {
            ConversationTurnInput::Agent {
                raw_user_text,
                user_message_id,
                steps,
            } => {
                let hidden = raw_user_text.starts_with(SAND_HIDDEN_PROMPT_MARKER);
                let user_text = strip_hidden_marker(raw_user_text);
                let mut items = Vec::new();
                if !user_text.trim().is_empty() {
                    items.push(OutlineItem::User {
                        id: format!("outline-user-{turn_index}"),
                        text: user_text,
                        hidden,
                    });
                }
                for (step_index, step) in steps.iter().enumerate() {
                    if let Some(item) =
                        step_to_outline_item(step, &format!("outline-{turn_index}-{step_index}"))
                    {
                        items.push(item);
                    }
                }
                output.push(OutlineTurn {
                    raw_user_text: raw_user_text.clone(),
                    user_message_id: user_message_id.clone(),
                    items,
                });
            }
            ConversationTurnInput::Shell { command } => {
                output.push(OutlineTurn {
                    raw_user_text: String::new(),
                    user_message_id: String::new(),
                    items: vec![OutlineItem::ToolCall {
                        id: format!("outline-shell-{turn_index}"),
                        name: "shellToolCall".into(),
                        status: "done".into(),
                        summary: (!command.is_empty()).then(|| command.clone()),
                    }],
                });
            }
        }
    }
    output
}

pub fn derive_outline_from_conversation_state(
    turns: &[ConversationTurnInput],
) -> Vec<OutlineItem> {
    derive_outline_turns_from_conversation_state(turns)
        .into_iter()
        .flat_map(|turn| turn.items)
        .collect()
}
