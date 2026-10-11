pub const SAND_FEEDBACK_MESSAGE_MAX_CHARS: usize = 10_000;
pub const SAND_FEEDBACK_CONVERSATION_ID_MAX_CHARS: usize = 512;
pub const SAND_FEEDBACK_ACCOUNT_SLOT_MAX_CHARS: usize = 512;
pub const SAND_FEEDBACK_SENTRY_EVENT_ID_MAX_COUNT: usize = 5;
pub const SAND_FEEDBACK_SENTRY_EVENT_ID_PATTERN: &str = "^[0-9a-f]{32}$";

pub fn is_valid_sentry_event_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sentry_ids_are_exactly_32_lowercase_hex_chars() {
        assert!(is_valid_sentry_event_id("0123456789abcdef0123456789abcdef"));
        assert!(!is_valid_sentry_event_id("0123456789ABCDEF0123456789ABCDEF"));
        assert!(!is_valid_sentry_event_id("abc"));
    }
}
