pub fn is_project_send_message_enabled(is_root_project_conversation: Option<bool>) -> bool {
    is_root_project_conversation == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_message_requires_explicit_root_project_state() {
        assert!(is_project_send_message_enabled(Some(true)));
        assert!(!is_project_send_message_enabled(Some(false)));
        assert!(!is_project_send_message_enabled(None));
    }
}
