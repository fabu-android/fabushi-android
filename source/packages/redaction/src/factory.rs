use crate::{
    classification::DataClassification,
    privacy_context::PrivacyContext,
    should_redact::should_redact,
    types::RedactionResult,
};

#[derive(Clone, Copy, Debug)]
pub struct Redactor { context: PrivacyContext }

impl Redactor {
    pub fn new(context: PrivacyContext) -> Self { Self { context } }
    pub fn redact(&self, classification: DataClassification, value: &str) -> RedactionResult {
        if should_redact(self.context, classification) {
            RedactionResult { value: "[redacted]".into(), redacted: true }
        } else {
            RedactionResult { value: value.into(), redacted: false }
        }
    }
}
