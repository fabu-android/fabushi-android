use super::transient_stream_error::{
    compute_backoff_delay_ms, compute_server_paced_delay_ms, should_retry_turn_attempt,
    RetryPolicy, DEFAULT_FIRST_TOKEN_STALL_DEADLINE_MS,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamAttemptInput {
    pub operation_id: String,
    pub agent_id: String,
    pub model: String,
    pub prompt: String,
    pub resume_checkpoint_available: bool,
}

impl StreamAttemptInput {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.operation_id.trim().is_empty() {
            return Err("operation_id is required");
        }
        if self.agent_id.trim().is_empty() {
            return Err("agent_id is required");
        }
        if self.model.trim().is_empty() {
            return Err("model is required");
        }
        if self.prompt.trim().is_empty() {
            return Err("prompt is required");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamGeneration {
    pub first_token_delay_ms: u64,
    pub chunks: Vec<String>,
    pub finish_reason: String,
}

impl StreamGeneration {
    pub fn output_produced(&self) -> bool {
        self.chunks.iter().any(|chunk| !chunk.is_empty())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderFailure {
    pub message: String,
    pub retry_after_ms: Option<u64>,
    pub first_token_stall: bool,
    pub stream_output_produced: bool,
}

impl ProviderFailure {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retry_after_ms: None,
            first_token_stall: false,
            stream_output_produced: false,
        }
    }
}

pub trait TurnStreamProvider {
    fn start_stream(
        &mut self,
        input: &StreamAttemptInput,
        attempt: usize,
    ) -> Result<StreamGeneration, ProviderFailure>;

    fn start_stream_with_sink(
        &mut self,
        input: &StreamAttemptInput,
        attempt: usize,
        on_chunk: &mut dyn FnMut(&str) -> Result<(), String>,
    ) -> Result<StreamGeneration, ProviderFailure> {
        let generation = self.start_stream(input, attempt)?;
        let mut emitted = false;
        for chunk in &generation.chunks {
            on_chunk(chunk).map_err(|message| ProviderFailure {
                message,
                retry_after_ms: None,
                first_token_stall: false,
                stream_output_produced: emitted,
            })?;
            emitted = emitted || !chunk.is_empty();
        }
        Ok(generation)
    }

    fn cancel(&mut self, operation_id: &str) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetryObservation {
    pub attempt: usize,
    pub delay_ms: u64,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamAttemptResult {
    pub chunks: Vec<String>,
    pub finish_reason: String,
    pub attempts: usize,
    pub retries: Vec<RetryObservation>,
}

pub struct StreamAttemptHost<P: TurnStreamProvider> {
    provider: P,
    retry_policy: RetryPolicy,
    first_token_deadline_ms: u64,
    jitter_numerator: u32,
}

impl<P: TurnStreamProvider> StreamAttemptHost<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            retry_policy: RetryPolicy::automation_default(),
            first_token_deadline_ms: DEFAULT_FIRST_TOKEN_STALL_DEADLINE_MS,
            jitter_numerator: 5_000,
        }
    }

    pub fn with_policy(
        provider: P,
        retry_policy: RetryPolicy,
        first_token_deadline_ms: u64,
    ) -> Self {
        Self {
            provider,
            retry_policy,
            first_token_deadline_ms: first_token_deadline_ms.max(1),
            jitter_numerator: 5_000,
        }
    }

    pub fn run(&mut self, input: &StreamAttemptInput) -> Result<StreamAttemptResult, ProviderFailure> {
        self.run_with_observers(input, &mut |_| {}, &mut |_| Ok(()))
    }

    pub fn run_with_observers(
        &mut self,
        input: &StreamAttemptInput,
        on_retry: &mut dyn FnMut(&RetryObservation),
        on_chunk: &mut dyn FnMut(&str) -> Result<(), String>,
    ) -> Result<StreamAttemptResult, ProviderFailure> {
        input
            .validate()
            .map_err(|message| ProviderFailure::new(message))?;

        let mut retries = Vec::new();
        for attempt in 1..=self.retry_policy.max_attempts.max(1) {
            match self
                .provider
                .start_stream_with_sink(input, attempt, on_chunk)
            {
                Ok(generation) => {
                    if generation.first_token_delay_ms > self.first_token_deadline_ms
                        && !generation.output_produced()
                    {
                        let failure = ProviderFailure {
                            message: format!(
                                "first-token stall after {} ms",
                                generation.first_token_delay_ms
                            ),
                            retry_after_ms: None,
                            first_token_stall: true,
                            stream_output_produced: false,
                        };
                        if self.should_retry(input, attempt, &failure) {
                            let observation = self.retry_observation(attempt, &failure);
                            on_retry(&observation);
                            retries.push(observation);
                            continue;
                        }
                        return Err(failure);
                    }
                    return Ok(StreamAttemptResult {
                        chunks: generation.chunks,
                        finish_reason: generation.finish_reason,
                        attempts: attempt,
                        retries,
                    });
                }
                Err(failure) => {
                    if self.should_retry(input, attempt, &failure) {
                        let observation = self.retry_observation(attempt, &failure);
                        on_retry(&observation);
                        retries.push(observation);
                        continue;
                    }
                    return Err(failure);
                }
            }
        }

        Err(ProviderFailure::new("stream attempts exhausted"))
    }

    pub fn cancel(&mut self, operation_id: &str) -> Result<(), String> {
        if operation_id.trim().is_empty() {
            return Err("operation_id is required".into());
        }
        self.provider.cancel(operation_id)
    }

    pub fn into_provider(self) -> P {
        self.provider
    }

    fn should_retry(
        &self,
        input: &StreamAttemptInput,
        attempt: usize,
        failure: &ProviderFailure,
    ) -> bool {
        should_retry_turn_attempt(
            false,
            &failure.message,
            failure.stream_output_produced,
            input.resume_checkpoint_available,
            attempt,
            self.retry_policy,
            failure.first_token_stall,
        )
    }

    fn retry_observation(
        &self,
        attempt: usize,
        failure: &ProviderFailure,
    ) -> RetryObservation {
        let delay_ms = failure
            .retry_after_ms
            .map(|retry_after| {
                compute_server_paced_delay_ms(retry_after, self.jitter_numerator)
            })
            .unwrap_or_else(|| {
                compute_backoff_delay_ms(
                    attempt,
                    self.retry_policy.base_delay_ms,
                    self.retry_policy.max_delay_ms,
                    self.jitter_numerator,
                )
            });
        RetryObservation {
            attempt,
            delay_ms,
            reason: failure.message.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct ScriptedProvider {
        responses: VecDeque<Result<StreamGeneration, ProviderFailure>>,
        cancelled: Vec<String>,
    }

    impl TurnStreamProvider for ScriptedProvider {
        fn start_stream(
            &mut self,
            _input: &StreamAttemptInput,
            _attempt: usize,
        ) -> Result<StreamGeneration, ProviderFailure> {
            self.responses
                .pop_front()
                .unwrap_or_else(|| Err(ProviderFailure::new("no scripted response")))
        }

        fn cancel(&mut self, operation_id: &str) -> Result<(), String> {
            self.cancelled.push(operation_id.to_string());
            Ok(())
        }
    }

    fn input() -> StreamAttemptInput {
        StreamAttemptInput {
            operation_id: "op-1".into(),
            agent_id: "agent-1".into(),
            model: "default".into(),
            prompt: "hello".into(),
            resume_checkpoint_available: false,
        }
    }

    #[test]
    fn transient_failure_retries_then_streams_chunks() {
        let provider = ScriptedProvider {
            responses: VecDeque::from([
                Err(ProviderFailure::new("ECONNRESET")),
                Ok(StreamGeneration {
                    first_token_delay_ms: 10,
                    chunks: vec!["hel".into(), "lo".into()],
                    finish_reason: "stop".into(),
                }),
            ]),
            cancelled: vec![],
        };
        let mut host = StreamAttemptHost::new(provider);
        let result = host.run(&input()).unwrap();
        assert_eq!(result.attempts, 2);
        assert_eq!(result.retries.len(), 1);
        assert_eq!(result.chunks.concat(), "hello");
    }

    #[test]
    fn first_token_watchdog_retries_before_output() {
        let provider = ScriptedProvider {
            responses: VecDeque::from([
                Ok(StreamGeneration {
                    first_token_delay_ms: 500,
                    chunks: vec![],
                    finish_reason: "stop".into(),
                }),
                Ok(StreamGeneration {
                    first_token_delay_ms: 5,
                    chunks: vec!["ok".into()],
                    finish_reason: "stop".into(),
                }),
            ]),
            cancelled: vec![],
        };
        let mut host = StreamAttemptHost::with_policy(
            provider,
            RetryPolicy {
                max_attempts: 2,
                base_delay_ms: 1,
                max_delay_ms: 2,
            },
            100,
        );
        let result = host.run(&input()).unwrap();
        assert_eq!(result.attempts, 2);
        assert_eq!(result.retries.len(), 1);
        assert_eq!(result.chunks, vec!["ok"]);
    }

    #[test]
    fn output_failure_without_checkpoint_is_not_retried() {
        let provider = ScriptedProvider {
            responses: VecDeque::from([Err(ProviderFailure {
                message: "connection reset".into(),
                retry_after_ms: None,
                first_token_stall: false,
                stream_output_produced: true,
            })]),
            cancelled: vec![],
        };
        let mut host = StreamAttemptHost::new(provider);
        assert!(host.run(&input()).is_err());
    }
}
