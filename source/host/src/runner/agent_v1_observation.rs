use super::android_host_inference::{
    AndroidToolCallObservation, AndroidToolCallObservationSink,
};
use crate::extensions::transcript::TranscriptStore;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// Persists the canonical agent.v1 Runner observation into the existing canonical
/// transcript store. This deliberately reuses TranscriptStore rather than creating
/// a second tool-call state owner.
pub struct DurableAgentV1ObservationOwner {
    transcript: Arc<Mutex<TranscriptStore>>,
    account_fence: String,
}

impl DurableAgentV1ObservationOwner {
    pub fn new(
        transcript: Arc<Mutex<TranscriptStore>>,
        account_fence: impl Into<String>,
    ) -> Self {
        Self {
            transcript,
            account_fence: account_fence.into(),
        }
    }

    fn entry_id(observation: &AndroidToolCallObservation) -> String {
        format!(
            "agent-v1-tool-observation:{}:{}",
            observation.operation_id, observation.tool_call_id
        )
    }

    fn entry(&self, observation: &AndroidToolCallObservation) -> Value {
        json!({
            "id": Self::entry_id(observation),
            "kind": "tool-call",
            "agentId": observation.agent_id,
            "operationId": observation.operation_id,
            "accountFence": self.account_fence,
            "toolName": observation.tool_name,
            "toolCallId": observation.tool_call_id,
            "status": if observation.succeeded { "success" } else { "error" },
            "startedAtMs": observation.started_at_ms,
            "completedAtMs": observation.completed_at_ms,
            "wireType": "agent.v1.AgentToolCall",
            "wireEncoding": "protobuf-base64",
            "wireBytes": BASE64_STANDARD.encode(&observation.protobuf_bytes),
        })
    }
}

impl AndroidToolCallObservationSink for DurableAgentV1ObservationOwner {
    fn record(&self, observation: &AndroidToolCallObservation) -> Result<(), String> {
        if observation.agent_id.trim().is_empty()
            || observation.operation_id.trim().is_empty()
            || observation.tool_call_id.trim().is_empty()
            || observation.protobuf_bytes.is_empty()
        {
            return Err("canonical agent.v1 observation identity or bytes are missing".into());
        }
        if observation.completed_at_ms < observation.started_at_ms {
            return Err("canonical agent.v1 observation timestamp order is invalid".into());
        }

        let expected = self.entry(observation);
        let id = Self::entry_id(observation);
        let mut transcript = self
            .transcript
            .lock()
            .map_err(|_| "transcript lock poisoned while recording agent.v1 observation".to_string())?;
        if let Some(existing) = transcript.entry(&id) {
            if existing == expected {
                return Ok(());
            }
            return Err(format!(
                "conflicting duplicate canonical agent.v1 observation for {id}"
            ));
        }
        transcript
            .append_entry(expected)
            .map_err(|error| format!("failed to persist canonical agent.v1 observation: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(bytes: Vec<u8>) -> AndroidToolCallObservation {
        AndroidToolCallObservation {
            agent_id: "agent-a".into(),
            operation_id: "operation-a".into(),
            tool_name: "SendMessage".into(),
            tool_call_id: "call-a".into(),
            protobuf_bytes: bytes,
            started_at_ms: 10,
            completed_at_ms: 12,
            succeeded: true,
        }
    }

    #[test]
    fn canonical_observation_is_durable_idempotent_and_conflicting_duplicate_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("transcript.json");
        let transcript = Arc::new(Mutex::new(TranscriptStore::open(&path).unwrap()));
        let owner = DurableAgentV1ObservationOwner::new(Arc::clone(&transcript), "account-a");

        let first = observation(vec![0xda, 0x03, 0x00]);
        owner.record(&first).unwrap();
        owner.record(&first).unwrap();
        assert_eq!(transcript.lock().unwrap().get_transcript().len(), 1);

        let mut conflict = first.clone();
        conflict.protobuf_bytes.push(1);
        assert!(owner.record(&conflict).unwrap_err().contains("conflicting duplicate"));

        drop(owner);
        drop(transcript);
        let reopened = TranscriptStore::open(&path).unwrap();
        let entry = reopened
            .entry("agent-v1-tool-observation:operation-a:call-a")
            .expect("durable observation");
        assert_eq!(entry["kind"], "tool-call");
        assert_eq!(entry["accountFence"], "account-a");
        assert_eq!(entry["wireType"], "agent.v1.AgentToolCall");
        assert_eq!(entry["wireEncoding"], "protobuf-base64");
        assert_eq!(entry["wireBytes"], BASE64_STANDARD.encode(first.protobuf_bytes));
    }
}
