use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;

pub const RUNTIME_TELEMETRY_BUFFER_CAP: usize = 64;
pub const RUNTIME_TELEMETRY_DURATION_CAP_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const TOKEN_MAX_CHARS: usize = 96;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RuntimeTelemetryRecord {
    pub event: String,
    pub level: String,
    pub metadata: Value,
}

#[derive(Default)]
pub struct RuntimeTelemetryBuffer {
    entries: VecDeque<RuntimeTelemetryRecord>,
}

impl RuntimeTelemetryBuffer {
    pub fn observe_agents_unreachable(
        &mut self,
        phase: &str,
        is_manual: bool,
        error_class: Option<&str>,
        ms_since_last_reached: Option<u64>,
    ) -> Result<RuntimeTelemetryRecord, String> {
        if !matches!(phase, "entered" | "retry" | "recovered") {
            return Err("agents-unreachable phase is invalid".into());
        }
        let mut metadata = serde_json::Map::from_iter([
            ("phase".into(), json!(phase)),
            ("is_manual".into(), json!(is_manual.to_string())),
        ]);
        if let Some(error_class) = error_class {
            metadata.insert("error_class".into(), json!(bounded_token(error_class)));
        }
        if let Some(ms) = ms_since_last_reached {
            metadata.insert(
                "ms_since_last_reached".into(),
                json!(ms.min(RUNTIME_TELEMETRY_DURATION_CAP_MS).to_string()),
            );
        }
        let record = RuntimeTelemetryRecord {
            event: "sand.agents_unreachable".into(),
            level: if phase == "entered" { "warn" } else { "info" }.into(),
            metadata: Value::Object(metadata),
        };
        self.push(record.clone());
        Ok(record)
    }

    pub fn snapshot(&self) -> Vec<RuntimeTelemetryRecord> {
        self.entries.iter().cloned().collect()
    }

    pub fn drain(&mut self) -> Vec<RuntimeTelemetryRecord> {
        self.entries.drain(..).collect()
    }

    fn push(&mut self, record: RuntimeTelemetryRecord) {
        if self.entries.len() >= RUNTIME_TELEMETRY_BUFFER_CAP {
            self.entries.pop_front();
        }
        self.entries.push_back(record);
    }
}

fn bounded_token(value: &str) -> String {
    let normalized: String = value
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-' | '/'))
        .take(TOKEN_MAX_CHARS)
        .collect();
    if normalized.is_empty() { "Error".into() } else { normalized }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreachable_projection_matches_desktop_phase_and_bounds_sensitive_fields() {
        let mut buffer = RuntimeTelemetryBuffer::default();
        let record = buffer
            .observe_agents_unreachable(
                "entered",
                false,
                Some("NetworkError bearer secret must not survive"),
                Some(u64::MAX),
            )
            .unwrap();
        assert_eq!(record.event, "sand.agents_unreachable");
        assert_eq!(record.level, "warn");
        assert_eq!(record.metadata["phase"], "entered");
        assert_eq!(record.metadata["is_manual"], "false");
        assert_eq!(
            record.metadata["ms_since_last_reached"],
            RUNTIME_TELEMETRY_DURATION_CAP_MS.to_string()
        );
        assert!(!serde_json::to_string(&record).unwrap().contains("secret"));
    }

    #[test]
    fn unreachable_projection_rejects_unknown_phase_and_keeps_buffer_bounded() {
        let mut buffer = RuntimeTelemetryBuffer::default();
        assert!(buffer
            .observe_agents_unreachable("other", false, None, None)
            .is_err());
        for index in 0..(RUNTIME_TELEMETRY_BUFFER_CAP + 5) {
            buffer
                .observe_agents_unreachable(
                    if index % 2 == 0 { "retry" } else { "recovered" },
                    true,
                    Some("NetworkError"),
                    Some(index as u64),
                )
                .unwrap();
        }
        assert_eq!(buffer.snapshot().len(), RUNTIME_TELEMETRY_BUFFER_CAP);
        assert_eq!(buffer.drain().len(), RUNTIME_TELEMETRY_BUFFER_CAP);
        assert!(buffer.snapshot().is_empty());
    }
}
