pub const SAND_FEEDBACK_MESSAGE_MAX_CHARS: usize = 10_000;
pub const SAND_FEEDBACK_CONVERSATION_ID_MAX_CHARS: usize = 512;
pub const SAND_FEEDBACK_ACCOUNT_SLOT_MAX_CHARS: usize = 512;
pub const SAND_FEEDBACK_SENTRY_EVENT_ID_MAX_COUNT: usize = 5;
pub const SAND_FEEDBACK_SENTRY_EVENT_ID_PATTERN: &str = "^[0-9a-f]{32}$";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedbackSubmitArgs {
    pub account_slot: String,
    pub message: String,
    pub submission_id: String,
    pub conversation_id: Option<String>,
    pub sentry_event_ids: Vec<String>,
}

pub fn is_valid_sentry_event_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub fn is_valid_feedback_submission_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for index in [8, 13, 18, 23] {
        if bytes[index] != b'-' {
            return false;
        }
    }
    let is_hex = |byte: u8| byte.is_ascii_hexdigit();
    bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| matches!(index, 8 | 13 | 18 | 23) || is_hex(*byte))
        && matches!(bytes[14].to_ascii_lowercase(), b'1'..=b'8')
        && matches!(bytes[19].to_ascii_lowercase(), b'8' | b'9' | b'a' | b'b')
}

pub fn normalize_feedback_submit_args(
    account_slot: &str,
    message: &str,
    submission_id: &str,
    conversation_id: Option<&str>,
    sentry_event_ids: impl IntoIterator<Item = String>,
) -> Option<FeedbackSubmitArgs> {
    let account_slot = account_slot.trim();
    let message = message.trim();
    let submission_id = submission_id.trim();
    if account_slot.is_empty()
        || account_slot.len() > SAND_FEEDBACK_ACCOUNT_SLOT_MAX_CHARS
        || message.is_empty()
        || message.len() > SAND_FEEDBACK_MESSAGE_MAX_CHARS
        || !is_valid_feedback_submission_id(submission_id)
    {
        return None;
    }
    let conversation_id = match conversation_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) if value.len() > SAND_FEEDBACK_CONVERSATION_ID_MAX_CHARS => return None,
        Some(value) => Some(value.to_string()),
        None => None,
    };
    let sentry_event_ids = sentry_event_ids
        .into_iter()
        .filter(|value| is_valid_sentry_event_id(value))
        .take(SAND_FEEDBACK_SENTRY_EVENT_ID_MAX_COUNT)
        .collect();
    Some(FeedbackSubmitArgs {
        account_slot: account_slot.to_string(),
        message: message.to_string(),
        submission_id: submission_id.to_string(),
        conversation_id,
        sentry_event_ids,
    })
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

    #[test]
    fn feedback_contract_matches_desktop_uuid_bounds_and_diagnostic_cap() {
        let parsed = normalize_feedback_submit_args(
            " account@example.com ",
            " hello ",
            "123e4567-e89b-12d3-a456-426614174000",
            Some(" conversation "),
            vec![
                "0123456789abcdef0123456789abcdef".into(),
                "11111111111111111111111111111111".into(),
                "22222222222222222222222222222222".into(),
                "33333333333333333333333333333333".into(),
                "44444444444444444444444444444444".into(),
                "55555555555555555555555555555555".into(),
                "NOT-A-SENTRY-ID".into(),
            ],
        )
        .unwrap();
        assert_eq!(parsed.account_slot, "account@example.com");
        assert_eq!(parsed.message, "hello");
        assert_eq!(parsed.conversation_id.as_deref(), Some("conversation"));
        assert_eq!(parsed.sentry_event_ids.len(), 5);
        assert!(normalize_feedback_submit_args(
            "slot",
            "message",
            "not-a-uuid",
            None,
            Vec::new(),
        )
        .is_none());
        assert!(normalize_feedback_submit_args(
            "slot",
            &"x".repeat(SAND_FEEDBACK_MESSAGE_MAX_CHARS + 1),
            "123e4567-e89b-12d3-a456-426614174000",
            None,
            Vec::new(),
        )
        .is_none());
    }
}
