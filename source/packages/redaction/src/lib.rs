pub mod classification;
pub mod core_message;
pub mod factory;
pub mod privacy_context;
pub mod privacy_mode;
#[allow(non_snake_case)]
#[path = "shouldRedact.rs"]
pub mod should_redact;
pub mod types;

#[cfg(test)]
mod tests {
    use super::{classification::DataClassification, factory::Redactor, privacy_context::PrivacyContext, privacy_mode::PrivacyMode};
    #[test]
    fn credentials_are_always_redacted() {
        let redactor = Redactor::new(PrivacyContext { mode: PrivacyMode::Balanced });
        assert_eq!(redactor.redact(DataClassification::Credential, "secret").value, "[redacted]");
        assert_eq!(redactor.redact(DataClassification::Public, "hello").value, "hello");
    }
}
