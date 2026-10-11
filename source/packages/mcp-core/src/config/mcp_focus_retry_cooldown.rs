pub const DEFAULT_FOCUS_RETRY_COOLDOWN_MS: u64 = 750;

pub fn next_focus_retry_at_ms(last_attempt_ms: u64, cooldown_ms: u64) -> u64 {
    last_attempt_ms.saturating_add(cooldown_ms.max(1))
}
