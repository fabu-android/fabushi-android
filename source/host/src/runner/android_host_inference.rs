use super::stream_attempt::{
    ProviderFailure, StreamAttemptInput, StreamGeneration, TurnStreamProvider,
};
use serde_json::{json, Map, Value};
use std::{
    io::{BufRead, BufReader},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub const DEFAULT_DACHENG_RESPONSES_BASE_URL: &str =
    "https://api.ombhrum.com/codex-deepseek/v1";
pub const DEFAULT_DEEPSEEK_MODEL: &str = "deepseek-chat";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AndroidInferenceMode {
    Production,
    Test,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AndroidSubagentReviewDecision {
    Allow,
    Block {
        reason: String,
        proposed_rule: Option<String>,
    },
    Reject {
        reason: String,
    },
}

pub trait AndroidRoutedToolBridge: Send + Sync {
    fn list_tools(&self) -> Result<Vec<Value>, String>;
    fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String>;
}

#[derive(Clone)]
pub struct AndroidHostInferenceProvider {
    mode: AndroidInferenceMode,
    bearer_token: Option<String>,
    base_url: String,
    default_model: String,
    cancelled: Arc<AtomicBool>,
    routed_tools: Option<Arc<dyn AndroidRoutedToolBridge>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ProviderSseEvent {
    Delta(String),
    ResponseId(String),
    ToolCall {
        call_id: String,
        name: String,
        arguments: Value,
    },
    Completed(Option<String>),
    Failed(String),
    Ignore,
}

impl AndroidHostInferenceProvider {
    pub fn new(mode: AndroidInferenceMode) -> Self {
        Self {
            mode,
            bearer_token: None,
            base_url: DEFAULT_DACHENG_RESPONSES_BASE_URL.into(),
            default_model: DEFAULT_DEEPSEEK_MODEL.into(),
            cancelled: Arc::new(AtomicBool::new(false)),
            routed_tools: None,
        }
    }

    pub fn production(
        bearer_token: String,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, ProviderFailure> {
        if !valid_bearer_token(&bearer_token) {
            return Err(ProviderFailure::new("provider_credentials_unavailable"));
        }
        Ok(Self {
            mode: AndroidInferenceMode::Production,
            bearer_token: Some(bearer_token),
            base_url: DEFAULT_DACHENG_RESPONSES_BASE_URL.into(),
            default_model: DEFAULT_DEEPSEEK_MODEL.into(),
            cancelled,
            routed_tools: None,
        })
    }

    pub fn resolve_model_id(requested_model: &str) -> String {
        let requested_model = requested_model.trim();
        if requested_model.is_empty() || requested_model == "default" {
            DEFAULT_DEEPSEEK_MODEL.to_string()
        } else {
            requested_model.to_string()
        }
    }

    pub fn run_summarization_prompt(
        mode: AndroidInferenceMode,
        bearer_token: Option<String>,
        cancelled: Arc<AtomicBool>,
        system_prompt: &str,
        user_prompt: &str,
        should_cancel: &dyn Fn() -> bool,
    ) -> Result<String, ProviderFailure> {
        Self::run_summarization_prompt_with_model(
            mode,
            bearer_token,
            cancelled,
            DEFAULT_DEEPSEEK_MODEL,
            system_prompt,
            user_prompt,
            should_cancel,
        )
    }

    pub fn run_summarization_prompt_with_model(
        mode: AndroidInferenceMode,
        bearer_token: Option<String>,
        cancelled: Arc<AtomicBool>,
        model_id: &str,
        system_prompt: &str,
        user_prompt: &str,
        should_cancel: &dyn Fn() -> bool,
    ) -> Result<String, ProviderFailure> {
        if should_cancel() || cancelled.load(Ordering::Acquire) {
            return Err(ProviderFailure::new("cancelled"));
        }
        let mut provider = match mode {
            AndroidInferenceMode::Test => Self::new(AndroidInferenceMode::Test),
            AndroidInferenceMode::Production => {
                Self::production(
                    bearer_token.ok_or_else(|| {
                        ProviderFailure::new("provider_credentials_unavailable")
                    })?,
                    Arc::clone(&cancelled),
                )?
            }
        };
        let prompt = format!("{system_prompt}\n\n{user_prompt}");
        let input = StreamAttemptInput {
            operation_id: "summarization".into(),
            agent_id: "mahayana-summarizer".into(),
            model: Self::resolve_model_id(model_id),
            prompt,
            resume_checkpoint_available: false,
        };
        let mut output = String::new();
        provider.run_stream(&input, &mut |chunk| {
            if should_cancel() || cancelled.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            output.push_str(chunk);
            Ok(())
        })?;
        Ok(output)
    }

    pub fn run_subagent_review(
        mode: AndroidInferenceMode,
        bearer_token: Option<String>,
        cancelled: Arc<AtomicBool>,
        action: &str,
        prompt: &str,
        subagent_id: Option<&str>,
        subagent_type: Option<&str>,
    ) -> Result<AndroidSubagentReviewDecision, ProviderFailure> {
        let action = action.trim();
        let prompt = prompt.trim();
        if action.is_empty() || prompt.is_empty() {
            return Err(ProviderFailure::new("subagent auto-review target is invalid"));
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(ProviderFailure::new("cancelled"));
        }
        if mode == AndroidInferenceMode::Test {
            if prompt.contains("[[review:error]]") {
                return Err(ProviderFailure::new("test subagent auto-review failure"));
            }
            if prompt.contains("[[review:reject]]") {
                return Ok(AndroidSubagentReviewDecision::Reject {
                    reason: "Generated-subagent auto-review rejected this action.".into(),
                });
            }
            if prompt.contains("[[review:block]]") || prompt.contains("[[review:deny]]") {
                return Ok(AndroidSubagentReviewDecision::Block {
                    reason: "Blocked by generated-subagent auto-review.".into(),
                    proposed_rule: prompt
                        .contains("[[review:rule]]")
                        .then(|| "Allow this generated-subagent action when explicitly approved.".into()),
                });
            }
            return Ok(AndroidSubagentReviewDecision::Allow);
        }

        let mut provider = Self::production(
            bearer_token.ok_or_else(|| ProviderFailure::new("provider_credentials_unavailable"))?,
            Arc::clone(&cancelled),
        )?;
        let target = json!({
            "action":"sand_subagent",
            "arguments":{
                "action":action,
                "prompt":prompt,
                "subagent_id":subagent_id,
                "subagent_type":subagent_type,
            }
        });
        let review_prompt = build_subagent_review_prompt(&target);
        let input = StreamAttemptInput {
            operation_id: "subagent-auto-review".into(),
            agent_id: "mahayana-subagent-review".into(),
            model: provider.default_model.clone(),
            prompt: review_prompt,
            resume_checkpoint_available: false,
        };
        let mut output = String::new();
        provider.run_stream(&input, &mut |chunk| {
            if cancelled.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            output.push_str(chunk);
            Ok(())
        })?;
        parse_subagent_review_decision(&output)
    }

    #[cfg(test)]
    fn with_endpoint(
        bearer_token: String,
        base_url: String,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        Self {
            mode: AndroidInferenceMode::Production,
            bearer_token: Some(bearer_token),
            base_url,
            default_model: DEFAULT_DEEPSEEK_MODEL.into(),
            cancelled,
            routed_tools: None,
        }
    }

    pub fn with_routed_tools(mut self, bridge: Arc<dyn AndroidRoutedToolBridge>) -> Self {
        self.routed_tools = Some(bridge);
        self
    }

    fn run_stream(
        &mut self,
        input: &StreamAttemptInput,
        on_chunk: &mut dyn FnMut(&str) -> Result<(), String>,
    ) -> Result<StreamGeneration, ProviderFailure> {
        if input.prompt.trim().is_empty() {
            return Err(ProviderFailure::new("prompt is required"));
        }
        if self.mode == AndroidInferenceMode::Test {
            if let Some(bridge) = self.routed_tools.as_ref() {
                if let Some(script) = input.prompt.strip_prefix("[[tool:") {
                    if let Some((name, rest)) = script.split_once("]]") {
                        let args = rest.trim();
                        let args = if args.is_empty() {
                            json!({})
                        } else {
                            serde_json::from_str(args).map_err(|_| {
                                ProviderFailure::new("test routed tool arguments are invalid JSON")
                            })?
                        };
                        let result = bridge
                            .call_tool(name.trim(), args, "test-tool-call-1")
                            .map_err(ProviderFailure::new)?;
                        let text = format!("tool_result:{}", result);
                        on_chunk(&text).map_err(ProviderFailure::new)?;
                        return Ok(StreamGeneration {
                            first_token_delay_ms: 1,
                            chunks: vec![text],
                            finish_reason: "stop".into(),
                        });
                    }
                }
            }
            let text = "自动化测试状态正常。";
            on_chunk(text).map_err(ProviderFailure::new)?;
            return Ok(StreamGeneration {
                first_token_delay_ms: 1,
                chunks: vec![text.into()],
                finish_reason: "stop".into(),
            });
        }

        let token = self
            .bearer_token
            .as_deref()
            .filter(|value| valid_bearer_token(value))
            .ok_or_else(|| ProviderFailure::new("provider_credentials_unavailable"))?;
        let resolved_model = Self::resolve_model_id(&input.model);
        let model = resolved_model.as_str();
        let endpoint = format!("{}/responses", self.base_url.trim_end_matches('/'));
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(20))
            .timeout_read(Duration::from_secs(180))
            .timeout_write(Duration::from_secs(30))
            .build();
        let tool_definitions = match self.routed_tools.as_ref() {
            Some(bridge) => bridge.list_tools().map_err(ProviderFailure::new)?,
            None => Vec::new(),
        };
        let started = Instant::now();
        let mut all_chunks = Vec::new();
        let mut first_token_delay_ms = None;
        let mut previous_response_id: Option<String> = None;
        let mut next_input = Value::String(input.prompt.clone());

        for _tool_step in 0..8 {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(ProviderFailure {
                    message: "cancelled".into(),
                    retry_after_ms: None,
                    first_token_stall: false,
                    stream_output_produced: !all_chunks.is_empty(),
                });
            }
            let mut body = Map::new();
            body.insert("model".into(), Value::String(model.to_string()));
            body.insert("input".into(), next_input);
            body.insert("stream".into(), Value::Bool(true));
            if !tool_definitions.is_empty() {
                body.insert("tools".into(), Value::Array(tool_definitions.clone()));
            }
            if let Some(previous) = previous_response_id.as_ref() {
                body.insert("previous_response_id".into(), Value::String(previous.clone()));
            }
            let response = agent
                .post(&endpoint)
                .set("Accept", "text/event-stream")
                .set("Authorization", &format!("Bearer {token}"))
                .send_json(Value::Object(body));

            let response = match response {
                Ok(response) => response,
                Err(ureq::Error::Status(status, response)) => {
                    let retry_after_ms = response
                        .header("Retry-After")
                        .and_then(parse_retry_after_ms);
                    let message = if status == 429 || status >= 500 {
                        format!("provider overloaded http_{status}")
                    } else {
                        format!("provider_http_{status}")
                    };
                    return Err(ProviderFailure {
                        message,
                        retry_after_ms,
                        first_token_stall: false,
                        stream_output_produced: !all_chunks.is_empty(),
                    });
                }
                Err(ureq::Error::Transport(error)) => {
                    return Err(ProviderFailure {
                        message: format!("network error: {}", safe_transport_kind(&error)),
                        retry_after_ms: None,
                        first_token_stall: false,
                        stream_output_produced: !all_chunks.is_empty(),
                    });
                }
            };

            let mut reader = BufReader::new(response.into_reader());
            let mut line = String::new();
            let mut completed = false;
            let mut response_id = previous_response_id.clone();
            let mut tool_calls = Vec::new();

            loop {
                if self.cancelled.load(Ordering::Acquire) {
                    return Err(ProviderFailure {
                        message: "cancelled".into(),
                        retry_after_ms: None,
                        first_token_stall: false,
                        stream_output_produced: !all_chunks.is_empty(),
                    });
                }
                line.clear();
                let bytes = reader.read_line(&mut line).map_err(|_| ProviderFailure {
                    message: "network error: stream read failed".into(),
                    retry_after_ms: None,
                    first_token_stall: false,
                    stream_output_produced: !all_chunks.is_empty(),
                })?;
                if bytes == 0 {
                    break;
                }
                let trimmed = line.trim_end_matches(['\r', '\n']);
                let Some(data) = trimmed.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data.is_empty() {
                    continue;
                }
                match parse_sse_data(data) {
                    Ok(ProviderSseEvent::Delta(delta)) => {
                        if delta.is_empty() {
                            continue;
                        }
                        if first_token_delay_ms.is_none() {
                            first_token_delay_ms = Some(
                                started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
                            );
                        }
                        on_chunk(&delta).map_err(|message| ProviderFailure {
                            message,
                            retry_after_ms: None,
                            first_token_stall: false,
                            stream_output_produced: !all_chunks.is_empty(),
                        })?;
                        all_chunks.push(delta);
                    }
                    Ok(ProviderSseEvent::ResponseId(id)) => response_id = Some(id),
                    Ok(ProviderSseEvent::ToolCall { call_id, name, arguments }) => {
                        tool_calls.push((call_id, name, arguments));
                    }
                    Ok(ProviderSseEvent::Completed(id)) => {
                        if let Some(id) = id {
                            response_id = Some(id);
                        }
                        completed = true;
                        break;
                    }
                    Ok(ProviderSseEvent::Failed(message)) => {
                        return Err(ProviderFailure {
                            message,
                            retry_after_ms: None,
                            first_token_stall: false,
                            stream_output_produced: !all_chunks.is_empty(),
                        });
                    }
                    Ok(ProviderSseEvent::Ignore) => {}
                    Err(message) => {
                        return Err(ProviderFailure {
                            message,
                            retry_after_ms: None,
                            first_token_stall: false,
                            stream_output_produced: !all_chunks.is_empty(),
                        });
                    }
                }
            }

            if !completed && all_chunks.is_empty() && tool_calls.is_empty() {
                return Err(ProviderFailure::new(
                    "network error: provider stream closed before output",
                ));
            }
            if tool_calls.is_empty() {
                return Ok(StreamGeneration {
                    first_token_delay_ms: first_token_delay_ms.unwrap_or_else(|| {
                        started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
                    }),
                    chunks: all_chunks,
                    finish_reason: if completed { "stop" } else { "eof" }.into(),
                });
            }
            let bridge = self.routed_tools.as_ref().ok_or_else(|| {
                ProviderFailure::new("provider requested a tool but no routed tool bridge is bound")
            })?;
            let mut outputs = Vec::with_capacity(tool_calls.len());
            for (call_id, name, arguments) in tool_calls {
                if self.cancelled.load(Ordering::Acquire) {
                    return Err(ProviderFailure::new("cancelled"));
                }
                let result = bridge
                    .call_tool(&name, arguments, &call_id)
                    .map_err(ProviderFailure::new)?;
                outputs.push(json!({
                    "type":"function_call_output",
                    "call_id":call_id,
                    "output": if result.is_string() {
                        result.as_str().unwrap_or_default().to_string()
                    } else {
                        result.to_string()
                    }
                }));
            }
            previous_response_id = Some(response_id.ok_or_else(|| {
                ProviderFailure::new("provider tool call response omitted response identity")
            })?);
            next_input = Value::Array(outputs);
        }

        Err(ProviderFailure::new(
            "provider exceeded routed tool-call step limit",
        ))
    }
}

impl TurnStreamProvider for AndroidHostInferenceProvider {
    fn start_stream(
        &mut self,
        input: &StreamAttemptInput,
        _attempt: usize,
    ) -> Result<StreamGeneration, ProviderFailure> {
        self.run_stream(input, &mut |_| Ok(()))
    }

    fn start_stream_with_sink(
        &mut self,
        input: &StreamAttemptInput,
        _attempt: usize,
        on_chunk: &mut dyn FnMut(&str) -> Result<(), String>,
    ) -> Result<StreamGeneration, ProviderFailure> {
        self.run_stream(input, on_chunk)
    }

    fn cancel(&mut self, _operation_id: &str) -> Result<(), String> {
        self.cancelled.store(true, Ordering::Release);
        Ok(())
    }
}

fn build_subagent_review_prompt(target: &Value) -> String {
    format!(
        "Classify this generated-subagent action for safety and authorization. \
Return exactly one JSON object and no markdown using the canonical Auto-review contract: \
{{\"decision\":\"allow\"}}, \
{{\"decision\":\"block\",\"reason\":\"concise reason\",\"proposed_rule\":\"optional narrowly-scoped allow rule\"}}, or \
{{\"decision\":\"reject\",\"reason\":\"concise classifier failure or uncertainty reason\"}}. \
Fail closed when uncertain. Target: {target}"
    )
}

fn parse_subagent_review_decision(
    raw: &str,
) -> Result<AndroidSubagentReviewDecision, ProviderFailure> {
    let value: Value = serde_json::from_str(raw.trim())
        .map_err(|_| ProviderFailure::new("subagent auto-review returned invalid JSON"))?;
    let decision = value
        .get("decision")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    match decision {
        "allow" => Ok(AndroidSubagentReviewDecision::Allow),
        "block" => {
            let reason = value
                .get("reason")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    ProviderFailure::new("subagent auto-review block decision omitted reason")
                })?;
            let proposed_rule = value
                .get("proposed_rule")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            Ok(AndroidSubagentReviewDecision::Block {
                reason: reason.to_string(),
                proposed_rule,
            })
        }
        "reject" => {
            let reason = value
                .get("reason")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    ProviderFailure::new("subagent auto-review reject decision omitted reason")
                })?;
            Ok(AndroidSubagentReviewDecision::Reject {
                reason: reason.to_string(),
            })
        }
        _ => Err(ProviderFailure::new(
            "subagent auto-review returned invalid decision",
        )),
    }
}

fn valid_bearer_token(token: &str) -> bool {
    (24..=16 * 1024).contains(&token.len())
        && !token.chars().any(char::is_whitespace)
        && !token.contains(['\r', '\n'])
}

fn parse_retry_after_ms(value: &str) -> Option<u64> {
    value.trim().parse::<u64>().ok().map(|seconds| seconds.saturating_mul(1_000))
}

fn safe_transport_kind(error: &ureq::Transport) -> &'static str {
    use ureq::ErrorKind;
    match error.kind() {
        ErrorKind::ConnectionFailed => "connection failed",
        ErrorKind::Dns => "dns failure",
        ErrorKind::Io => "io failure",
        ErrorKind::InvalidUrl => "invalid url",
        ErrorKind::ProxyConnect => "proxy failure",
        ErrorKind::TooManyRedirects => "redirect failure",
        _ => "transport failure",
    }
}

fn parse_sse_data(data: &str) -> Result<ProviderSseEvent, String> {
    if data == "[DONE]" {
        return Ok(ProviderSseEvent::Completed(None));
    }
    let value: Value =
        serde_json::from_str(data).map_err(|_| "provider stream returned invalid JSON".to_string())?;
    let event_type = value.get("type").and_then(Value::as_str).unwrap_or_default();
    match event_type {
        "response.created" | "response.in_progress" => Ok(value
            .pointer("/response/id")
            .and_then(Value::as_str)
            .map(|id| ProviderSseEvent::ResponseId(id.to_string()))
            .unwrap_or(ProviderSseEvent::Ignore)),
        "response.output_item.done" => {
            let item = value.get("item").unwrap_or(&Value::Null);
            if item.get("type").and_then(Value::as_str) == Some("function_call") {
                let call_id = item
                    .get("call_id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "provider tool call omitted call_id".to_string())?;
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "provider tool call omitted name".to_string())?;
                let raw = item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                let arguments = serde_json::from_str(raw)
                    .map_err(|_| "provider tool call arguments were invalid JSON".to_string())?;
                Ok(ProviderSseEvent::ToolCall {
                    call_id: call_id.to_string(),
                    name: name.to_string(),
                    arguments,
                })
            } else {
                Ok(ProviderSseEvent::Ignore)
            }
        }
        "response.output_text.delta" => Ok(ProviderSseEvent::Delta(
            value
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        )),
        "response.completed" => Ok(ProviderSseEvent::Completed(
            value
                .pointer("/response/id")
                .and_then(Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .map(str::to_string),
        )),
        "response.failed" | "response.incomplete" => {
            let message = value
                .pointer("/response/error/message")
                .or_else(|| value.pointer("/error/message"))
                .and_then(Value::as_str)
                .unwrap_or("provider response failed");
            Ok(ProviderSseEvent::Failed(message.to_string()))
        }
        _ => {
            if let Some(delta) = value
                .pointer("/choices/0/delta/content")
                .and_then(Value::as_str)
            {
                return Ok(ProviderSseEvent::Delta(delta.to_string()));
            }
            Ok(ProviderSseEvent::Ignore)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> StreamAttemptInput {
        StreamAttemptInput {
            operation_id: "op".into(),
            agent_id: "agent".into(),
            model: "default".into(),
            prompt: "hello".into(),
            resume_checkpoint_available: false,
        }
    }

    #[test]
    fn test_summarization_binding_uses_same_android_provider_and_honours_cancellation() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let summary = AndroidHostInferenceProvider::run_summarization_prompt(
            AndroidInferenceMode::Test,
            None,
            Arc::clone(&cancelled),
            "system",
            "user",
            &|| false,
        )
        .unwrap();
        assert_eq!(summary, "自动化测试状态正常。");

        cancelled.store(true, Ordering::Release);
        assert!(AndroidHostInferenceProvider::run_summarization_prompt(
            AndroidInferenceMode::Test,
            None,
            cancelled,
            "system",
            "user",
            &|| false,
        )
        .unwrap_err()
        .message
        .contains("cancelled"));
    }

    #[test]
    fn test_subagent_auto_review_prompt_matches_parser_contract() {
        let prompt = build_subagent_review_prompt(&json!({
            "action":"sand_subagent",
            "arguments":{"action":"launch","prompt":"work","subagent_type":"executor"}
        }));
        assert!(prompt.contains(r#"\"decision\":\"allow\""#));
        assert!(prompt.contains(r#"\"decision\":\"block\""#));
        assert!(prompt.contains(r#"\"decision\":\"reject\""#));
        assert!(prompt.contains("proposed_rule"));
        assert!(!prompt.contains(r#"\"decision\":\"deny\""#));
    }

    #[test]
    fn test_subagent_auto_review_is_fail_closed_and_deterministic() {
        let allow = AndroidHostInferenceProvider::run_subagent_review(
            AndroidInferenceMode::Test,
            None,
            Arc::new(AtomicBool::new(false)),
            "launch",
            "safe child task",
            None,
            Some("executor"),
        )
        .unwrap();
        assert_eq!(allow, AndroidSubagentReviewDecision::Allow);

        let blocked = AndroidHostInferenceProvider::run_subagent_review(
            AndroidInferenceMode::Test,
            None,
            Arc::new(AtomicBool::new(false)),
            "steer",
            "[[review:block]][[review:rule]] unsafe steer",
            Some("generated:child"),
            None,
        )
        .unwrap();
        assert!(matches!(
            blocked,
            AndroidSubagentReviewDecision::Block {
                proposed_rule: Some(_),
                ..
            }
        ));

        let rejected = AndroidHostInferenceProvider::run_subagent_review(
            AndroidInferenceMode::Test,
            None,
            Arc::new(AtomicBool::new(false)),
            "launch",
            "[[review:reject]] forbidden child",
            None,
            Some("executor"),
        )
        .unwrap();
        assert!(matches!(
            rejected,
            AndroidSubagentReviewDecision::Reject { .. }
        ));

        let error = AndroidHostInferenceProvider::run_subagent_review(
            AndroidInferenceMode::Test,
            None,
            Arc::new(AtomicBool::new(false)),
            "launch",
            "[[review:error]] classifier unavailable",
            None,
            Some("executor"),
        )
        .unwrap_err();
        assert!(error.message.contains("auto-review failure"));

        assert!(parse_subagent_review_decision(r#"{"decision":"block"}"#).is_err());
        assert!(parse_subagent_review_decision(r#"{"decision":"reject"}"#).is_err());
        assert_eq!(
            parse_subagent_review_decision(
                r#"{"decision":"block","reason":"review","proposed_rule":"allow once"}"#
            )
            .unwrap(),
            AndroidSubagentReviewDecision::Block {
                reason: "review".into(),
                proposed_rule: Some("allow once".into()),
            }
        );
        assert!(parse_subagent_review_decision("not-json").is_err());
    }

    struct EchoTools;

    impl AndroidRoutedToolBridge for EchoTools {
        fn list_tools(&self) -> Result<Vec<Value>, String> {
            Ok(vec![json!({
                "type":"function",
                "name":"Echo",
                "description":"Echo a value",
                "parameters":{"type":"object","properties":{"value":{"type":"string"}}}
            })])
        }

        fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String> {
            if name != "Echo" || tool_call_id.trim().is_empty() {
                return Err("unexpected test tool".into());
            }
            Ok(json!({"echo":args.get("value").cloned().unwrap_or(Value::Null)}))
        }
    }

    #[test]
    fn test_routed_provider_calls_tools_and_feeds_results_back_into_turn_output() {
        let mut provider = AndroidHostInferenceProvider::new(AndroidInferenceMode::Test)
            .with_routed_tools(Arc::new(EchoTools));
        let mut live = String::new();
        let output = provider
            .start_stream_with_sink(
                &StreamAttemptInput {
                    prompt: "[[tool:Echo]] {\"value\":\"ok\"}".into(),
                    ..input()
                },
                1,
                &mut |delta| {
                    live.push_str(delta);
                    Ok(())
                },
            )
            .unwrap();
        assert!(output.chunks.concat().contains("\"echo\":\"ok\""));
        assert_eq!(live, output.chunks.concat());
    }

    #[test]
    fn test_provider_streams_deterministically() {
        let mut provider = AndroidHostInferenceProvider::new(AndroidInferenceMode::Test);
        let mut live = String::new();
        let output = provider
            .start_stream_with_sink(&input(), 1, &mut |delta| {
                live.push_str(delta);
                Ok(())
            })
            .unwrap();
        assert_eq!(output.chunks.concat(), "自动化测试状态正常。");
        assert_eq!(live, "自动化测试状态正常。");
    }

    #[test]
    fn responses_sse_parser_handles_delta_terminal_and_failure_without_credentials() {
        assert_eq!(
            parse_sse_data(
                r#"{"type":"response.output_text.delta","delta":"hello"}"#
            )
            .unwrap(),
            ProviderSseEvent::Delta("hello".into())
        );
        assert_eq!(
            parse_sse_data(r#"{"type":"response.completed"}"#).unwrap(),
            ProviderSseEvent::Completed(None)
        );
        assert_eq!(
            parse_sse_data(r#"{"type":"response.completed","response":{"id":"resp-1"}}"#).unwrap(),
            ProviderSseEvent::Completed(Some("resp-1".into()))
        );
        assert_eq!(
            parse_sse_data(
                r#"{"type":"response.failed","response":{"error":{"message":"capacity"}}}"#
            )
            .unwrap(),
            ProviderSseEvent::Failed("capacity".into())
        );
        assert_eq!(
            parse_sse_data(r#"{"choices":[{"delta":{"content":"x"}}]}"#).unwrap(),
            ProviderSseEvent::Delta("x".into())
        );
    }

    #[test]
    fn production_provider_requires_a_bounded_non_whitespace_bearer() {
        let cancel = Arc::new(AtomicBool::new(false));
        assert!(AndroidHostInferenceProvider::production("short".into(), cancel.clone()).is_err());
        assert!(
            AndroidHostInferenceProvider::production("a".repeat(32), cancel).is_ok()
        );
    }

    #[test]
    fn cancellation_is_shared_with_host_owner() {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut provider = AndroidHostInferenceProvider::with_endpoint(
            "a".repeat(32),
            "https://api.ombhrum.com/codex-deepseek/v1".into(),
            cancel.clone(),
        );
        provider.cancel("op").unwrap();
        assert!(cancel.load(Ordering::Acquire));
    }
}
