pub const TRANSCRIPTS_SUBDIR: &str = "agent-transcripts";
pub const MAX_CONVERSATION_ID_LENGTH: usize = 200;

pub fn get_safe_conversation_id(conversation_id: &str) -> String {
    let mut encoded = String::new();
    for byte in conversation_id.as_bytes() {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(*byte, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')');
        if keep {
            encoded.push(*byte as char);
        } else {
            encoded.push('_');
            encoded.push_str(&format!("{:02X}", byte));
        }
        if encoded.len() >= MAX_CONVERSATION_ID_LENGTH {
            encoded.truncate(MAX_CONVERSATION_ID_LENGTH);
            break;
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversation_ids_are_uri_encoded_percent_rewritten_and_bounded() {
        assert_eq!(get_safe_conversation_id("a/b c"), "a_2Fb_20c");
        assert_eq!(get_safe_conversation_id("hello-world_1"), "hello-world_1");
        assert_eq!(get_safe_conversation_id(&"x".repeat(250)).len(), 200);
        assert!(get_safe_conversation_id("佛").starts_with("_E4_BD"));
    }
}
