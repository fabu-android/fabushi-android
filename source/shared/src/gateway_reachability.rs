pub const CLOUD_AGENT_STORAGE_DISABLED: &str = "CLOUD_AGENT_STORAGE_DISABLED";
pub const GATEWAY_NO_STORAGE_MESSAGE_MARKER: &str =
    "sand box access blocked by privacy mode (no_storage)";
pub const GATEWAY_ACCESS_DENIED_MESSAGE_MARKER: &str =
    "sand box access refused by backend access gate (access_denied)";
pub const SAND_BOX_BLOCKED: &str = "SAND_BOX_BLOCKED";
pub const SAND_BOX_BLOCK_REASON_KEY: &str = "sandBoxBlockReason";
pub const GATEWAY_BOX_BLOCKED_PREFIX: &str = "sand box blocked by kill switch: ";
pub const SAND_CLIENT_PAUSE_REASON: &str = "SAND_CLIENT_PAUSE";

const UNIT_SEPARATOR: char = '\u{001f}';

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandBoxBlockedInfo {
    pub reason: String,
    pub title: String,
    pub detail: String,
}

pub fn encode_sand_box_blocked_message(info: &SandBoxBlockedInfo) -> String {
    format!(
        "{GATEWAY_BOX_BLOCKED_PREFIX}{}{}{}{}{}",
        info.reason, UNIT_SEPARATOR, info.title, UNIT_SEPARATOR, info.detail
    )
}

pub fn has_sand_box_blocked_marker(message: &str) -> bool {
    message.contains(GATEWAY_BOX_BLOCKED_PREFIX)
}

pub fn sand_client_pause_blocked_message() -> String {
    encode_sand_box_blocked_message(&SandBoxBlockedInfo {
        reason: SAND_CLIENT_PAUSE_REASON.into(),
        title: String::new(),
        detail: String::new(),
    })
}

pub fn find_sand_box_blocked_message<'a, I>(messages: I) -> Option<String>
where
    I: IntoIterator<Item = &'a str>,
{
    for message in messages {
        if let Some(index) = message.find(GATEWAY_BOX_BLOCKED_PREFIX) {
            return Some(message[index..].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_message_is_structured_and_detectable() {
        let encoded = encode_sand_box_blocked_message(&SandBoxBlockedInfo {
            reason: "policy".into(),
            title: "Unavailable".into(),
            detail: "retry later".into(),
        });
        assert!(has_sand_box_blocked_marker(&encoded));
        assert!(encoded.contains("\u{001f}"));
        assert_eq!(
            find_sand_box_blocked_message(["wrapper", &format!("prefix: {encoded}")]),
            Some(encoded)
        );
    }

    #[test]
    fn pause_marker_uses_canonical_reason() {
        assert!(sand_client_pause_blocked_message().contains(SAND_CLIENT_PAUSE_REASON));
    }
}
