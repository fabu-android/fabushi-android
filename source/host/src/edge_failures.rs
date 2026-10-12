use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const EDGE_FAILURE_BUFFER_CAP: usize = 32;
const EDGE_LABEL_MAX_CHARS: usize = 128;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeFailure {
    pub area: String,
    pub leg: String,
    #[serde(rename = "errorClass")]
    pub error_class: String,
}

#[derive(Default)]
pub struct EdgeFailureBuffer {
    entries: VecDeque<EdgeFailure>,
}

impl EdgeFailureBuffer {
    pub fn report(&mut self, area: &str, leg: &str, error_class: &str) {
        if self.entries.len() >= EDGE_FAILURE_BUFFER_CAP { return; }
        self.entries.push_back(EdgeFailure {
            area: bounded_label(area),
            leg: bounded_label(leg),
            error_class: bounded_error_class(error_class),
        });
    }
    pub fn snapshot(&self) -> Vec<EdgeFailure> { self.entries.iter().cloned().collect() }
    pub fn drain(&mut self) -> Vec<EdgeFailure> { self.entries.drain(..).collect() }
}

fn bounded_label(value: &str) -> String {
    let normalized: String = value.trim().chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-' | '/'))
        .take(EDGE_LABEL_MAX_CHARS).collect();
    if normalized.is_empty() { "unknown".into() } else { normalized }
}

fn bounded_error_class(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > EDGE_LABEL_MAX_CHARS ||
        !value.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-')) {
        "Error".into()
    } else { value.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pre_install_buffer_matches_desktop_cap_and_rejects_error_details() {
        let mut buffer = EdgeFailureBuffer::default();
        buffer.report("android-host-jni", "feature.account.profileUpdate", "HostError secret-token-must-not-survive");
        assert_eq!(buffer.snapshot()[0].error_class, "Error");
        for index in 1..(EDGE_FAILURE_BUFFER_CAP + 5) {
            buffer.report("android-host-jni", &format!("feature.account.call-{index}"), "HostError");
        }
        let snapshot = buffer.snapshot();
        assert_eq!(snapshot.len(), EDGE_FAILURE_BUFFER_CAP);
        assert_eq!(snapshot[0].leg, "feature.account.profileUpdate");
        assert!(!serde_json::to_string(&snapshot).unwrap().contains("secret-token"));
        assert_eq!(buffer.drain().len(), EDGE_FAILURE_BUFFER_CAP);
        assert!(buffer.snapshot().is_empty());
    }
}
