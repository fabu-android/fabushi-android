pub const DEV_SMART_MODE_CLASSIFIER_DELAY_MS: u64 = 10_000;
pub const DEV_SMART_MODE_CLASSIFIER_DELAY_SECONDS: u64 =
    DEV_SMART_MODE_CLASSIFIER_DELAY_MS / 1_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn milliseconds_and_seconds_are_consistent() {
        assert_eq!(DEV_SMART_MODE_CLASSIFIER_DELAY_SECONDS, 10);
    }
}
