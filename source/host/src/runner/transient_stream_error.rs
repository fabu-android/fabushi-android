pub const TRANSIENT_ERRNO_CODES: &[&str] = &[
    "ECONNRESET",
    "ETIMEDOUT",
    "EPIPE",
    "ECONNABORTED",
    "ECONNREFUSED",
    "ENETRESET",
    "ENETDOWN",
    "ENETUNREACH",
    "EHOSTUNREACH",
    "EAI_AGAIN",
];

pub const TRANSIENT_MESSAGE_TOKENS: &[&str] = &[
    "socket hang up",
    "premature close",
    "stream closed",
    "closed stream",
    "connection reset",
    "connection closed",
    "connection terminated",
    "network error",
    "the operation was aborted",
];

pub const MAX_SERVER_RETRY_AFTER_MS: u64 = 30_000;
pub const DEFAULT_AUTOMATION_STREAM_RETRY_MAX_ATTEMPTS: usize = 4;
pub const DEFAULT_AUTOMATION_STREAM_RETRY_BASE_DELAY_MS: u64 = 1_000;
pub const DEFAULT_AUTOMATION_STREAM_RETRY_MAX_DELAY_MS: u64 = 15_000;
pub const DEFAULT_OVERLOAD_STREAM_RETRY_MAX_ATTEMPTS: usize = 3;
pub const DEFAULT_OVERLOAD_STREAM_RETRY_BASE_DELAY_MS: u64 = 750;
pub const DEFAULT_OVERLOAD_STREAM_RETRY_MAX_DELAY_MS: u64 = 6_000;
pub const DEFAULT_FIRST_TOKEN_STALL_DEADLINE_MS: u64 = 150_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: usize,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl RetryPolicy {
    pub const fn automation_default() -> Self {
        Self {
            max_attempts: DEFAULT_AUTOMATION_STREAM_RETRY_MAX_ATTEMPTS,
            base_delay_ms: DEFAULT_AUTOMATION_STREAM_RETRY_BASE_DELAY_MS,
            max_delay_ms: DEFAULT_AUTOMATION_STREAM_RETRY_MAX_DELAY_MS,
        }
    }

    pub const fn overload_default() -> Self {
        Self {
            max_attempts: DEFAULT_OVERLOAD_STREAM_RETRY_MAX_ATTEMPTS,
            base_delay_ms: DEFAULT_OVERLOAD_STREAM_RETRY_BASE_DELAY_MS,
            max_delay_ms: DEFAULT_OVERLOAD_STREAM_RETRY_MAX_DELAY_MS,
        }
    }
}

pub fn message_looks_transient(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    TRANSIENT_ERRNO_CODES
        .iter()
        .any(|code| lower.contains(&code.to_ascii_lowercase()))
        || TRANSIENT_MESSAGE_TOKENS
            .iter()
            .any(|token| lower.contains(token))
}

pub fn is_provider_capacity_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("resourceexhausted")
        || lower.contains("resource_exhausted")
        || lower.contains("provider overloaded")
        || lower.contains("capacity")
        || lower.contains("rate limit")
}

pub fn is_context_overflow_dead_end(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("inputtokenlimiterror")
        || lower.contains("context window")
        || lower.contains("context length")
        || lower.contains("conversation too large")
}

pub fn should_retry_turn_attempt(
    canceled: bool,
    message: &str,
    stream_output_produced: bool,
    resume_checkpoint_available: bool,
    attempt: usize,
    policy: RetryPolicy,
    first_token_stall: bool,
) -> bool {
    if canceled || attempt >= policy.max_attempts {
        return false;
    }
    if stream_output_produced && !resume_checkpoint_available {
        return false;
    }
    if is_context_overflow_dead_end(message) {
        return false;
    }
    first_token_stall || message_looks_transient(message) || is_provider_capacity_error(message)
}

pub fn compute_backoff_delay_ms(
    attempt: usize,
    base_delay_ms: u64,
    max_delay_ms: u64,
    jitter_numerator: u32,
) -> u64 {
    let base = base_delay_ms;
    let cap = max_delay_ms.max(base);
    let exponent = attempt.saturating_sub(1).min(31);
    let exponential = base
        .saturating_mul(1_u64 << exponent)
        .min(cap);
    let jitter = u64::from(jitter_numerator.min(10_000));
    let half = exponential / 2;
    half.saturating_add(half.saturating_mul(jitter) / 10_000)
        .min(cap)
}

pub fn compute_server_paced_delay_ms(retry_after_ms: u64, jitter_numerator: u32) -> u64 {
    let retry_after_ms = retry_after_ms.min(MAX_SERVER_RETRY_AFTER_MS);
    let jitter = u64::from(jitter_numerator.min(10_000));
    retry_after_ms
        .saturating_add(retry_after_ms.saturating_mul(jitter) / 20_000)
        .min(MAX_SERVER_RETRY_AFTER_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_and_capacity_errors_are_retryable_before_stream_output() {
        let policy = RetryPolicy::automation_default();
        assert!(should_retry_turn_attempt(
            false,
            "ECONNRESET from provider",
            false,
            false,
            1,
            policy,
            false,
        ));
        assert!(should_retry_turn_attempt(
            false,
            "ResourceExhausted",
            false,
            false,
            1,
            policy,
            false,
        ));
        assert!(!should_retry_turn_attempt(
            false,
            "context window exceeded",
            false,
            false,
            1,
            policy,
            false,
        ));
    }

    #[test]
    fn produced_output_requires_resume_checkpoint_for_retry() {
        let policy = RetryPolicy::automation_default();
        assert!(!should_retry_turn_attempt(
            false,
            "connection reset",
            true,
            false,
            1,
            policy,
            false,
        ));
        assert!(should_retry_turn_attempt(
            false,
            "connection reset",
            true,
            true,
            1,
            policy,
            false,
        ));
    }

    #[test]
    fn backoff_is_bounded_and_server_delay_is_capped() {
        assert_eq!(compute_backoff_delay_ms(1, 1_000, 15_000, 0), 500);
        assert_eq!(compute_backoff_delay_ms(1, 1_000, 15_000, 10_000), 1_000);
        assert!(compute_backoff_delay_ms(10, 1_000, 15_000, 5_000) <= 15_000);
        assert_eq!(
            compute_server_paced_delay_ms(60_000, 10_000),
            MAX_SERVER_RETRY_AFTER_MS
        );
    }
}
