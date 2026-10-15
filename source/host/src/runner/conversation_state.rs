use std::collections::HashMap;

use serde_json::Value;

pub const HIDDEN_PROMPT_MARKER: &str = "[SAND_HIDDEN_PROMPT]";
pub const SUMMARIZATION_MAX_PROMPT_CHARS: usize = 2_800_000;
pub const SUMMARIZATION_MAX_OUTPUT_TOKENS: u64 = 32_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentUserMessage {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryUserMessage {
    pub id: String,
    pub text: String,
    pub confirmed: Option<bool>,
}

pub fn would_recover_via_prepend(
    recent_user_messages: &[RecoveryUserMessage],
    current_message_id: &str,
    message_id: &str,
) -> bool {
    let Some(current_index) = recent_user_messages
        .iter()
        .position(|message| message.id == current_message_id)
    else {
        return false;
    };
    let Some(target_index) = recent_user_messages
        .iter()
        .position(|message| message.id == message_id)
    else {
        return false;
    };
    target_index <= current_index
        && recent_user_messages[target_index].confirmed != Some(true)
}

pub fn select_unconfirmed_user_messages(
    recent_user_messages: &[RecentUserMessage],
    current_message_id: Option<&str>,
    last_turn_user_message_id: Option<&str>,
    has_confirmed_turns: bool,
) -> Vec<RecentUserMessage> {
    let Some(current_message_id) = current_message_id else {
        return Vec::new();
    };
    let Some(current_index) = recent_user_messages
        .iter()
        .position(|message| message.id == current_message_id)
    else {
        return Vec::new();
    };
    if current_index == 0 {
        return Vec::new();
    }

    let start = if let Some(last_turn_user_message_id) = last_turn_user_message_id {
        let Some(watermark) = recent_user_messages
            .iter()
            .position(|message| message.id == last_turn_user_message_id)
        else {
            return Vec::new();
        };
        watermark.saturating_add(1)
    } else if !has_confirmed_turns {
        0
    } else {
        return Vec::new();
    };

    if start >= current_index {
        return Vec::new();
    }
    recent_user_messages[start..current_index]
        .iter()
        .filter(|message| !message.text.trim().is_empty())
        .cloned()
        .collect()
}

pub fn build_unanswered_questions_note(skipped: &[String], dismissed: &[String]) -> String {
    let skipped = clean_prompts(skipped);
    let dismissed = clean_prompts(dismissed);
    let mut sections = Vec::new();
    match skipped.as_slice() {
        [] => {}
        [one] => sections.push(format!(
            "Earlier you prompted the user and they moved on without responding (\"{one}\") — treat it as skipped. Don't wait for or assume a response; continue with what you already know, and only ask again if you still genuinely need it."
        )),
        many => {
            let list = many.iter().map(|value| format!("\n- \"{value}\"")).collect::<String>();
            sections.push(format!(
                "Earlier you prompted the user for these and they moved on without responding — treat them as skipped:{list}\nDon't wait for or assume responses; continue with what you already know, and only ask again if you still genuinely need to."
            ));
        }
    }
    match dismissed.as_slice() {
        [] => {}
        [one] => sections.push(format!(
            "The user dismissed your question (\"{one}\") without answering — they'd rather not respond. Don't ask it again or wait for an answer; continue with what you already know and decide yourself."
        )),
        many => {
            let list = many.iter().map(|value| format!("\n- \"{value}\"")).collect::<String>();
            sections.push(format!(
                "The user dismissed these questions without answering — they'd rather not respond:{list}\nDon't ask them again or wait for answers; continue with what you already know and decide yourself."
            ));
        }
    }
    if sections.is_empty() {
        String::new()
    } else {
        format!("{HIDDEN_PROMPT_MARKER}{}", sections.join("\n\n"))
    }
}

pub fn to_safe_usage_count(value: f64) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else {
        value.floor().min(u64::MAX as f64) as u64
    }
}

pub fn await_block_until_ms(value: Option<&Value>) -> f64 {
    match value {
        None | Some(Value::Null) => 0.0,
        Some(Value::Bool(value)) => u8::from(*value) as f64,
        Some(Value::Number(value)) => value.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(value)) if value.trim().is_empty() => 0.0,
        Some(Value::String(value)) => value.trim().parse::<f64>().unwrap_or(f64::NAN),
        Some(Value::Array(_)) | Some(Value::Object(_)) => f64::NAN,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletedAwaitOutcome {
    SleptFull,
    CompletedEarly,
}

pub fn classify_completed_await_outcome(await_call: &Value) -> CompletedAwaitOutcome {
    let Some(success) = await_call
        .get("result")
        .and_then(|value| value.get("result"))
        .filter(|value| value.get("case").and_then(Value::as_str) == Some("success"))
    else {
        return CompletedAwaitOutcome::CompletedEarly;
    };
    let Some(await_result) = success
        .get("value")
        .and_then(|value| value.get("awaitResult"))
    else {
        return CompletedAwaitOutcome::CompletedEarly;
    };

    match await_result.get("case").and_then(Value::as_str) {
        Some("complete") => {
            let task_id = await_result
                .get("value")
                .and_then(|value| value.get("taskId"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if task_id.trim().is_empty() {
                CompletedAwaitOutcome::SleptFull
            } else {
                CompletedAwaitOutcome::CompletedEarly
            }
        }
        Some("stillRunning") => {
            let regex_match = await_result
                .get("value")
                .and_then(|value| value.get("regexMatch"));
            let matched = match regex_match {
                Some(Value::String(value)) => !value.is_empty(),
                Some(Value::Array(value)) => !value.is_empty(),
                _ => false,
            };
            if matched {
                CompletedAwaitOutcome::CompletedEarly
            } else {
                CompletedAwaitOutcome::SleptFull
            }
        }
        _ => CompletedAwaitOutcome::CompletedEarly,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizedUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

pub fn sanitize_usage(
    prompt_tokens: f64,
    completion_tokens: f64,
    total_tokens: f64,
) -> SanitizedUsage {
    let prompt_tokens = to_safe_usage_count(prompt_tokens);
    let completion_tokens = to_safe_usage_count(completion_tokens);
    let total_tokens = to_safe_usage_count(total_tokens);
    SanitizedUsage {
        prompt_tokens,
        completion_tokens,
        total_tokens: if total_tokens == 0 {
            prompt_tokens.saturating_add(completion_tokens)
        } else {
            total_tokens
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SanitizedExtendedUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub max_tokens: u64,
}

#[derive(Debug, Default)]
pub struct ContextWindowTracker {
    by_model: HashMap<String, u64>,
}

impl ContextWindowTracker {
    pub fn last_reported(&self, model_id: &str) -> Option<u64> {
        self.by_model.get(model_id).copied()
    }

    pub fn sanitize_extended_usage(
        &mut self,
        usage: &Value,
        model_id: &str,
    ) -> SanitizedExtendedUsage {
        let count = |field: &str| {
            usage
                .get(field)
                .and_then(Value::as_f64)
                .map(to_safe_usage_count)
                .unwrap_or_default()
        };
        let max_tokens = count("maxTokens");
        if max_tokens > 0 {
            self.by_model.insert(model_id.to_string(), max_tokens);
        }
        SanitizedExtendedUsage {
            input_tokens: count("inputTokens"),
            output_tokens: count("outputTokens"),
            cache_read_tokens: count("cacheReadTokens"),
            cache_write_tokens: count("cacheWriteTokens"),
            max_tokens,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamSanitizerItem<T> {
    Emit(T),
    DeferredError(String),
    Suppressed,
}

#[derive(Debug, Default)]
pub struct FullStreamSanitizer {
    deferred_error: Option<String>,
}

impl FullStreamSanitizer {
    pub fn accept_error(&mut self, error: impl Into<String>) -> StreamSanitizerItem<()> {
        if self.deferred_error.is_none() {
            self.deferred_error = Some(error.into());
            StreamSanitizerItem::DeferredError(
                self.deferred_error.clone().unwrap_or_default(),
            )
        } else {
            StreamSanitizerItem::Suppressed
        }
    }

    pub fn accept_value<T>(&self, value: T) -> StreamSanitizerItem<T> {
        if self.deferred_error.is_some() {
            StreamSanitizerItem::Suppressed
        } else {
            StreamSanitizerItem::Emit(value)
        }
    }

    pub fn finish(self) -> Result<(), String> {
        match self.deferred_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SummarizationPolicy {
    pub enable_reduce_inputs_retry: bool,
    pub max_prompt_chars: usize,
    pub max_output_tokens: u64,
    pub preserve_latest_image: bool,
}

pub fn summarization_policy(preserve_latest_image: bool) -> SummarizationPolicy {
    SummarizationPolicy {
        enable_reduce_inputs_retry: true,
        max_prompt_chars: SUMMARIZATION_MAX_PROMPT_CHARS,
        max_output_tokens: SUMMARIZATION_MAX_OUTPUT_TOKENS,
        preserve_latest_image,
    }
}

#[derive(Debug, Default)]
pub struct ResolvedModelTracker {
    sequence: u64,
    accepted: HashMap<String, (u64, String)>,
}

impl ResolvedModelTracker {
    pub fn resolved(&self, requested_model: &str) -> Option<&str> {
        self.accepted
            .get(requested_model)
            .map(|(_, model)| model.as_str())
    }

    pub fn begin_request(&mut self, requested_model: &str) -> ModelResolutionTicket {
        self.sequence = self.sequence.saturating_add(1);
        ModelResolutionTicket {
            requested_model: requested_model.to_string(),
            sequence: self.sequence,
        }
    }

    pub fn accept_resolution(&mut self, ticket: &ModelResolutionTicket, model_id: &str) -> bool {
        let prior = self
            .accepted
            .get(&ticket.requested_model)
            .map(|(sequence, _)| *sequence);
        if prior.is_some_and(|sequence| sequence >= ticket.sequence) {
            return false;
        }
        self.accepted.insert(
            ticket.requested_model.clone(),
            (ticket.sequence, model_id.to_string()),
        );
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelResolutionTicket {
    pub requested_model: String,
    pub sequence: u64,
}

pub fn should_use_self_summary(model_id: &str) -> bool {
    const DEPRECATED: [&str; 6] = [
        "grok-4.5-medium",
        "grok-4.5-fast-medium",
        "grok-4.5-high",
        "grok-4.5-fast-high",
        "grok-4.5-xhigh",
        "grok-4.5-fast-xhigh",
    ];
    let model = model_id.split('#').next().unwrap_or(model_id);
    let external = model
        .strip_prefix("XAIEXTERNAL--")
        .and_then(|rest| rest.split_once("--"))
        .map(|(_, name)| name);
    model == "grok-4.5"
        || DEPRECATED.contains(&model)
        || model == "cursor-grok-4.5"
        || model.starts_with("cursor-grok-4.5-")
        || model == "vega"
        || model.starts_with("vega-")
        || model.starts_with("accounts/anysphere/models/vega")
        || model.starts_with("cursor/vega")
        || model == "v9"
        || model.starts_with("v9-")
        || external.is_some_and(|value| value == "v9" || value.starts_with("v9-"))
}

fn clean_prompts(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}
