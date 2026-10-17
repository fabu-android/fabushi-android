use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CLIENT_SIDE_TOOL_V2_WIRE_VERSION: u32 = 1;
pub const CLIENT_SIDE_TOOL_V2_ACCOUNT_SLOT: &str = "host";
pub const CLIENT_SIDE_TOOL_V2_FAMILY: &str = "client-side-tool-v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientSideToolV2MessageKind {
    Call,
    Result,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientSideToolV2ProducedValue {
    pub kind: ClientSideToolV2MessageKind,
    pub tool_call_id: String,
    pub protobuf_bytes: Vec<u8>,
}

impl ClientSideToolV2ProducedValue {
    pub fn call(tool_call_id: impl Into<String>, protobuf_bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            kind: ClientSideToolV2MessageKind::Call,
            tool_call_id: tool_call_id.into(),
            protobuf_bytes: protobuf_bytes.into(),
        }
    }

    pub fn result(tool_call_id: impl Into<String>, protobuf_bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            kind: ClientSideToolV2MessageKind::Result,
            tool_call_id: tool_call_id.into(),
            protobuf_bytes: protobuf_bytes.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodedClientSideToolV2Message {
    pub encoding: String,
    pub message_type: String,
    pub bytes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientSideToolV2TransportKind {
    Call,
    Result,
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSideToolV2TransportEvent {
    pub version: u32,
    pub kind: ClientSideToolV2TransportKind,
    pub account_slot: String,
    pub agent_id: String,
    pub epoch: String,
    pub sequence: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<EncodedClientSideToolV2Message>,
}

pub struct ClientSideToolV2Producer {
    epoch: String,
    sequences: HashMap<String, u64>,
    open_calls: HashMap<String, HashSet<String>>,
}

impl Default for ClientSideToolV2Producer {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientSideToolV2Producer {
    pub fn new() -> Self {
        Self::with_epoch(Uuid::new_v4().to_string())
    }

    pub fn with_epoch(epoch: impl Into<String>) -> Self {
        Self {
            epoch: epoch.into(),
            sequences: HashMap::new(),
            open_calls: HashMap::new(),
        }
    }

    pub fn epoch(&self) -> &str {
        &self.epoch
    }

    pub fn publish(
        &mut self,
        agent_id: &str,
        produced: ClientSideToolV2ProducedValue,
    ) -> Option<ClientSideToolV2TransportEvent> {
        if agent_id.is_empty() || produced.tool_call_id.is_empty() {
            return None;
        }

        match produced.kind {
            ClientSideToolV2MessageKind::Call => {
                self.open_calls
                    .entry(agent_id.to_string())
                    .or_default()
                    .insert(produced.tool_call_id.clone());
            }
            ClientSideToolV2MessageKind::Result => {
                let open = self.open_calls.get_mut(agent_id)?;
                if !open.remove(&produced.tool_call_id) {
                    return None;
                }
                if open.is_empty() {
                    self.open_calls.remove(agent_id);
                }
            }
        }

        let sequence = self.next_sequence(agent_id);
        let (kind, message_type) = match produced.kind {
            ClientSideToolV2MessageKind::Call => (
                ClientSideToolV2TransportKind::Call,
                "aiserver.v1.ClientSideToolV2Call",
            ),
            ClientSideToolV2MessageKind::Result => (
                ClientSideToolV2TransportKind::Result,
                "aiserver.v1.ClientSideToolV2Result",
            ),
        };
        Some(ClientSideToolV2TransportEvent {
            version: CLIENT_SIDE_TOOL_V2_WIRE_VERSION,
            kind,
            account_slot: CLIENT_SIDE_TOOL_V2_ACCOUNT_SLOT.to_string(),
            agent_id: agent_id.to_string(),
            epoch: self.epoch.clone(),
            sequence,
            message: Some(EncodedClientSideToolV2Message {
                encoding: "protobuf-base64".into(),
                message_type: message_type.into(),
                bytes: STANDARD.encode(produced.protobuf_bytes),
            }),
        })
    }

    pub fn reset(&mut self, agent_id: &str) -> Option<ClientSideToolV2TransportEvent> {
        if agent_id.is_empty() {
            return None;
        }
        self.open_calls.remove(agent_id);
        Some(ClientSideToolV2TransportEvent {
            version: CLIENT_SIDE_TOOL_V2_WIRE_VERSION,
            kind: ClientSideToolV2TransportKind::Reset,
            account_slot: CLIENT_SIDE_TOOL_V2_ACCOUNT_SLOT.to_string(),
            agent_id: agent_id.to_string(),
            epoch: self.epoch.clone(),
            sequence: self.next_sequence(agent_id),
            message: None,
        })
    }

    fn next_sequence(&mut self, agent_id: &str) -> u64 {
        let next = self
            .sequences
            .get(agent_id)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        self.sequences.insert(agent_id.to_string(), next);
        next
    }
}


static PRODUCTION_CLIENT_SIDE_TOOL_V2_PRODUCER: OnceLock<Mutex<ClientSideToolV2Producer>> =
    OnceLock::new();

pub fn publish_production_client_side_tool_v2(
    agent_id: &str,
    produced: ClientSideToolV2ProducedValue,
) -> Option<ClientSideToolV2TransportEvent> {
    let producer = PRODUCTION_CLIENT_SIDE_TOOL_V2_PRODUCER
        .get_or_init(|| Mutex::new(ClientSideToolV2Producer::new()));
    producer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .publish(agent_id, produced)
}

pub fn reset_production_client_side_tool_v2(
    agent_id: &str,
) -> Option<ClientSideToolV2TransportEvent> {
    let producer = PRODUCTION_CLIENT_SIDE_TOOL_V2_PRODUCER
        .get_or_init(|| Mutex::new(ClientSideToolV2Producer::new()));
    producer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .reset(agent_id)
}


#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    #[test]
    fn call_result_and_reset_preserve_agent_sequence_and_identity() {
        let mut producer = ClientSideToolV2Producer::with_epoch("epoch-a");
        let call = producer
            .publish("agent-a", ClientSideToolV2ProducedValue::call("tool-1", [1, 2, 3]))
            .expect("call");
        assert_eq!(call.sequence, 1);
        assert_eq!(call.epoch, "epoch-a");
        assert_eq!(call.kind, ClientSideToolV2TransportKind::Call);
        assert_eq!(
            call.message.as_ref().unwrap().bytes,
            STANDARD.encode([1, 2, 3])
        );

        let result = producer
            .publish("agent-a", ClientSideToolV2ProducedValue::result("tool-1", [4, 5]))
            .expect("matching result");
        assert_eq!(result.sequence, 2);
        assert_eq!(result.kind, ClientSideToolV2TransportKind::Result);

        let reset = producer.reset("agent-a").expect("reset");
        assert_eq!(reset.sequence, 3);
        assert_eq!(reset.kind, ClientSideToolV2TransportKind::Reset);
        assert!(reset.message.is_none());
    }

    #[test]
    fn result_without_open_call_duplicate_result_and_empty_identity_fail_closed() {
        let mut producer = ClientSideToolV2Producer::with_epoch("epoch-a");
        assert!(producer
            .publish("agent-a", ClientSideToolV2ProducedValue::result("missing", [9]))
            .is_none());
        assert!(producer
            .publish("", ClientSideToolV2ProducedValue::call("tool-1", [1]))
            .is_none());
        assert!(producer
            .publish("agent-a", ClientSideToolV2ProducedValue::call("", [1]))
            .is_none());

        assert!(producer
            .publish("agent-a", ClientSideToolV2ProducedValue::call("tool-1", [1]))
            .is_some());
        assert!(producer
            .publish("agent-a", ClientSideToolV2ProducedValue::result("tool-1", [2]))
            .is_some());
        assert!(producer
            .publish("agent-a", ClientSideToolV2ProducedValue::result("tool-1", [3]))
            .is_none());
    }

    #[test]
    fn per_agent_sequences_are_independent_and_reset_discards_open_calls() {
        let mut producer = ClientSideToolV2Producer::with_epoch("epoch-a");
        assert_eq!(
            producer
                .publish("agent-a", ClientSideToolV2ProducedValue::call("a-1", [1]))
                .unwrap()
                .sequence,
            1
        );
        assert_eq!(
            producer
                .publish("agent-b", ClientSideToolV2ProducedValue::call("b-1", [1]))
                .unwrap()
                .sequence,
            1
        );
        assert_eq!(producer.reset("agent-a").unwrap().sequence, 2);
        assert!(producer
            .publish("agent-a", ClientSideToolV2ProducedValue::result("a-1", [2]))
            .is_none());
        assert_eq!(
            producer
                .publish("agent-b", ClientSideToolV2ProducedValue::result("b-1", [2]))
                .unwrap()
                .sequence,
            2
        );
    }
}
