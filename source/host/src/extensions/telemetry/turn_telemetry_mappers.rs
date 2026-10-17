use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const TURN_INTERRUPT_EVENT: &str = "sand.turn.interrupt";
pub const TURN_AWAIT_EVENT: &str = "sand.turn.await";
pub const TURN_RETRY_EVENT: &str = "sand.turn.retry";
pub const CLOSING_SEND_NUDGE_EVENT: &str = "sand.turn.closing_send_nudge";
pub const USER_MESSAGE_RECEIVED_EVENT: &str = "sand.user_message.received";
pub const COMPUTER_USE_USAGE_EVENT: &str = "sand.computer_use.usage";
pub const TTFT_EVENT: &str = "sand.ttft";
pub const TURN_USAGE_EVENT: &str = "sand.turn.usage";
pub const TURN_USAGE_SCHEMA_VERSION: &str = "2";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: Option<u64>,
}

pub fn total_input_tokens(usage: TokenUsage) -> u64 {
    usage.input_tokens
}

pub fn usage_token_tags(usage: Option<TokenUsage>) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    let Some(usage) = usage else {
        return metadata;
    };
    metadata.insert("input_tokens".into(), usage.input_tokens.to_string());
    metadata.insert("output_tokens".into(), usage.output_tokens.to_string());
    metadata.insert("cache_read_tokens".into(), usage.cache_read_tokens.to_string());
    metadata.insert("cache_write_tokens".into(), usage.cache_write_tokens.to_string());
    if let Some(reasoning_tokens) = usage.reasoning_tokens {
        metadata.insert("reasoning_tokens".into(), reasoning_tokens.to_string());
    }
    metadata.insert("total_input_tokens".into(), total_input_tokens(usage).to_string());
    metadata
}

fn projection(
    level: &'static str,
    event: &'static str,
    metadata: BTreeMap<String, String>,
) -> HostTelemetryProjection {
    HostTelemetryProjection {
        level: Some(level),
        event: Some(event),
        metadata,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnInterruptFields {
    pub conversation_id: String,
    pub reason: String,
    pub had_active_run: bool,
    pub was_in_flight: bool,
}

pub fn turn_interrupt_telemetry(fields: &TurnInterruptFields) -> HostTelemetryProjection {
    projection(
        "info",
        TURN_INTERRUPT_EVENT,
        BTreeMap::from([
            ("conversation_id".into(), fields.conversation_id.clone()),
            ("reason".into(), fields.reason.clone()),
            ("had_active_run".into(), fields.had_active_run.to_string()),
            ("was_in_flight".into(), fields.was_in_flight.to_string()),
        ]),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnAwaitFields {
    pub conversation_id: String,
    pub block_until_ms: u64,
    pub outcome: String,
    pub await_index: u64,
}

pub fn turn_await_telemetry(fields: &TurnAwaitFields) -> HostTelemetryProjection {
    projection(
        "info",
        TURN_AWAIT_EVENT,
        BTreeMap::from([
            ("conversation_id".into(), fields.conversation_id.clone()),
            ("block_until_ms".into(), fields.block_until_ms.to_string()),
            ("outcome".into(), fields.outcome.clone()),
            ("await_index".into(), fields.await_index.to_string()),
        ]),
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct TurnRetryFields {
    pub conversation_id: String,
    pub outcome: String,
    pub attempt: u64,
    pub max_attempts: u64,
    pub error_type: String,
    pub error_code: String,
    pub cause: String,
    pub delay_ms: Option<f64>,
    pub server_paced: Option<bool>,
}

fn bounded_error_type(value: &str) -> bool {
    let len = value.chars().count();
    (1..=64).contains(&len)
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '|' | ':' | '-'))
}

pub fn turn_retry_telemetry(fields: &TurnRetryFields) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("conversation_id".into(), fields.conversation_id.clone()),
        ("outcome".into(), fields.outcome.clone()),
        ("attempt".into(), fields.attempt.to_string()),
        ("max_attempts".into(), fields.max_attempts.to_string()),
        (
            "error_type".into(),
            if bounded_error_type(&fields.error_type) {
                fields.error_type.clone()
            } else {
                "error".into()
            },
        ),
        ("error_code".into(), fields.error_code.clone()),
        ("cause".into(), fields.cause.clone()),
    ]);
    if let Some(delay_ms) = fields.delay_ms {
        metadata.insert("delay_ms".into(), format!("{:.0}", delay_ms.round()));
    }
    if let Some(server_paced) = fields.server_paced {
        metadata.insert("server_paced".into(), server_paced.to_string());
    }
    projection(
        if fields.outcome == "retried" { "info" } else { "warn" },
        TURN_RETRY_EVENT,
        metadata,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserMessageReceivedFields {
    pub conversation_id: String,
    pub was_in_flight: bool,
}

pub fn user_message_received_telemetry(
    fields: &UserMessageReceivedFields,
) -> HostTelemetryProjection {
    projection(
        "info",
        USER_MESSAGE_RECEIVED_EVENT,
        BTreeMap::from([
            ("conversation_id".into(), fields.conversation_id.clone()),
            ("was_in_flight".into(), fields.was_in_flight.to_string()),
        ]),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosingSendNudgeFields {
    pub conversation_id: String,
    pub delivered: bool,
    pub sent_message_count: u64,
    pub aborted: bool,
}

pub fn closing_send_nudge_telemetry(fields: &ClosingSendNudgeFields) -> HostTelemetryProjection {
    projection(
        if !fields.delivered && !fields.aborted {
            "warn"
        } else {
            "info"
        },
        CLOSING_SEND_NUDGE_EVENT,
        BTreeMap::from([
            ("conversation_id".into(), fields.conversation_id.clone()),
            ("delivered".into(), fields.delivered.to_string()),
            (
                "sent_message_count".into(),
                fields.sent_message_count.to_string(),
            ),
            ("aborted".into(), fields.aborted.to_string()),
        ]),
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct TtftFields {
    pub conversation_id: String,
    pub ttft_ms: Option<f64>,
    pub skew: bool,
    pub skew_reason: String,
    pub chunk_type: String,
    pub is_fork: bool,
    pub model_id: String,
    pub trace_id: String,
    pub span_id: String,
}

pub fn ttft_telemetry(fields: &TtftFields) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("conversation_id".into(), fields.conversation_id.clone()),
        ("skew".into(), fields.skew.to_string()),
        ("skew_reason".into(), fields.skew_reason.clone()),
        ("chunk_type".into(), fields.chunk_type.clone()),
        ("is_fork".into(), fields.is_fork.to_string()),
        ("model_id".into(), fields.model_id.clone()),
        ("trace_id".into(), fields.trace_id.clone()),
        ("span_id".into(), fields.span_id.clone()),
    ]);
    if let Some(ttft_ms) = fields.ttft_ms {
        metadata.insert("ttft_ms".into(), format!("{:.0}", ttft_ms.round()));
    }
    projection("info", TTFT_EVENT, metadata)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnUsageFields {
    pub conversation_id: String,
    pub source: String,
    pub request_id: Option<String>,
    pub request_id_count: u64,
    pub turn_ended_seq: u64,
    pub usage: Option<TokenUsage>,
}

pub fn turn_usage_telemetry(fields: &TurnUsageFields) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("schema_version".into(), TURN_USAGE_SCHEMA_VERSION.into()),
        ("conversation_id".into(), fields.conversation_id.clone()),
        ("source".into(), fields.source.clone()),
        (
            "has_request_id".into(),
            fields.request_id.is_some().to_string(),
        ),
        ("request_id_count".into(), fields.request_id_count.to_string()),
        ("turn_ended_seq".into(), fields.turn_ended_seq.to_string()),
        ("has_usage".into(), fields.usage.is_some().to_string()),
    ]);
    if let Some(request_id) = &fields.request_id {
        metadata.insert("request_id".into(), request_id.clone());
    }
    metadata.extend(usage_token_tags(fields.usage));
    projection("info", TURN_USAGE_EVENT, metadata)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputerUseUsageFields {
    pub parent_agent_id: String,
    pub subagent_agent_id: String,
    pub subagent_type: String,
    pub subagent_request_id: String,
    pub model_id: String,
    pub outcome: String,
    pub duration_ms: f64,
    pub tool_call_count: u64,
    pub turn_ended_count: u64,
    pub usage: Option<TokenUsage>,
}

pub fn computer_use_usage_telemetry(
    fields: &ComputerUseUsageFields,
) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("schema_version".into(), "1".into()),
        ("parent_agent_id".into(), fields.parent_agent_id.clone()),
        ("subagent_agent_id".into(), fields.subagent_agent_id.clone()),
        ("subagent_type".into(), fields.subagent_type.clone()),
        ("request_id".into(), fields.subagent_request_id.clone()),
        ("model_id".into(), fields.model_id.clone()),
        ("outcome".into(), fields.outcome.clone()),
        (
            "duration_ms".into(),
            format!("{:.0}", fields.duration_ms.round()),
        ),
        ("tool_call_count".into(), fields.tool_call_count.to_string()),
        (
            "turn_ended_count".into(),
            fields.turn_ended_count.to_string(),
        ),
        ("has_usage".into(), fields.usage.is_some().to_string()),
    ]);
    metadata.extend(usage_token_tags(fields.usage));
    projection(
        if fields.outcome == "error" { "warn" } else { "info" },
        COMPUTER_USE_USAGE_EVENT,
        metadata,
    )
}
