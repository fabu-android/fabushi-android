use super::{
    agent_v1_wire::{
        encode_routed_tool_call_initial_observation, encode_routed_tool_call_observation,
        is_agent_v1_observed_tool,
    },
    production_turn_agent_owner::SAND_AGENT_MAX_STEPS,
    stream_attempt::{ProviderFailure, StreamAttemptInput, StreamGeneration, TurnStreamProvider},
    turn_tool_session_reminders::TurnToolSessionReminderState,
};
use serde_json::{json, Map, Value};
use std::{
    io::{BufRead, BufReader},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const DEFAULT_DACHENG_RESPONSES_BASE_URL: &str =
    "https://api.ombhrum.com/codex-deepseek/v1";
pub const DEFAULT_SAND_MODEL_ID: &str = "grok-4.5";
pub const SAND_SUMMARIZATION_MODEL_ID: &str = "gemini-2.5-flash";
const SAND_AUTO_REVIEW_CLASSIFIER_TIMEOUT_SECS: u64 = 10;
const SAND_SUBAGENT_CLASSIFIER_ERROR_REASON: &str =
    "An error occurred while classifying this task action. Please review manually.";

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AndroidToolCallObservation {
    pub agent_id: String,
    pub operation_id: String,
    pub phase: String,
    pub tool_name: String,
    pub tool_call_id: String,
    pub protobuf_bytes: Vec<u8>,
    pub started_at_ms: u64,
    pub completed_at_ms: Option<u64>,
    pub succeeded: Option<bool>,
}

pub trait AndroidToolCallObservationSink: Send + Sync {
    fn record(&self, observation: &AndroidToolCallObservation) -> Result<(), String>;
}

#[derive(Clone)]
pub struct AndroidHostInferenceProvider {
    mode: AndroidInferenceMode,
    bearer_token: Option<String>,
    base_url: String,
    default_model: String,
    cancelled: Arc<AtomicBool>,
    routed_tools: Option<Arc<dyn AndroidRoutedToolBridge>>,
    tool_call_observation_sink: Option<Arc<dyn AndroidToolCallObservationSink>>,
    request_timeout: Option<Duration>,
    turn_tool_session_reminders_enabled: bool,
    silence_allowed: bool,
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
            default_model: DEFAULT_SAND_MODEL_ID.into(),
            cancelled: Arc::new(AtomicBool::new(false)),
            routed_tools: None,
            tool_call_observation_sink: None,
            request_timeout: None,
            turn_tool_session_reminders_enabled: false,
            silence_allowed: false,
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
            default_model: DEFAULT_SAND_MODEL_ID.into(),
            cancelled,
            routed_tools: None,
            tool_call_observation_sink: None,
            request_timeout: None,
            turn_tool_session_reminders_enabled: false,
            silence_allowed: false,
        })
    }

    pub fn resolve_model_id(requested_model: &str) -> String {
        let requested_model = requested_model.trim();
        if requested_model.is_empty() || requested_model == "default" {
            DEFAULT_SAND_MODEL_ID.to_string()
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
            SAND_SUMMARIZATION_MODEL_ID,
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
        let target = json!({
            "action":"sand_subagent",
            "arguments":{
                "action":action,
                "prompt":prompt,
                "subagent_id":subagent_id,
                "subagent_type":subagent_type,
            }
        });
        Self::run_auto_review_target(mode, bearer_token, cancelled, &target)
    }

    pub fn run_auto_review_target(
        mode: AndroidInferenceMode,
        bearer_token: Option<String>,
        cancelled: Arc<AtomicBool>,
        target: &Value,
    ) -> Result<AndroidSubagentReviewDecision, ProviderFailure> {
        if cancelled.load(Ordering::Acquire) {
            return Err(ProviderFailure::new("cancelled"));
        }
        let test_target = target.to_string();
        if mode == AndroidInferenceMode::Test {
            if test_target.contains("[[review:error]]") {
                return Err(ProviderFailure::new("test auto-review failure"));
            }
            if test_target.contains("[[review:reject]]") {
                return Ok(AndroidSubagentReviewDecision::Reject {
                    reason: "Auto-review rejected this action.".into(),
                });
            }
            if test_target.contains("[[review:block]]") || test_target.contains("[[review:deny]]") {
                return Ok(AndroidSubagentReviewDecision::Block {
                    reason: "Blocked by Auto-review.".into(),
                    proposed_rule: test_target
                        .contains("[[review:rule]]")
                        .then(|| "Allow this action when explicitly approved.".into()),
                });
            }
            return Ok(AndroidSubagentReviewDecision::Allow);
        }

        let mut provider = Self::production(
            bearer_token.ok_or_else(|| ProviderFailure::new("provider_credentials_unavailable"))?,
            Arc::clone(&cancelled),
        )?;
        provider.configure_subagent_review_timeout();
        let review_prompt = build_subagent_review_prompt(target);
        let input = StreamAttemptInput {
            operation_id: "auto-review".into(),
            agent_id: "mahayana-auto-review".into(),
            model: provider.default_model.clone(),
            prompt: review_prompt,
            resume_checkpoint_available: false,
        };
        let mut output = String::new();
        if let Err(error) = provider.run_stream(&input, &mut |chunk| {
            if cancelled.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            output.push_str(chunk);
            Ok(())
        }) {
            return Self::subagent_review_provider_failure(
                error,
                cancelled.load(Ordering::Acquire),
            );
        }
        Ok(Self::parse_subagent_review_fail_closed(&output))
    }

    fn configure_subagent_review_timeout(&mut self) {
        self.request_timeout =
            Some(Duration::from_secs(SAND_AUTO_REVIEW_CLASSIFIER_TIMEOUT_SECS));
    }

    fn subagent_review_provider_failure(
        error: ProviderFailure,
        cancelled: bool,
    ) -> Result<AndroidSubagentReviewDecision, ProviderFailure> {
        if cancelled || error.message == "cancelled" {
            return Err(error);
        }
        Ok(AndroidSubagentReviewDecision::Reject {
            reason: SAND_SUBAGENT_CLASSIFIER_ERROR_REASON.into(),
        })
    }

    fn parse_subagent_review_fail_closed(output: &str) -> AndroidSubagentReviewDecision {
        parse_subagent_review_decision(output).unwrap_or_else(|_| {
            AndroidSubagentReviewDecision::Reject {
                reason: SAND_SUBAGENT_CLASSIFIER_ERROR_REASON.into(),
            }
        })
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
            default_model: DEFAULT_SAND_MODEL_ID.into(),
            cancelled,
            routed_tools: None,
            tool_call_observation_sink: None,
            request_timeout: None,
            turn_tool_session_reminders_enabled: false,
            silence_allowed: false,
        }
    }

    pub fn with_routed_tools(mut self, bridge: Arc<dyn AndroidRoutedToolBridge>) -> Self {
        self.routed_tools = Some(bridge);
        self
    }

    pub fn with_tool_call_observation_sink(
        mut self,
        sink: Arc<dyn AndroidToolCallObservationSink>,
    ) -> Self {
        self.tool_call_observation_sink = Some(sink);
        self
    }

    pub fn with_turn_tool_session_reminders(mut self, enabled: bool) -> Self {
        self.turn_tool_session_reminders_enabled = enabled;
        self
    }

    pub fn with_silence_allowed(mut self, allowed: bool) -> Self {
        self.silence_allowed = allowed;
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
                            .map_err(|message| ProviderFailure {
                                message,
                                retry_after_ms: None,
                                first_token_stall: false,
                                stream_output_produced: true,
                            })?;
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
        let agent = match self.request_timeout {
            Some(timeout) => ureq::AgentBuilder::new()
                .timeout_connect(timeout)
                .timeout_read(timeout)
                .timeout_write(timeout)
                .build(),
            None => ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(20))
                .timeout_read(Duration::from_secs(180))
                .timeout_write(Duration::from_secs(30))
                .build(),
        };
        let tool_definitions = match self.routed_tools.as_ref() {
            Some(bridge) => bridge.list_tools().map_err(ProviderFailure::new)?,
            None => Vec::new(),
        };
        let started = Instant::now();
        let mut all_chunks = Vec::new();
        let mut first_token_delay_ms = None;
        let mut previous_response_id: Option<String> = None;
        let mut next_input = Value::String(input.prompt.clone());
        // Provider retries are only safe while no externally routed tool dispatch has started.
        // A tool bridge error can represent an outcome-unknown side effect, so count the attempt
        // itself as progress before invoking the bridge. This keeps StreamAttemptHost from blindly
        // replaying the whole Responses turn when a later provider/tool/network leg fails.
        let mut routed_tool_dispatch_started = false;
        let mut turn_tool_session_reminders = (self.turn_tool_session_reminders_enabled
            && !self.silence_allowed)
            .then(TurnToolSessionReminderState::default);

        for _tool_step in 0..SAND_AGENT_MAX_STEPS {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(ProviderFailure {
                    message: "cancelled".into(),
                    retry_after_ms: None,
                    first_token_stall: false,
                    stream_output_produced: !all_chunks.is_empty() || routed_tool_dispatch_started,
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
                    let message = provider_http_error_message(status);
                    return Err(ProviderFailure {
                        message,
                        retry_after_ms,
                        first_token_stall: false,
                        stream_output_produced: !all_chunks.is_empty() || routed_tool_dispatch_started,
                    });
                }
                Err(ureq::Error::Transport(error)) => {
                    return Err(ProviderFailure {
                        message: format!("network error: {}", safe_transport_kind(&error)),
                        retry_after_ms: None,
                        first_token_stall: false,
                        stream_output_produced: !all_chunks.is_empty() || routed_tool_dispatch_started,
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
                        stream_output_produced: !all_chunks.is_empty() || routed_tool_dispatch_started,
                    });
                }
                line.clear();
                let bytes = reader.read_line(&mut line).map_err(|_| ProviderFailure {
                    message: "network error: stream read failed".into(),
                    retry_after_ms: None,
                    first_token_stall: false,
                    stream_output_produced: !all_chunks.is_empty() || routed_tool_dispatch_started,
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
                            stream_output_produced: !all_chunks.is_empty() || routed_tool_dispatch_started,
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
                            stream_output_produced: !all_chunks.is_empty() || routed_tool_dispatch_started,
                        });
                    }
                    Ok(ProviderSseEvent::Ignore) => {}
                    Err(message) => {
                        return Err(ProviderFailure {
                            message,
                            retry_after_ms: None,
                            first_token_stall: false,
                            stream_output_produced: !all_chunks.is_empty() || routed_tool_dispatch_started,
                        });
                    }
                }
            }

            if !completed && all_chunks.is_empty() && tool_calls.is_empty() {
                return Err(ProviderFailure {
                    message: "network error: provider stream closed before output".into(),
                    retry_after_ms: None,
                    first_token_stall: false,
                    stream_output_produced: routed_tool_dispatch_started,
                });
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
            let observed_tool_step = tool_calls
                .iter()
                .map(|(_, name, arguments)| (name.clone(), arguments.clone()))
                .collect::<Vec<_>>();
            let mut outputs = Vec::with_capacity(tool_calls.len() + 1);
            for (call_id, name, arguments) in tool_calls {
                if self.cancelled.load(Ordering::Acquire) {
                    return Err(ProviderFailure::new("cancelled"));
                }

                let canonical_agent_v1_tool = is_agent_v1_observed_tool(&name);
                let observation_sink = if canonical_agent_v1_tool {
                    Some(self.tool_call_observation_sink.as_ref().ok_or_else(|| ProviderFailure {
                        message: "canonical agent.v1 tool observation sink is not bound".into(),
                        retry_after_ms: None,
                        first_token_stall: false,
                        stream_output_produced: !all_chunks.is_empty() || routed_tool_dispatch_started,
                    })?)
                } else {
                    None
                };

                let started_at_ms = unix_time_ms();
                if let Some(sink) = observation_sink {
                    let encoded = encode_routed_tool_call_initial_observation(
                        &name,
                        &arguments,
                        &call_id,
                        started_at_ms,
                    )
                    .map_err(|message| ProviderFailure {
                        message,
                        retry_after_ms: None,
                        first_token_stall: false,
                        stream_output_produced: !all_chunks.is_empty() || routed_tool_dispatch_started,
                    })?
                    .ok_or_else(|| ProviderFailure {
                        message: "canonical agent.v1 initial observation was not encoded".into(),
                        retry_after_ms: None,
                        first_token_stall: false,
                        stream_output_produced: !all_chunks.is_empty() || routed_tool_dispatch_started,
                    })?;
                    sink.record(&AndroidToolCallObservation {
                        agent_id: input.agent_id.clone(),
                        operation_id: input.operation_id.clone(),
                        phase: "initial".into(),
                        tool_name: name.clone(),
                        tool_call_id: call_id.clone(),
                        protobuf_bytes: encoded,
                        started_at_ms,
                        completed_at_ms: None,
                        succeeded: None,
                    })
                    .map_err(|message| ProviderFailure {
                        message: format!("canonical agent.v1 initial observation persistence failed: {message}"),
                        retry_after_ms: None,
                        first_token_stall: false,
                        stream_output_produced: !all_chunks.is_empty() || routed_tool_dispatch_started,
                    })?;
                }

                // From this point onward a bridge error can mean the side effect committed but
                // its acknowledgement was lost. Never allow StreamAttemptHost to replay the turn.
                routed_tool_dispatch_started = true;
                let dispatch = bridge.call_tool(&name, arguments.clone(), &call_id);
                let completed_at_ms = unix_time_ms().max(started_at_ms);

                if let Some(sink) = observation_sink {
                    let encoded = encode_routed_tool_call_observation(
                        &name,
                        &arguments,
                        dispatch.as_ref().map_err(String::as_str),
                        &call_id,
                        started_at_ms,
                        completed_at_ms,
                    )
                    .map_err(|message| ProviderFailure {
                        message,
                        retry_after_ms: None,
                        first_token_stall: false,
                        stream_output_produced: true,
                    })?
                    .ok_or_else(|| ProviderFailure {
                        message: "canonical agent.v1 tool observation was not encoded".into(),
                        retry_after_ms: None,
                        first_token_stall: false,
                        stream_output_produced: true,
                    })?;
                    sink.record(&AndroidToolCallObservation {
                        agent_id: input.agent_id.clone(),
                        operation_id: input.operation_id.clone(),
                        phase: "completed".into(),
                        tool_name: name.clone(),
                        tool_call_id: call_id.clone(),
                        protobuf_bytes: encoded,
                        started_at_ms,
                        completed_at_ms: Some(completed_at_ms),
                        succeeded: Some(dispatch.is_ok()),
                    })
                    .map_err(|message| ProviderFailure {
                        message: format!("canonical agent.v1 observation persistence failed: {message}"),
                        retry_after_ms: None,
                        first_token_stall: false,
                        stream_output_produced: true,
                    })?;
                }

                let result = dispatch.map_err(|message| ProviderFailure {
                    message,
                    retry_after_ms: None,
                    first_token_stall: false,
                    stream_output_produced: true,
                })?;
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
            previous_response_id = Some(response_id.ok_or_else(|| ProviderFailure {
                message: "provider tool call response omitted response identity".into(),
                retry_after_ms: None,
                first_token_stall: false,
                stream_output_produced: routed_tool_dispatch_started,
            })?);
            if let Some(reminder) = turn_tool_session_reminders
                .as_mut()
                .and_then(|state| state.observe_tool_step(&observed_tool_step))
            {
                outputs.push(json!({
                    "role":"user",
                    "content":reminder,
                }));
            }
            next_input = Value::Array(outputs);
        }

        Err(ProviderFailure {
            message: "provider exceeded routed tool-call step limit".into(),
            retry_after_ms: None,
            first_token_stall: false,
            stream_output_produced: routed_tool_dispatch_started || !all_chunks.is_empty(),
        })
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

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn valid_bearer_token(token: &str) -> bool {
    (24..=16 * 1024).contains(&token.len())
        && !token.chars().any(char::is_whitespace)
        && !token.contains(['\r', '\n'])
}

fn parse_retry_after_ms(value: &str) -> Option<u64> {
    value.trim().parse::<u64>().ok().map(|seconds| seconds.saturating_mul(1_000))
}

fn provider_http_error_message(status: u16) -> String {
    match status {
        401 | 403 => format!("provider_authentication_required http_{status}"),
        408 => format!("provider timeout http_{status}"),
        429 => format!("provider overloaded http_{status}"),
        status if status >= 500 => format!("provider overloaded http_{status}"),
        _ => format!("provider_http_{status}"),
    }
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
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::Mutex;

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
    fn provider_http_authentication_failures_have_a_stable_non_secret_contract() {
        assert_eq!(
            provider_http_error_message(401),
            "provider_authentication_required http_401"
        );
        assert_eq!(
            provider_http_error_message(403),
            "provider_authentication_required http_403"
        );
        assert_eq!(provider_http_error_message(408), "provider timeout http_408");
        assert_eq!(provider_http_error_message(429), "provider overloaded http_429");
        assert_eq!(provider_http_error_message(503), "provider overloaded http_503");
        assert_eq!(provider_http_error_message(400), "provider_http_400");
    }

    #[test]
    fn default_model_matches_current_desktop_sand_authority() {
        assert_eq!(DEFAULT_SAND_MODEL_ID, "grok-4.5");
        assert_eq!(SAND_SUMMARIZATION_MODEL_ID, "gemini-2.5-flash");
        assert_eq!(AndroidHostInferenceProvider::resolve_model_id("default"), "grok-4.5");
        assert_eq!(AndroidHostInferenceProvider::resolve_model_id("   "), "grok-4.5");
        assert_eq!(
            AndroidHostInferenceProvider::resolve_model_id("claude-opus-4-8"),
            "claude-opus-4-8",
        );
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
    fn subagent_auto_review_uses_desktop_timeout_and_fails_closed() {
        let mut provider = AndroidHostInferenceProvider::production(
            "a".repeat(32),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(provider.request_timeout, None);
        provider.configure_subagent_review_timeout();
        assert_eq!(
            provider.request_timeout,
            Some(Duration::from_secs(SAND_AUTO_REVIEW_CLASSIFIER_TIMEOUT_SECS))
        );

        let rejected = AndroidHostInferenceProvider::subagent_review_provider_failure(
            ProviderFailure::new("network unavailable"),
            false,
        )
        .unwrap();
        assert_eq!(
            rejected,
            AndroidSubagentReviewDecision::Reject {
                reason: SAND_SUBAGENT_CLASSIFIER_ERROR_REASON.into(),
            }
        );
        assert!(
            AndroidHostInferenceProvider::subagent_review_provider_failure(
                ProviderFailure::new("cancelled"),
                true,
            )
            .unwrap_err()
            .message
            .contains("cancelled")
        );
        assert_eq!(
            AndroidHostInferenceProvider::parse_subagent_review_fail_closed("not-json"),
            AndroidSubagentReviewDecision::Reject {
                reason: SAND_SUBAGENT_CLASSIFIER_ERROR_REASON.into(),
            }
        );
    }

    #[test]
    fn test_subagent_auto_review_prompt_matches_parser_contract() {
        let prompt = build_subagent_review_prompt(&json!({
            "action":"sand_subagent",
            "arguments":{"action":"launch","prompt":"work","subagent_type":"executor"}
        }));
        assert!(prompt.contains(r#""decision":"allow""#));
        assert!(prompt.contains(r#""decision":"block""#));
        assert!(prompt.contains(r#""decision":"reject""#));
        assert!(prompt.contains("proposed_rule"));
        assert!(!prompt.contains(r#""decision":"deny""#));
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
    fn routed_tool_failure_is_marked_as_non_retryable_progress() {
        struct FailingTools;
        impl AndroidRoutedToolBridge for FailingTools {
            fn list_tools(&self) -> Result<Vec<Value>, String> {
                Ok(vec![json!({
                    "type":"function",
                    "name":"Mutate",
                    "parameters":{"type":"object"}
                })])
            }

            fn call_tool(
                &self,
                _name: &str,
                _args: Value,
                _tool_call_id: &str,
            ) -> Result<Value, String> {
                Err("outcome unknown".into())
            }
        }

        let mut provider = AndroidHostInferenceProvider::new(AndroidInferenceMode::Test)
            .with_routed_tools(Arc::new(FailingTools));
        // The deterministic test transport calls the bridge directly; failure must be surfaced
        // without being converted into a successful generation.
        let error = provider
            .start_stream(
                &StreamAttemptInput {
                    prompt: "[[tool:Mutate]] {}".into(),
                    ..input()
                },
                1,
            )
            .unwrap_err();
        assert!(error.message.contains("outcome unknown"));
        assert!(
            error.stream_output_produced,
            "a routed tool dispatch attempt must suppress blind provider retry",
        );
    }

    #[derive(Default)]
    struct RecordingObservationSink {
        observations: Mutex<Vec<AndroidToolCallObservation>>,
    }

    impl AndroidToolCallObservationSink for RecordingObservationSink {
        fn record(&self, observation: &AndroidToolCallObservation) -> Result<(), String> {
            self.observations.lock().unwrap().push(observation.clone());
            Ok(())
        }
    }

    struct CanonicalProductionTools {
        result: Result<Value, String>,
        calls: Arc<AtomicUsize>,
    }

    impl AndroidRoutedToolBridge for CanonicalProductionTools {
        fn list_tools(&self) -> Result<Vec<Value>, String> {
            Ok(vec![
                json!({
                    "type":"function",
                    "name":"SendMessage",
                    "parameters":{"type":"object"}
                }),
                json!({
                    "type":"function",
                    "name":"UpdateAgent",
                    "parameters":{"type":"object"}
                }),
            ])
        }

        fn call_tool(
            &self,
            _name: &str,
            _args: Value,
            _tool_call_id: &str,
        ) -> Result<Value, String> {
            self.calls.fetch_add(1, AtomicOrdering::SeqCst);
            self.result.clone()
        }
    }

    fn spawn_sse_server(responses: Vec<String>) -> String {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for body in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0_u8; 4096];
                let mut header_end = None;
                loop {
                    let read = stream.read(&mut buffer).unwrap_or(0);
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..read]);
                    if header_end.is_none() {
                        header_end = request
                            .windows(4)
                            .position(|window| window == b"\r\n\r\n")
                            .map(|position| position + 4);
                    }
                    if let Some(end) = header_end {
                        let headers = String::from_utf8_lossy(&request[..end]);
                        let content_length = headers
                            .lines()
                            .find_map(|line| {
                                line.split_once(':').and_then(|(name, value)| {
                                    name.eq_ignore_ascii_case("content-length")
                                        .then(|| value.trim().parse::<usize>().ok())
                                        .flatten()
                                })
                            })
                            .unwrap_or(0);
                        if request.len() >= end + content_length {
                            break;
                        }
                    }
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
                stream.flush().unwrap();
            }
        });
        format!("http://{address}")
    }


    fn spawn_recording_sse_server(
        responses: Vec<String>,
    ) -> (String, Arc<Mutex<Vec<Value>>>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        std::thread::spawn(move || {
            for body in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0_u8; 4096];
                let mut header_end = None;
                loop {
                    let read = stream.read(&mut buffer).unwrap_or(0);
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..read]);
                    if header_end.is_none() {
                        header_end = request
                            .windows(4)
                            .position(|window| window == b"\r\n\r\n")
                            .map(|position| position + 4);
                    }
                    if let Some(end) = header_end {
                        let headers = String::from_utf8_lossy(&request[..end]);
                        let content_length = headers
                            .lines()
                            .find_map(|line| {
                                line.split_once(':').and_then(|(name, value)| {
                                    name.eq_ignore_ascii_case("content-length")
                                        .then(|| value.trim().parse::<usize>().ok())
                                        .flatten()
                                })
                            })
                            .unwrap_or(0);
                        if request.len() >= end + content_length {
                            if let Ok(value) =
                                serde_json::from_slice::<Value>(&request[end..end + content_length])
                            {
                                recorded.lock().unwrap().push(value);
                            }
                            break;
                        }
                    }
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
                stream.flush().unwrap();
            }
        });
        (format!("http://{address}"), requests)
    }

    fn tool_sse(name: &str, call_id: &str, arguments_json: &str) -> String {
        let item = json!({
            "type":"response.output_item.done",
            "item":{
                "type":"function_call",
                "call_id":call_id,
                "name":name,
                "arguments":arguments_json,
            }
        });
        let completed = json!({"type":"response.completed","response":{"id":"resp-tool"}});
        format!("data: {item}\n\ndata: {completed}\n\n")
    }

    fn final_sse() -> String {
        let delta = json!({"type":"response.output_text.delta","delta":"done"});
        let completed = json!({"type":"response.completed","response":{"id":"resp-final"}});
        format!("data: {delta}\n\ndata: {completed}\n\n")
    }

    #[test]
    fn production_send_message_dispatch_persists_canonical_agent_v1_observation() {
        use super::super::agent_v1_wire::{
            agent_tool_call, decode_agent_tool_call, send_message_args, send_message_result,
        };

        let endpoint = spawn_sse_server(vec![
            tool_sse(
                "SendMessage",
                "call-send-9",
                r#"{"message":{"type":"text","content":"hello"}}"#,
            ),
            final_sse(),
        ]);
        let calls = Arc::new(AtomicUsize::new(0));
        let sink = Arc::new(RecordingObservationSink::default());
        let mut provider = AndroidHostInferenceProvider::with_endpoint(
            "a".repeat(32),
            endpoint,
            Arc::new(AtomicBool::new(false)),
        )
        .with_routed_tools(Arc::new(CanonicalProductionTools {
            result: Ok(json!({"sent":true,"messageId":"msg-9"})),
            calls: Arc::clone(&calls),
        }))
        .with_tool_call_observation_sink(sink.clone());

        let output = provider.start_stream(&input(), 1).unwrap();
        assert_eq!(output.chunks.concat(), "done");
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);

        let observations = sink.observations.lock().unwrap();
        assert_eq!(observations.len(), 2);
        assert_eq!(observations[0].phase, "initial");
        let observation = &observations[1];
        assert_eq!(observation.phase, "completed");
        assert_eq!(observation.agent_id, "agent");
        assert_eq!(observation.operation_id, "op");
        assert_eq!(observation.tool_call_id, "call-send-9");
        assert_eq!(observation.succeeded, Some(true));
        assert!(observation.completed_at_ms.unwrap() >= observation.started_at_ms);
        assert_eq!(&observation.protobuf_bytes[..2], &[0xba, 0x03]);

        let decoded = decode_agent_tool_call(&observation.protobuf_bytes).unwrap();
        assert_eq!(decoded.tool_call_id.as_deref(), Some("call-send-9"));
        assert_eq!(decoded.started_at_ms, Some(observation.started_at_ms));
        assert_eq!(decoded.completed_at_ms, observation.completed_at_ms);
        match decoded.tool.unwrap() {
            agent_tool_call::Tool::SendMessageToolCall(call) => {
                assert!(matches!(
                    call.args.unwrap().message.unwrap(),
                    send_message_args::Message::Text(text) if text.content == "hello"
                ));
                assert!(matches!(
                    call.result.unwrap().result.unwrap(),
                    send_message_result::Result::Success(result)
                        if result.message_id == "msg-9"
                            && Some(result.timestamp) == observation.completed_at_ms
                ));
            }
            _ => panic!("expected canonical field 55 SendMessageToolCall"),
        }
    }

    #[test]
    fn production_secret_request_dispatch_preserves_field_55_without_secret_material() {
        use super::super::agent_v1_wire::{
            agent_tool_call, decode_agent_tool_call, send_message_args,
        };

        let endpoint = spawn_sse_server(vec![
            tool_sse(
                "SendMessage",
                "call-secret-1",
                r#"{"type":"secret-request","secret":{"label":" Discord   bot token ","description":"Paste it securely","connector":"discord","field":"token"}}"#,
            ),
            final_sse(),
        ]);
        let calls = Arc::new(AtomicUsize::new(0));
        let sink = Arc::new(RecordingObservationSink::default());
        let mut provider = AndroidHostInferenceProvider::with_endpoint(
            "a".repeat(32),
            endpoint,
            Arc::new(AtomicBool::new(false)),
        )
        .with_routed_tools(Arc::new(CanonicalProductionTools {
            result: Ok(json!({"sent":true,"messageId":"secret-card-1"})),
            calls: Arc::clone(&calls),
        }))
        .with_tool_call_observation_sink(sink.clone());

        let output = provider.start_stream(&input(), 1).unwrap();
        assert_eq!(output.chunks.concat(), "done");
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        let observations = sink.observations.lock().unwrap();
        assert_eq!(observations.len(), 2);
        assert_eq!(observations[0].phase, "initial");
        assert_eq!(observations[1].phase, "completed");
        assert_eq!(&observations[0].protobuf_bytes[..2], &[0xba, 0x03]);
        assert_eq!(&observations[1].protobuf_bytes[..2], &[0xba, 0x03]);
        for observation in observations.iter() {
            let decoded = decode_agent_tool_call(&observation.protobuf_bytes).unwrap();
            assert_eq!(decoded.tool_call_id.as_deref(), Some("call-secret-1"));
            match decoded.tool.unwrap() {
                agent_tool_call::Tool::SendMessageToolCall(call) => {
                    match call.args.unwrap().message.unwrap() {
                        send_message_args::Message::Text(text) => {
                            assert_eq!(
                                text.content,
                                "Requested a secret from the user securely: Discord bot token"
                            );
                            assert!(!text.content.contains("Paste it securely"));
                        }
                        _ => panic!("secret request must project to canonical SendMessageText"),
                    }
                }
                _ => panic!("secret request must remain field 55 SendMessageToolCall"),
            }
        }
    }

    #[test]
    fn production_frozen_communicate_bridge_failure_records_error_and_suppresses_retry() {
        use super::super::agent_v1_wire::{
            agent_tool_call, communicate_update_result, decode_agent_tool_call,
        };

        let endpoint = spawn_sse_server(vec![tool_sse(
            "UpdateAgent",
            "call-update-3",
            r#"{"id":"agent-3","description":"working"}"#,
        )]);
        let calls = Arc::new(AtomicUsize::new(0));
        let sink = Arc::new(RecordingObservationSink::default());
        let mut provider = AndroidHostInferenceProvider::with_endpoint(
            "a".repeat(32),
            endpoint,
            Arc::new(AtomicBool::new(false)),
        )
        .with_routed_tools(Arc::new(CanonicalProductionTools {
            result: Err("outcome unknown".into()),
            calls: Arc::clone(&calls),
        }))
        .with_tool_call_observation_sink(sink.clone());

        let error = provider.start_stream(&input(), 1).unwrap_err();
        assert!(error.message.contains("outcome unknown"));
        assert!(error.stream_output_produced);
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);

        let observations = sink.observations.lock().unwrap();
        assert_eq!(observations.len(), 2);
        assert_eq!(observations[0].phase, "initial");
        let observation = &observations[1];
        assert_eq!(observation.phase, "completed");
        assert_eq!(observation.succeeded, Some(false));
        assert_eq!(observation.tool_name, "UpdateAgent");
        assert_eq!(&observation.protobuf_bytes[..2], &[0x82, 0x03]);
        let decoded = decode_agent_tool_call(&observation.protobuf_bytes).unwrap();
        assert_eq!(decoded.tool_call_id.as_deref(), Some("call-update-3"));
        assert_eq!(decoded.started_at_ms, Some(observation.started_at_ms));
        assert_eq!(decoded.completed_at_ms, observation.completed_at_ms);
        match decoded.tool.unwrap() {
            agent_tool_call::Tool::CommunicateUpdateToolCall(call) => {
                assert!(matches!(
                    call.result.unwrap().result.unwrap(),
                    communicate_update_result::Result::Error(result)
                        if result.error == "outcome unknown"
                ));
            }
            _ => panic!("expected canonical field 48 CommunicateUpdateToolCall"),
        }
    }

    #[test]
    fn production_frozen_communicate_success_uses_real_dispatch_result() {
        use super::super::agent_v1_wire::{
            agent_tool_call, communicate_update_result, decode_agent_tool_call,
        };

        let endpoint = spawn_sse_server(vec![
            tool_sse(
                "CreateAgent",
                "call-create-2",
                r#"{"name":"Researcher","description":"Investigate"}"#,
            ),
            final_sse(),
        ]);
        let calls = Arc::new(AtomicUsize::new(0));
        let sink = Arc::new(RecordingObservationSink::default());
        let mut provider = AndroidHostInferenceProvider::with_endpoint(
            "a".repeat(32),
            endpoint,
            Arc::new(AtomicBool::new(false)),
        )
        .with_routed_tools(Arc::new(CanonicalProductionTools {
            result: Ok(Value::String("Created agent Researcher (id: agent-2)".into())),
            calls: Arc::clone(&calls),
        }))
        .with_tool_call_observation_sink(sink.clone());

        let output = provider.start_stream(&input(), 1).unwrap();
        assert_eq!(output.chunks.concat(), "done");
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);

        let observations = sink.observations.lock().unwrap();
        assert_eq!(observations.len(), 2);
        assert_eq!(observations[0].phase, "initial");
        let observation = &observations[1];
        assert_eq!(observation.phase, "completed");
        assert_eq!(observation.tool_name, "CreateAgent");
        let decoded = decode_agent_tool_call(&observation.protobuf_bytes).unwrap();
        assert_eq!(decoded.tool_call_id.as_deref(), Some("call-create-2"));
        assert_eq!(decoded.started_at_ms, Some(observation.started_at_ms));
        assert_eq!(decoded.completed_at_ms, observation.completed_at_ms);
        match decoded.tool.unwrap() {
            agent_tool_call::Tool::CommunicateUpdateToolCall(call) => {
                let args = call.args.unwrap();
                assert!(args
                    .current_step
                    .as_deref()
                    .is_some_and(|value| value.contains("\"result\":\"Created agent Researcher")));
                assert!(matches!(
                    call.result.unwrap().result.unwrap(),
                    communicate_update_result::Result::Success(result)
                        if result.current_step == "Created agent Researcher (id: agent-2)"
                            && result.message_index == 0
                ));
            }
            _ => panic!("expected canonical field 48 CommunicateUpdateToolCall"),
        }
    }

    #[test]
    fn production_canonical_dispatch_fails_closed_for_malformed_args_and_results() {
        let widget_endpoint = spawn_sse_server(vec![
            tool_sse(
                "SendMessage",
                "widget-call",
                r#"{"type":"widget","widget":{"prompt":"Choose","options":[{"label":"Alpha"},{"label":"Beta"}]}}"#,
            ),
            final_sse(),
        ]);
        let widget_calls = Arc::new(AtomicUsize::new(0));
        let widget_sink = Arc::new(RecordingObservationSink::default());
        let mut widget_provider = AndroidHostInferenceProvider::with_endpoint(
            "a".repeat(32),
            widget_endpoint,
            Arc::new(AtomicBool::new(false)),
        )
        .with_routed_tools(Arc::new(CanonicalProductionTools {
            result: Ok(json!({"sent":true,"messageId":"widget-message"})),
            calls: Arc::clone(&widget_calls),
        }))
        .with_tool_call_observation_sink(widget_sink.clone());
        widget_provider.start_stream(&input(), 1).unwrap();
        assert_eq!(widget_calls.load(AtomicOrdering::SeqCst), 1);
        let widget_observations = widget_sink.observations.lock().unwrap();
        assert_eq!(widget_observations.len(), 2);
        assert_eq!(widget_observations[0].phase, "initial");
        assert_eq!(widget_observations[1].phase, "completed");
        let widget_decoded = super::super::agent_v1_wire::decode_agent_tool_call(
            &widget_observations[1].protobuf_bytes,
        )
        .unwrap();
        match widget_decoded.tool.unwrap() {
            super::super::agent_v1_wire::agent_tool_call::Tool::SendMessageToolCall(call) => {
                assert!(matches!(
                    call.args.unwrap().message.unwrap(),
                    super::super::agent_v1_wire::send_message_args::Message::Text(text)
                        if text.content == "Choose — Alpha / Beta"
                ));
            }
            _ => panic!("widget must remain canonical field 55"),
        }
        drop(widget_observations);

        let malformed_args_endpoint = spawn_sse_server(vec![tool_sse(
            "SendMessage",
            "bad-args",
            r#"{"type":"unknown","content":"nope"}"#,
        )]);
        let malformed_args_calls = Arc::new(AtomicUsize::new(0));
        let sink = Arc::new(RecordingObservationSink::default());
        let mut provider = AndroidHostInferenceProvider::with_endpoint(
            "a".repeat(32),
            malformed_args_endpoint,
            Arc::new(AtomicBool::new(false)),
        )
        .with_routed_tools(Arc::new(CanonicalProductionTools {
            result: Ok(json!({"messageId":"should-not-dispatch"})),
            calls: Arc::clone(&malformed_args_calls),
        }))
        .with_tool_call_observation_sink(sink.clone());
        let error = provider.start_stream(&input(), 1).unwrap_err();
        assert!(error.message.contains("unsupported SendMessage canonical message type"));
        assert_eq!(malformed_args_calls.load(AtomicOrdering::SeqCst), 0);
        assert!(sink.observations.lock().unwrap().is_empty());

        let malformed_result_endpoint = spawn_sse_server(vec![tool_sse(
            "SendMessage",
            "bad-result",
            r#"{"message":{"type":"text","content":"hello"}}"#,
        )]);
        let malformed_result_calls = Arc::new(AtomicUsize::new(0));
        let sink = Arc::new(RecordingObservationSink::default());
        let mut provider = AndroidHostInferenceProvider::with_endpoint(
            "a".repeat(32),
            malformed_result_endpoint,
            Arc::new(AtomicBool::new(false)),
        )
        .with_routed_tools(Arc::new(CanonicalProductionTools {
            result: Ok(json!({"sent":true})),
            calls: Arc::clone(&malformed_result_calls),
        }))
        .with_tool_call_observation_sink(sink.clone());
        let error = provider.start_stream(&input(), 1).unwrap_err();
        assert!(error.message.contains("message_id"));
        assert!(error.stream_output_produced);
        assert_eq!(malformed_result_calls.load(AtomicOrdering::SeqCst), 1);
        let malformed_result_observations = sink.observations.lock().unwrap();
        assert_eq!(malformed_result_observations.len(), 1);
        assert_eq!(malformed_result_observations[0].phase, "initial");
        assert_eq!(malformed_result_observations[0].completed_at_ms, None);
    }

    #[test]
    fn production_canonical_dispatch_requires_observation_owner_before_side_effect() {
        let endpoint = spawn_sse_server(vec![tool_sse(
            "SendMessage",
            "missing-owner",
            r#"{"message":{"type":"text","content":"hello"}}"#,
        )]);
        let calls = Arc::new(AtomicUsize::new(0));
        let mut provider = AndroidHostInferenceProvider::with_endpoint(
            "a".repeat(32),
            endpoint,
            Arc::new(AtomicBool::new(false)),
        )
        .with_routed_tools(Arc::new(CanonicalProductionTools {
            result: Ok(json!({"sent":true,"messageId":"msg"})),
            calls: Arc::clone(&calls),
        }));
        let error = provider.start_stream(&input(), 1).unwrap_err();
        assert!(error.message.contains("observation sink is not bound"));
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn routed_tool_loop_uses_canonical_turn_step_budget() {
        assert_eq!(SAND_AGENT_MAX_STEPS, 5_000);
        let source = include_str!("android_host_inference.rs");
        let production = source
            .split_once("\n#[cfg(test)]\nmod tests {")
            .map(|(production, _)| production)
            .unwrap_or(source);
        assert!(production.contains("for _tool_step in 0..SAND_AGENT_MAX_STEPS {"));
        assert!(!production.contains("for step in 0..8 {"));
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

    struct ReminderTools {
        calls: Arc<AtomicUsize>,
        cancelled: Option<Arc<AtomicBool>>,
    }

    impl AndroidRoutedToolBridge for ReminderTools {
        fn list_tools(&self) -> Result<Vec<Value>, String> {
            Ok(["Read", "Shell", "SendMessage"]
                .into_iter()
                .map(|name| json!({
                    "type":"function",
                    "name":name,
                    "parameters":{"type":"object"}
                }))
                .collect())
        }

        fn call_tool(
            &self,
            name: &str,
            _args: Value,
            _tool_call_id: &str,
        ) -> Result<Value, String> {
            self.calls.fetch_add(1, AtomicOrdering::SeqCst);
            if let Some(cancelled) = self.cancelled.as_ref() {
                cancelled.store(true, Ordering::Release);
            }
            if name == "SendMessage" {
                Ok(json!({"sent":true,"messageId":"reminder-test-message"}))
            } else {
                Ok(json!({"ok":true}))
            }
        }
    }

    fn request_contains_user_reminder(request: &Value, reminder: &str) -> bool {
        request
            .get("input")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items.iter().any(|item| {
                    item.get("role").and_then(Value::as_str) == Some("user")
                        && item.get("content").and_then(Value::as_str) == Some(reminder)
                })
            })
    }

    #[test]
    fn production_loop_injects_start_ack_into_third_provider_request() {
        use super::super::turn_tool_session_reminders::START_OF_TURN_ACK_REMINDER_MESSAGE;

        let (endpoint, requests) = spawn_recording_sse_server(vec![
            tool_sse("Read", "read-1", "{}"),
            tool_sse("Shell", "shell-2", "{}"),
            final_sse(),
        ]);
        let calls = Arc::new(AtomicUsize::new(0));
        let mut provider = AndroidHostInferenceProvider::with_endpoint(
            "a".repeat(32),
            endpoint,
            Arc::new(AtomicBool::new(false)),
        )
        .with_turn_tool_session_reminders(true)
        .with_routed_tools(Arc::new(ReminderTools {
            calls: Arc::clone(&calls),
            cancelled: None,
        }));

        let output = provider.start_stream(&input(), 1).unwrap();
        assert_eq!(output.chunks.concat(), "done");
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 2);
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(!request_contains_user_reminder(
            &requests[1],
            START_OF_TURN_ACK_REMINDER_MESSAGE
        ));
        assert!(request_contains_user_reminder(
            &requests[2],
            START_OF_TURN_ACK_REMINDER_MESSAGE
        ));
    }

    #[test]
    fn production_loop_text_send_message_clears_start_ack_condition() {
        use super::super::turn_tool_session_reminders::{
            EARLY_RESULT_REMINDER_MESSAGE, START_OF_TURN_ACK_REMINDER_MESSAGE,
        };

        let (endpoint, requests) = spawn_recording_sse_server(vec![
            tool_sse("Read", "read-1", "{}"),
            tool_sse(
                "SendMessage",
                "send-2",
                r#"{"type":"text","content":"Checking now."}"#,
            ),
            tool_sse("Read", "read-3", "{}"),
            final_sse(),
        ]);
        let calls = Arc::new(AtomicUsize::new(0));
        let sink = Arc::new(RecordingObservationSink::default());
        let mut provider = AndroidHostInferenceProvider::with_endpoint(
            "a".repeat(32),
            endpoint,
            Arc::new(AtomicBool::new(false)),
        )
        .with_turn_tool_session_reminders(true)
        .with_routed_tools(Arc::new(ReminderTools {
            calls: Arc::clone(&calls),
            cancelled: None,
        }))
        .with_tool_call_observation_sink(sink);

        provider.start_stream(&input(), 1).unwrap();
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 3);
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 4);
        assert!(!requests.iter().any(|request| {
            request_contains_user_reminder(request, START_OF_TURN_ACK_REMINDER_MESSAGE)
        }));
        assert!(request_contains_user_reminder(
            &requests[3],
            EARLY_RESULT_REMINDER_MESSAGE
        ));
    }

    #[test]
    fn production_loop_generated_subagent_configuration_does_not_inject_reminders() {
        use super::super::turn_tool_session_reminders::START_OF_TURN_ACK_REMINDER_MESSAGE;

        let (endpoint, requests) = spawn_recording_sse_server(vec![
            tool_sse("Read", "read-1", "{}"),
            tool_sse("Shell", "shell-2", "{}"),
            final_sse(),
        ]);
        let mut provider = AndroidHostInferenceProvider::with_endpoint(
            "a".repeat(32),
            endpoint,
            Arc::new(AtomicBool::new(false)),
        )
        .with_turn_tool_session_reminders(false)
        .with_routed_tools(Arc::new(ReminderTools {
            calls: Arc::new(AtomicUsize::new(0)),
            cancelled: None,
        }));

        provider.start_stream(&input(), 1).unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(!requests.iter().any(|request| {
            request_contains_user_reminder(request, START_OF_TURN_ACK_REMINDER_MESSAGE)
        }));
    }

    #[test]
    fn cancellation_after_tool_dispatch_does_not_repeat_side_effect_or_send_reminder_request() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let (endpoint, requests) =
            spawn_recording_sse_server(vec![tool_sse("Read", "read-1", "{}")]);
        let calls = Arc::new(AtomicUsize::new(0));
        let mut provider = AndroidHostInferenceProvider::with_endpoint(
            "a".repeat(32),
            endpoint,
            Arc::clone(&cancelled),
        )
        .with_turn_tool_session_reminders(true)
        .with_routed_tools(Arc::new(ReminderTools {
            calls: Arc::clone(&calls),
            cancelled: Some(cancelled),
        }));

        let error = provider.start_stream(&input(), 1).unwrap_err();
        assert_eq!(error.message, "cancelled");
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(requests.lock().unwrap().len(), 1);
    }


}
