use crate::account_telemetry::AccountTelemetryProjection;
use crate::android_structured_log_spill::{
    AndroidStructuredLogSpillState, FileAndroidStructuredLogSpill, SpillLoad,
    StructuredLogRecord, TelemetryDropReason,
};
use serde_json::{Map, Value};
use std::{collections::BTreeMap, io, path::Path};

const SESSION_EVENT: &str = "sand.desktop.session";
const SIGNIN_EVENT: &str = "sand.desktop.signin";

pub struct DurableAccountTelemetryOutbox {
    spill: FileAndroidStructuredLogSpill,
    state: AndroidStructuredLogSpillState,
}

impl DurableAccountTelemetryOutbox {
    pub fn open(
        path: impl AsRef<Path>,
        account_slot: &str,
        counter_id: &str,
        now_ms: u64,
    ) -> io::Result<Self> {
        let spill = FileAndroidStructuredLogSpill::new(path.as_ref());
        let state = match spill.load(account_slot, now_ms)? {
            SpillLoad::Loaded(state) => state,
            SpillLoad::AccountRotated => {
                let mut state = AndroidStructuredLogSpillState::empty(account_slot, counter_id);
                state.observe_drop(TelemetryDropReason::AccountRotated, 1);
                spill.replace(&state, now_ms)?.unwrap_or(state)
            }
            SpillLoad::InvalidDiscarded | SpillLoad::OversizeDiscarded | SpillLoad::Empty => {
                AndroidStructuredLogSpillState::empty(account_slot, counter_id)
            }
        };
        Ok(Self { spill, state })
    }

    pub fn push(
        &mut self,
        entries: impl IntoIterator<Item = AccountTelemetryProjection>,
        now_ms: u64,
    ) -> io::Result<()> {
        for entry in entries {
            if let Some(record) = projection_to_record(entry, now_ms) {
                self.state.checkpoint.records.push(record);
            }
        }
        if let Some(state) = self.spill.replace(&self.state, now_ms)? {
            self.state = state;
        }
        Ok(())
    }

    pub fn account_slot(&self) -> &str {
        &self.state.account_slot
    }

    pub fn batch(&self) -> Vec<AccountTelemetryProjection> {
        self.state
            .checkpoint
            .records
            .iter()
            .filter_map(record_to_projection)
            .collect()
    }

    pub fn mark_ship_failed(&mut self, now_ms: u64) -> io::Result<()> {
        self.state.observe_drop(TelemetryDropReason::ShipFailed, 1);
        if let Some(state) = self.spill.replace(&self.state, now_ms)? {
            self.state = state;
        }
        Ok(())
    }

    pub fn settle_success(
        &mut self,
        processed: u64,
        backend_dropped: u64,
        now_ms: u64,
    ) -> io::Result<()> {
        let acknowledged = processed.saturating_add(backend_dropped) as usize;
        let acknowledged = acknowledged.min(self.state.checkpoint.records.len());
        self.state.checkpoint.records.drain(..acknowledged);
        if backend_dropped > 0 {
            self.state
                .observe_drop(TelemetryDropReason::BackendDropped, backend_dropped);
        }
        self.state.acknowledge_drop_counters();
        match self.spill.replace(&self.state, now_ms)? {
            Some(state) => self.state = state,
            None => {
                self.state = AndroidStructuredLogSpillState::empty(
                    self.state.account_slot.clone(),
                    self.state.checkpoint.counter_id.clone(),
                )
            }
        }
        Ok(())
    }
}

fn projection_to_record(
    projection: AccountTelemetryProjection,
    timestamp: u64,
) -> Option<StructuredLogRecord> {
    let message = match projection.stream.as_str() {
        "session" => SESSION_EVENT,
        "signin" => SIGNIN_EVENT,
        _ => return None,
    };
    let metadata = projection
        .metadata
        .as_object()
        .map(|values| {
            values
                .iter()
                .filter_map(|(key, value)| scalar_string(value).map(|value| (key.clone(), value)))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    Some(StructuredLogRecord {
        level: projection.level,
        message: message.into(),
        metadata,
        timestamp,
    })
}

fn record_to_projection(record: &StructuredLogRecord) -> Option<AccountTelemetryProjection> {
    let stream = match record.message.as_str() {
        SESSION_EVENT => "session",
        SIGNIN_EVENT => "signin",
        _ => return None,
    };
    let metadata = record
        .metadata
        .iter()
        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
        .collect::<Map<_, _>>();
    Some(AccountTelemetryProjection {
        stream: stream.into(),
        level: record.level.clone(),
        metadata: Value::Object(metadata),
    })
}

fn scalar_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.chars().take(4096).collect()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn outbox_reopens_account_telemetry_and_acks_backend_receipt() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("account-telemetry-spill.json");
        {
            let mut outbox =
                DurableAccountTelemetryOutbox::open(&path, "slot-a", "counter-a", 1_000).unwrap();
            outbox
                .push(
                    [AccountTelemetryProjection {
                        stream: "signin".into(),
                        level: "warn".into(),
                        metadata: json!({"phase":"login_started","nested":{"secret":"discard"}}),
                    }],
                    1_000,
                )
                .unwrap();
        }
        let mut reopened =
            DurableAccountTelemetryOutbox::open(&path, "slot-a", "counter-a", 1_001).unwrap();
        let batch = reopened.batch();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].stream, "signin");
        assert_eq!(batch[0].metadata["phase"], "login_started");
        assert!(batch[0].metadata.get("nested").is_none());
        reopened.settle_success(1, 0, 1_002).unwrap();
        assert!(reopened.batch().is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn foreign_account_spill_is_never_replayed() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("account-telemetry-spill.json");
        let mut first =
            DurableAccountTelemetryOutbox::open(&path, "slot-a", "counter-a", 1_000).unwrap();
        first
            .push(
                [AccountTelemetryProjection {
                    stream: "session".into(),
                    level: "warn".into(),
                    metadata: json!({"phase":"signed_out"}),
                }],
                1_000,
            )
            .unwrap();
        drop(first);

        let second =
            DurableAccountTelemetryOutbox::open(&path, "slot-b", "counter-b", 1_001).unwrap();
        assert!(second.batch().is_empty());
        assert!(second.state.has_pending_drops());
    }
}
