use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const CLIENT_SIDE_TOOL_V2_FAMILY: &str = "client-side-tool-v2";
pub const CLIENT_SIDE_TOOL_V2_WIRE_VERSION: u32 = 1;
pub const CLIENT_SIDE_TOOL_V2_ACCOUNT_SLOT: &str = "host";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolMessageKind {
    Call,
    Result,
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EncodedToolMessage {
    pub encoding: String,
    pub message_type: String,
    pub bytes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolTransportEvent {
    pub version: u32,
    pub kind: ToolMessageKind,
    pub account_slot: String,
    pub agent_id: String,
    pub epoch: String,
    pub sequence: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<EncodedToolMessage>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererToolEvent {
    pub version: u32,
    pub kind: ToolMessageKind,
    pub account_slot: String,
    pub agent_id: String,
    pub epoch: String,
    pub sequence: u64,
    pub message_type: Option<String>,
    pub bytes: Option<Vec<u8>>,
}

#[derive(Debug, Default)]
struct AgentFence {
    epoch: String,
    sequence: u64,
    retired_epochs: BTreeSet<String>,
    updates_by_tool_call_id: BTreeMap<String, Vec<ToolTransportEvent>>,
}

#[derive(Debug, Default)]
pub struct ClientSideToolV2Relay {
    agents: BTreeMap<String, AgentFence>,
}

fn read_varint(bytes: &[u8], cursor: &mut usize) -> Option<u64> {
    let mut value = 0_u64;
    let mut shift = 0_u32;
    while *cursor < bytes.len() && shift <= 63 {
        let byte = bytes[*cursor];
        *cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
    }
    None
}

fn extract_length_delimited_field(bytes: &[u8], wanted_field: u32) -> Option<Vec<u8>> {
    let mut cursor = 0;
    while cursor < bytes.len() {
        let key = read_varint(bytes, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        match wire {
            0 => {
                read_varint(bytes, &mut cursor)?;
            }
            1 => {
                cursor = cursor.checked_add(8)?;
                if cursor > bytes.len() {
                    return None;
                }
            }
            2 => {
                let len = usize::try_from(read_varint(bytes, &mut cursor)?).ok()?;
                let end = cursor.checked_add(len)?;
                if end > bytes.len() {
                    return None;
                }
                if field == wanted_field {
                    return Some(bytes[cursor..end].to_vec());
                }
                cursor = end;
            }
            5 => {
                cursor = cursor.checked_add(4)?;
                if cursor > bytes.len() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    None
}

fn decode_message(kind: ToolMessageKind, message: &EncodedToolMessage) -> Option<(Vec<u8>, String)> {
    if message.encoding != "protobuf-base64" {
        return None;
    }
    let expected = match kind {
        ToolMessageKind::Call => "aiserver.v1.ClientSideToolV2Call",
        ToolMessageKind::Result => "aiserver.v1.ClientSideToolV2Result",
        ToolMessageKind::Reset => return None,
    };
    if message.message_type != expected || message.bytes.is_empty() || message.bytes.len() % 4 != 0 {
        return None;
    }
    let bytes = STANDARD.decode(&message.bytes).ok()?;
    if STANDARD.encode(&bytes) != message.bytes {
        return None;
    }
    let tool_call_field = match kind {
        ToolMessageKind::Call => 3,
        ToolMessageKind::Result => 35,
        ToolMessageKind::Reset => unreachable!(),
    };
    let tool_call_bytes = extract_length_delimited_field(&bytes, tool_call_field)?;
    let tool_call_id = String::from_utf8(tool_call_bytes).ok()?;
    if tool_call_id.trim().is_empty() || tool_call_id.chars().any(char::is_control) {
        return None;
    }
    Some((bytes, tool_call_id))
}

fn materialize(event: &ToolTransportEvent) -> Option<RendererToolEvent> {
    if event.kind == ToolMessageKind::Reset {
        return Some(RendererToolEvent {
            version: event.version,
            kind: event.kind,
            account_slot: event.account_slot.clone(),
            agent_id: event.agent_id.clone(),
            epoch: event.epoch.clone(),
            sequence: event.sequence,
            message_type: None,
            bytes: None,
        });
    }
    let message = event.message.as_ref()?;
    let (bytes, _) = decode_message(event.kind, message)?;
    Some(RendererToolEvent {
        version: event.version,
        kind: event.kind,
        account_slot: event.account_slot.clone(),
        agent_id: event.agent_id.clone(),
        epoch: event.epoch.clone(),
        sequence: event.sequence,
        message_type: Some(message.message_type.clone()),
        bytes: Some(bytes),
    })
}

impl ClientSideToolV2Relay {
    pub fn accept_value(&mut self, raw: Value) -> Option<RendererToolEvent> {
        let event = serde_json::from_value::<ToolTransportEvent>(raw).ok()?;
        self.accept(event)
    }

    pub fn accept(&mut self, event: ToolTransportEvent) -> Option<RendererToolEvent> {
        if event.version != CLIENT_SIDE_TOOL_V2_WIRE_VERSION
            || event.account_slot != CLIENT_SIDE_TOOL_V2_ACCOUNT_SLOT
            || event.agent_id.trim().is_empty()
            || event.epoch.trim().is_empty()
            || event.agent_id.chars().any(char::is_control)
            || event.epoch.chars().any(char::is_control)
            || event.sequence < 1
        {
            return None;
        }

        let decoded_tool_call_id = match event.kind {
            ToolMessageKind::Reset => {
                if event.message.is_some() {
                    return None;
                }
                None
            }
            ToolMessageKind::Call | ToolMessageKind::Result => {
                let (_, tool_call_id) = decode_message(event.kind, event.message.as_ref()?)?;
                Some(tool_call_id)
            }
        };

        let fence = self.agents.entry(event.agent_id.clone()).or_default();
        if fence.epoch.is_empty() {
            if fence.retired_epochs.contains(&event.epoch) || event.kind == ToolMessageKind::Result {
                return None;
            }
            fence.epoch = event.epoch.clone();
        } else if fence.epoch != event.epoch {
            if fence.retired_epochs.contains(&event.epoch) || event.kind == ToolMessageKind::Result {
                return None;
            }
            fence.retired_epochs.insert(std::mem::replace(&mut fence.epoch, event.epoch.clone()));
            fence.sequence = 0;
            fence.updates_by_tool_call_id.clear();
        }

        if event.sequence <= fence.sequence {
            return None;
        }

        match event.kind {
            ToolMessageKind::Reset => {
                fence.updates_by_tool_call_id.clear();
            }
            ToolMessageKind::Call => {
                let tool_call_id = decoded_tool_call_id?;
                if fence.updates_by_tool_call_id.contains_key(&tool_call_id) {
                    return None;
                }
                fence
                    .updates_by_tool_call_id
                    .insert(tool_call_id, vec![event.clone()]);
            }
            ToolMessageKind::Result => {
                let tool_call_id = decoded_tool_call_id?;
                let lifecycle = fence.updates_by_tool_call_id.get_mut(&tool_call_id)?;
                if lifecycle.len() != 1 || lifecycle[0].kind != ToolMessageKind::Call {
                    return None;
                }
                lifecycle.push(event.clone());
            }
        }

        fence.sequence = event.sequence;
        materialize(&event)
    }

    pub fn replay(&self) -> Vec<RendererToolEvent> {
        let mut events = self
            .agents
            .values()
            .flat_map(|fence| fence.updates_by_tool_call_id.values())
            .flat_map(|updates| updates.iter())
            .collect::<Vec<_>>();
        events.sort_by_key(|event| event.sequence);
        events.into_iter().filter_map(materialize).collect()
    }

    pub fn retire_for_account_switch(&mut self) {
        for fence in self.agents.values_mut() {
            if !fence.epoch.is_empty() {
                fence.retired_epochs.insert(std::mem::take(&mut fence.epoch));
            }
            fence.sequence = 0;
            fence.updates_by_tool_call_id.clear();
        }
    }

    pub fn clear(&mut self) {
        self.agents.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn varint(mut value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if value == 0 {
                return out;
            }
        }
    }

    fn protobuf_id(field: u32, id: &str) -> Vec<u8> {
        let mut out = varint(u64::from((field << 3) | 2));
        out.extend(varint(id.len() as u64));
        out.extend(id.as_bytes());
        out
    }

    fn message(kind: ToolMessageKind, id: &str) -> EncodedToolMessage {
        let (message_type, field) = match kind {
            ToolMessageKind::Call => ("aiserver.v1.ClientSideToolV2Call", 3),
            ToolMessageKind::Result => ("aiserver.v1.ClientSideToolV2Result", 35),
            ToolMessageKind::Reset => panic!("reset has no message"),
        };
        EncodedToolMessage {
            encoding: "protobuf-base64".into(),
            message_type: message_type.into(),
            bytes: STANDARD.encode(protobuf_id(field, id)),
        }
    }

    fn event(sequence: u64, kind: ToolMessageKind, id: &str) -> ToolTransportEvent {
        ToolTransportEvent {
            version: CLIENT_SIDE_TOOL_V2_WIRE_VERSION,
            kind,
            account_slot: CLIENT_SIDE_TOOL_V2_ACCOUNT_SLOT.into(),
            agent_id: "agent-a".into(),
            epoch: "epoch-1".into(),
            sequence,
            message: (kind != ToolMessageKind::Reset).then(|| message(kind, id)),
        }
    }

    #[test]
    fn canonical_call_result_and_replay_succeed() {
        let mut relay = ClientSideToolV2Relay::default();
        let call = relay.accept(event(1, ToolMessageKind::Call, "call-1")).unwrap();
        assert_eq!(call.message_type.as_deref(), Some("aiserver.v1.ClientSideToolV2Call"));
        let result = relay.accept(event(2, ToolMessageKind::Result, "call-1")).unwrap();
        assert_eq!(result.sequence, 2);
        assert_eq!(relay.replay().iter().map(|item| item.sequence).collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn malformed_wire_version_account_base64_type_and_identity_fail_closed() {
        let mut relay = ClientSideToolV2Relay::default();

        let mut wrong_version = event(1, ToolMessageKind::Call, "call-1");
        wrong_version.version = 2;
        assert!(relay.accept(wrong_version).is_none());

        let mut wrong_slot = event(1, ToolMessageKind::Call, "call-1");
        wrong_slot.account_slot = "account".into();
        assert!(relay.accept(wrong_slot).is_none());

        let mut noncanonical = event(1, ToolMessageKind::Call, "call-1");
        noncanonical.message.as_mut().unwrap().bytes = "GgZjYWxsLTE".into();
        assert!(relay.accept(noncanonical).is_none());

        let mut wrong_type = event(1, ToolMessageKind::Call, "call-1");
        wrong_type.message.as_mut().unwrap().message_type = "aiserver.v1.ClientSideToolV2Result".into();
        assert!(relay.accept(wrong_type).is_none());

        let mut empty_agent = event(1, ToolMessageKind::Call, "call-1");
        empty_agent.agent_id.clear();
        assert!(relay.accept(empty_agent).is_none());

        let mut empty_epoch = event(1, ToolMessageKind::Call, "call-1");
        empty_epoch.epoch.clear();
        assert!(relay.accept(empty_epoch).is_none());

        let mut empty_tool_call = event(1, ToolMessageKind::Call, "");
        empty_tool_call.message = Some(message(ToolMessageKind::Call, ""));
        assert!(relay.accept(empty_tool_call).is_none());

        let raw = json!({
            "version": 1,
            "kind": "call",
            "accountSlot": "host",
            "agentId": "agent-a",
            "epoch": "epoch-1",
            "sequence": 1,
            "message": {"encoding":"json","messageType":"aiserver.v1.ClientSideToolV2Call","bytes":"e30="}
        });
        assert!(relay.accept_value(raw).is_none());

        let unknown_top_level = json!({
            "version": 1,
            "kind": "call",
            "accountSlot": "host",
            "agentId": "agent-a",
            "epoch": "epoch-1",
            "sequence": 1,
            "unexpected": true,
            "message": {
                "encoding": "protobuf-base64",
                "messageType": "aiserver.v1.ClientSideToolV2Call",
                "bytes": "GgZjYWxsLTE="
            }
        });
        assert!(relay.accept_value(unknown_top_level).is_none());

        let unknown_message_field = json!({
            "version": 1,
            "kind": "call",
            "accountSlot": "host",
            "agentId": "agent-a",
            "epoch": "epoch-1",
            "sequence": 1,
            "message": {
                "encoding": "protobuf-base64",
                "messageType": "aiserver.v1.ClientSideToolV2Call",
                "bytes": "GgZjYWxsLTE=",
                "unexpected": true
            }
        });
        assert!(relay.accept_value(unknown_message_field).is_none());
    }

    #[test]
    fn duplicate_out_of_order_inconsistent_result_and_stale_epoch_are_rejected() {
        let mut relay = ClientSideToolV2Relay::default();
        assert!(relay.accept(event(1, ToolMessageKind::Call, "call-1")).is_some());
        assert!(relay.accept(event(1, ToolMessageKind::Call, "call-2")).is_none());
        assert!(relay.accept(event(2, ToolMessageKind::Call, "call-1")).is_none());
        assert!(relay.accept(event(2, ToolMessageKind::Result, "other-call")).is_none());
        assert!(relay.accept(event(2, ToolMessageKind::Result, "call-1")).is_some());
        assert!(relay.accept(event(3, ToolMessageKind::Result, "call-1")).is_none());

        let mut next_epoch = event(1, ToolMessageKind::Call, "call-2");
        next_epoch.epoch = "epoch-2".into();
        assert!(relay.accept(next_epoch).is_some());

        let mut retired = event(4, ToolMessageKind::Call, "call-old");
        retired.epoch = "epoch-1".into();
        assert!(relay.accept(retired).is_none());

        let mut stale_result = event(2, ToolMessageKind::Result, "call-2");
        stale_result.epoch = "epoch-1".into();
        assert!(relay.accept(stale_result).is_none());
    }

    #[test]
    fn reset_call_result_account_switch_and_restart_replay_are_fenced() {
        let mut relay = ClientSideToolV2Relay::default();
        assert!(relay.accept(event(1, ToolMessageKind::Call, "call-1")).is_some());

        let reset = ToolTransportEvent {
            message: None,
            ..event(2, ToolMessageKind::Reset, "")
        };
        assert!(relay.accept(reset).is_some());
        assert!(relay.accept(event(3, ToolMessageKind::Result, "call-1")).is_none());
        assert!(relay.accept(event(3, ToolMessageKind::Call, "call-2")).is_some());
        assert!(relay.accept(event(4, ToolMessageKind::Result, "call-2")).is_some());

        relay.retire_for_account_switch();
        assert!(relay.replay().is_empty());
        assert!(relay.accept(event(5, ToolMessageKind::Call, "stale-account-call")).is_none());

        let mut new_account_call = event(1, ToolMessageKind::Call, "call-new");
        new_account_call.epoch = "epoch-account-2".into();
        assert!(relay.accept(new_account_call.clone()).is_some());

        let mut restarted = ClientSideToolV2Relay::default();
        let mut result = event(2, ToolMessageKind::Result, "call-new");
        result.epoch = "epoch-account-2".into();
        assert!(restarted.accept(result.clone()).is_none());
        assert!(restarted.accept(new_account_call).is_some());
        assert!(restarted.accept(result).is_some());
        assert_eq!(restarted.replay().len(), 2);
    }
}
