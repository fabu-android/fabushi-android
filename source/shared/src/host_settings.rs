#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationConfig {
    pub is_enabled: bool,
    pub allowed_apps: Vec<String>,
    pub min_interval_ms: u64,
    pub max_per_window: u32,
    pub window_ms: u64,
}

pub fn disabled_notification_config() -> NotificationConfig {
    NotificationConfig {
        is_enabled: false,
        allowed_apps: Vec::new(),
        min_interval_ms: 5_000,
        max_per_window: 10,
        window_ms: 5 * 60_000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_notifications_are_bounded_even_when_enabled_later() {
        let value = disabled_notification_config();
        assert!(!value.is_enabled);
        assert!(value.allowed_apps.is_empty());
        assert_eq!(value.min_interval_ms, 5_000);
        assert_eq!(value.max_per_window, 10);
        assert_eq!(value.window_ms, 300_000);
    }
}
