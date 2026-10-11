use crate::{classification::DataClassification, privacy_context::PrivacyContext, privacy_mode::PrivacyMode};

pub fn should_redact(context: PrivacyContext, classification: DataClassification) -> bool {
    match classification {
        DataClassification::Credential => true,
        DataClassification::Private => !matches!(context.mode, PrivacyMode::Off),
        DataClassification::Public => matches!(context.mode, PrivacyMode::Strict),
    }
}
