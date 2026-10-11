pub const DEFAULT_ENVIRONMENT_SETUP_MAX_RESUME_AGE_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
pub const MIN_ENVIRONMENT_SETUP_MAX_RESUME_AGE_MS: u64 = 60 * 60 * 1_000;
pub const MAX_ENVIRONMENT_SETUP_MAX_RESUME_AGE_MS: u64 = 30 * 24 * 60 * 60 * 1_000;

pub fn clamp_environment_setup_resume_age_ms(value: u64) -> u64 {
    value.clamp(
        MIN_ENVIRONMENT_SETUP_MAX_RESUME_AGE_MS,
        MAX_ENVIRONMENT_SETUP_MAX_RESUME_AGE_MS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_age_policy_is_bounded() {
        assert_eq!(
            clamp_environment_setup_resume_age_ms(0),
            MIN_ENVIRONMENT_SETUP_MAX_RESUME_AGE_MS
        );
        assert_eq!(
            clamp_environment_setup_resume_age_ms(u64::MAX),
            MAX_ENVIRONMENT_SETUP_MAX_RESUME_AGE_MS
        );
        assert_eq!(
            clamp_environment_setup_resume_age_ms(DEFAULT_ENVIRONMENT_SETUP_MAX_RESUME_AGE_MS),
            DEFAULT_ENVIRONMENT_SETUP_MAX_RESUME_AGE_MS
        );
    }
}
