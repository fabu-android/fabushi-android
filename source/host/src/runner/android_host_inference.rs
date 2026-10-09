use super::stream_attempt::{
    ProviderFailure, StreamAttemptInput, StreamGeneration, TurnStreamProvider,
};
use serde_json::{json, Value};
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

#[derive(Clone)]
pub struct AndroidHostInferenceProvider {
    mode: AndroidInferenceMode,
    bearer_token: Option<String>,
    base_url: String,
    default_model: String,
    cancelled: Arc<AtomicBool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ProviderSseEvent {
    Delta(String),
    Completed,
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
        })
    }

    pub fn run_summarization_prompt(
        mode: AndroidInferenceMode,
        bearer_token: Option<String>,
        cancelled: Arc<AtomicBool>,
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
        let prompt = format!(
            "{system_prompt}\n\n{user_prompt}"
        );
        let input = StreamAttemptInput {
            operation_id: "summarization".into(),
            agent_id: "mahayana-summarizer".into(),
            model: provider.default_model.clone(),
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
        }
    }

    fn run_stream(
        &mut self,
        input: &StreamAttemptInput,
        on_chunk: &mut dyn FnMut(&str) -> Result<(), String>,
    ) -> Result<StreamGeneration, ProviderFailure> {
        if input.prompt.trim().is_empty() {
            return Err(ProviderFailure::new("prompt is required"));
        }
        self.cancelled.store(false, Ordering::Release);

        if self.mode == AndroidInferenceMode::Test {
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
        let model = if input.model.trim().is_empty() || input.model == "default" {
            self.default_model.as_str()
        } else {
            input.model.as_str()
        };
        let endpoint = format!("{}/responses", self.base_url.trim_end_matches('/'));
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(20))
            .timeout_read(Duration::from_secs(180))
            .timeout_write(Duration::from_secs(30))
            .build();
        let started = Instant::now();
        let response = agent
            .post(&endpoint)
            .set("Accept", "text/event-stream")
            .set("Authorization", &format!("Bearer {token}"))
            .send_json(json!({
                "model": model,
                "input": input.prompt,
                "stream": true,
            }));

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
                    stream_output_produced: false,
                });
            }
            Err(ureq::Error::Transport(error)) => {
                return Err(ProviderFailure {
                    message: format!("network error: {}", safe_transport_kind(&error)),
                    retry_after_ms: None,
                    first_token_stall: false,
                    stream_output_produced: false,
                });
            }
        };

        let mut reader = BufReader::new(response.into_reader());
        let mut line = String::new();
        let mut chunks = Vec::new();
        let mut first_token_delay_ms = None;
        let mut completed = false;

        loop {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(ProviderFailure {
                    message: "cancelled".into(),
                    retry_after_ms: None,
                    first_token_stall: false,
                    stream_output_produced: !chunks.is_empty(),
                });
            }
            line.clear();
            let bytes = reader.read_line(&mut line).map_err(|_| ProviderFailure {
                message: "network error: stream read failed".into(),
                retry_after_ms: None,
                first_token_stall: false,
                stream_output_produced: !chunks.is_empty(),
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
                        first_token_delay_ms =
                            Some(started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
                    }
                    on_chunk(&delta).map_err(|message| ProviderFailure {
                        message,
                        retry_after_ms: None,
                        first_token_stall: false,
                        stream_output_produced: !chunks.is_empty(),
                    })?;
                    chunks.push(delta);
                }
                Ok(ProviderSseEvent::Completed) => {
                    completed = true;
                    break;
                }
                Ok(ProviderSseEvent::Failed(message)) => {
                    return Err(ProviderFailure {
                        message,
                        retry_after_ms: None,
                        first_token_stall: false,
                        stream_output_produced: !chunks.is_empty(),
                    });
                }
                Ok(ProviderSseEvent::Ignore) => {}
                Err(message) => {
                    return Err(ProviderFailure {
                        message,
                        retry_after_ms: None,
                        first_token_stall: false,
                        stream_output_produced: !chunks.is_empty(),
                    });
                }
            }
        }

        if !completed && chunks.is_empty() {
            return Err(ProviderFailure::new(
                "network error: provider stream closed before output",
            ));
        }
        Ok(StreamGeneration {
            first_token_delay_ms: first_token_delay_ms
                .unwrap_or_else(|| started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64),
            chunks,
            finish_reason: if completed { "stop" } else { "eof" }.into(),
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
        return Ok(ProviderSseEvent::Completed);
    }
    let value: Value =
        serde_json::from_str(data).map_err(|_| "provider stream returned invalid JSON".to_string())?;
    let event_type = value.get("type").and_then(Value::as_str).unwrap_or_default();
    match event_type {
        "response.output_text.delta" => Ok(ProviderSseEvent::Delta(
            value
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        )),
        "response.completed" => Ok(ProviderSseEvent::Completed),
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
            ProviderSseEvent::Completed
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
