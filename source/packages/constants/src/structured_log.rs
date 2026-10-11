pub const STRUCTURED_LOG_REPLAY_MAX_AGE_MS: u64 = 17 * 60 * 60 * 1_000;
pub const STRUCTURED_LOG_FUTURE_TIMESTAMP_MAX_SKEW_MS: u64 = 2 * 60 * 60 * 1_000;

pub fn structured_log_timestamp_allowed(now_ms: u64, timestamp_ms: u64) -> bool {
    timestamp_ms
        .saturating_add(STRUCTURED_LOG_REPLAY_MAX_AGE_MS)
        >= now_ms
        && timestamp_ms
            <= now_ms.saturating_add(STRUCTURED_LOG_FUTURE_TIMESTAMP_MAX_SKEW_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_window_rejects_old_and_far_future_entries() {
        let now = 100 * 60 * 60 * 1_000;
        assert!(structured_log_timestamp_allowed(now, now));
        assert!(!structured_log_timestamp_allowed(
            now,
            now - STRUCTURED_LOG_REPLAY_MAX_AGE_MS - 1
        ));
        assert!(!structured_log_timestamp_allowed(
            now,
            now + STRUCTURED_LOG_FUTURE_TIMESTAMP_MAX_SKEW_MS + 1
        ));
    }
}
