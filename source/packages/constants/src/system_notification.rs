pub const SYSTEM_NOTIFICATION_TAG: &str = "system_notification";
pub const SYSTEM_NOTIFICATION_OPEN_TAG: &str = "<system_notification>";
pub const SYSTEM_NOTIFICATION_CLOSE_TAG: &str = "</system_notification>";

pub fn wrap_system_notification(body: &str) -> String {
    format!("{SYSTEM_NOTIFICATION_OPEN_TAG}{body}{SYSTEM_NOTIFICATION_CLOSE_TAG}")
}

pub fn is_system_notification(value: &str) -> bool {
    value.starts_with(SYSTEM_NOTIFICATION_OPEN_TAG)
        && value.ends_with(SYSTEM_NOTIFICATION_CLOSE_TAG)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapper_is_recognizable_without_matching_partial_tags() {
        let wrapped = wrap_system_notification("ready");
        assert!(is_system_notification(&wrapped));
        assert!(!is_system_notification("<system_notification>ready"));
    }
}
