#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeLogEvent { pub category: String, pub message: String }

impl SafeLogEvent {
    pub fn new(category: impl Into<String>, message: impl Into<String>) -> Self {
        let category = category.into();
        let raw = message.into();
        let lowered = raw.to_ascii_lowercase();
        let message = if ["authorization", "access_token", "password", "cookie"].iter().any(|marker| lowered.contains(marker)) {
            "[redacted]".into()
        } else { raw };
        Self { category, message }
    }
}
