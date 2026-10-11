use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const CLIENT_SIDE_TOOL_V2_FAMILY: &str = "client-side-tool-v2";
pub const CLIENT_SIDE_TOOL_V2_WIRE_VERSION: u32 = 1;
pub const CLIENT_SIDE_TOOL_V2_ACCOUNT_SLOT: &str = "host";

const TOOL_EDIT_FILE: u64 = 7;
const TOOL_RUN_TERMINAL_COMMAND_V2: u64 = 15;
const TOOL_EDIT_FILE_V2: u64 = 38;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolResultCardKind {
    FileEdit,
    FileWrite,
    Shell,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolResultCardStatus {
    Running,
    Success,
    Error,
    Denied,
    Rejected,
    Cancelled,
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultCardSnapshot {
    pub kind: ToolResultCardKind,
    pub tool_call_id: String,
    pub status: ToolResultCardStatus,
    pub path: Option<String>,
    pub command: Option<String>,
    pub working_directory: Option<String>,
    pub summary: String,
    pub output: String,
    pub diff: String,
    pub is_streaming: bool,
    pub is_background: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection: Option<ToolResultCardSnapshot>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AgentFence {
    epoch: String,
    sequence: u64,
    retired_epochs: BTreeSet<String>,
    updates_by_tool_call_id: BTreeMap<String, Vec<ToolTransportEvent>>,
    projections_by_tool_call_id: BTreeMap<String, ToolResultCardSnapshot>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientSideToolV2Relay {
    agents: BTreeMap<String, AgentFence>,
}

#[derive(Clone, Debug)]
enum ProtoValue {
    Varint(u64),
    Bytes(Vec<u8>),
    Fixed32,
    Fixed64,
}

fn read_varint(bytes: &[u8], cursor: &mut usize) -> Option<u64> {
    let mut value = 0_u64;
    for index in 0..10_u32 {
        if *cursor >= bytes.len() {
            return None;
        }
        let byte = bytes[*cursor];
        *cursor += 1;
        if index == 9 && byte > 1 {
            return None;
        }
        value |= u64::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            if index > 0 && byte == 0 {
                return None;
            }
            return Some(value);
        }
    }
    None
}

fn parse_fields(bytes: &[u8]) -> Option<Vec<(u32, ProtoValue)>> {
    let mut cursor = 0;
    let mut fields = Vec::new();
    while cursor < bytes.len() {
        let key = read_varint(bytes, &mut cursor)?;
        let field = (key >> 3) as u32;
        if field == 0 {
            return None;
        }
        match (key & 0x07) as u8 {
            0 => fields.push((field, ProtoValue::Varint(read_varint(bytes, &mut cursor)?))),
            1 => {
                cursor = cursor.checked_add(8)?;
                if cursor > bytes.len() {
                    return None;
                }
                fields.push((field, ProtoValue::Fixed64));
            }
            2 => {
                let len = usize::try_from(read_varint(bytes, &mut cursor)?).ok()?;
                let end = cursor.checked_add(len)?;
                if end > bytes.len() {
                    return None;
                }
                fields.push((field, ProtoValue::Bytes(bytes[cursor..end].to_vec())));
                cursor = end;
            }
            5 => {
                cursor = cursor.checked_add(4)?;
                if cursor > bytes.len() {
                    return None;
                }
                fields.push((field, ProtoValue::Fixed32));
            }
            _ => return None,
        }
    }
    Some(fields)
}

fn unique_bytes(fields: &[(u32, ProtoValue)], wanted: u32) -> Option<Vec<u8>> {
    let mut found = None;
    for (field, value) in fields {
        if *field != wanted {
            continue;
        }
        let ProtoValue::Bytes(bytes) = value else {
            return None;
        };
        if found.is_some() {
            return None;
        }
        found = Some(bytes.clone());
    }
    found
}

fn first_bytes(fields: &[(u32, ProtoValue)], wanted: u32) -> Option<Vec<u8>> {
    fields.iter().find_map(|(field, value)| {
        (*field == wanted)
            .then(|| match value {
                ProtoValue::Bytes(bytes) => Some(bytes.clone()),
                _ => None,
            })
            .flatten()
    })
}

fn first_string(fields: &[(u32, ProtoValue)], wanted: u32) -> Option<String> {
    let bytes = first_bytes(fields, wanted)?;
    String::from_utf8(bytes).ok()
}

fn first_varint(fields: &[(u32, ProtoValue)], wanted: u32) -> Option<u64> {
    fields.iter().find_map(|(field, value)| {
        (*field == wanted)
            .then(|| match value {
                ProtoValue::Varint(value) => Some(*value),
                _ => None,
            })
            .flatten()
    })
}

fn has_field(fields: &[(u32, ProtoValue)], wanted: u32) -> bool {
    fields.iter().any(|(field, _)| *field == wanted)
}

fn normalize_text(value: String) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

fn decode_message(
    kind: ToolMessageKind,
    message: &EncodedToolMessage,
) -> Option<(Vec<u8>, String)> {
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
    let fields = parse_fields(&bytes)?;
    let tool_call_field = match kind {
        ToolMessageKind::Call => 3,
        ToolMessageKind::Result => 35,
        ToolMessageKind::Reset => unreachable!(),
    };
    let tool_call_bytes = unique_bytes(&fields, tool_call_field)?;
    let tool_call_id = String::from_utf8(tool_call_bytes).ok()?;
    if tool_call_id.trim().is_empty() || tool_call_id.chars().any(char::is_control) {
        return None;
    }
    Some((bytes, tool_call_id))
}

fn supported_tool(fields: &[(u32, ProtoValue)]) -> Option<u64> {
    let tool = first_varint(fields, 1)?;
    matches!(tool, TOOL_EDIT_FILE | TOOL_RUN_TERMINAL_COMMAND_V2 | TOOL_EDIT_FILE_V2)
        .then_some(tool)
}

fn call_projection(bytes: &[u8], tool_call_id: &str) -> Option<ToolResultCardSnapshot> {
    let fields = parse_fields(bytes)?;
    let tool = supported_tool(&fields)?;
    let is_streaming = first_varint(&fields, 14).unwrap_or(0) != 0;
    match tool {
        TOOL_EDIT_FILE => {
            let params = parse_fields(&first_bytes(&fields, 13)?)?;
            let path = first_string(&params, 1)?.trim().to_string();
            if path.is_empty() {
                return None;
            }
            Some(ToolResultCardSnapshot {
                kind: ToolResultCardKind::FileEdit,
                tool_call_id: tool_call_id.to_string(),
                status: ToolResultCardStatus::Running,
                path: Some(path),
                command: None,
                working_directory: None,
                summary: String::new(),
                output: String::new(),
                diff: String::new(),
                is_streaming: true,
                is_background: false,
            })
        }
        TOOL_EDIT_FILE_V2 => {
            let params = parse_fields(&first_bytes(&fields, 50)?)?;
            let path = first_string(&params, 1)?.trim().to_string();
            if path.is_empty() {
                return None;
            }
            let output = normalize_text(first_string(&params, 9).unwrap_or_default());
            Some(ToolResultCardSnapshot {
                kind: ToolResultCardKind::FileEdit,
                tool_call_id: tool_call_id.to_string(),
                status: ToolResultCardStatus::Running,
                path: Some(path),
                command: None,
                working_directory: None,
                summary: String::new(),
                output,
                diff: String::new(),
                is_streaming: is_streaming || has_field(&params, 9),
                is_background: false,
            })
        }
        TOOL_RUN_TERMINAL_COMMAND_V2 => {
            let params = parse_fields(&first_bytes(&fields, 23)?)?;
            let command = first_string(&params, 1)?.trim().to_string();
            if command.is_empty() {
                return None;
            }
            Some(ToolResultCardSnapshot {
                kind: ToolResultCardKind::Shell,
                tool_call_id: tool_call_id.to_string(),
                status: ToolResultCardStatus::Running,
                path: None,
                command: Some(command),
                working_directory: first_string(&params, 2).filter(|value| !value.trim().is_empty()),
                summary: String::new(),
                output: String::new(),
                diff: String::new(),
                is_streaming: true,
                is_background: first_varint(&params, 5).unwrap_or(0) != 0,
            })
        }
        _ => None,
    }
}

fn diff_text(bytes: &[u8]) -> String {
    let Some(fields) = parse_fields(bytes) else {
        return String::new();
    };
    fields
        .iter()
        .filter_map(|(field, value)| {
            if *field != 1 {
                return None;
            }
            let ProtoValue::Bytes(chunk) = value else {
                return None;
            };
            let chunk_fields = parse_fields(chunk)?;
            first_string(&chunk_fields, 1)
        })
        .map(normalize_text)
        .collect::<Vec<_>>()
        .join("")
}

fn result_projection(
    bytes: &[u8],
    previous: &ToolResultCardSnapshot,
) -> Option<ToolResultCardSnapshot> {
    let fields = parse_fields(bytes)?;
    let tool = supported_tool(&fields)?;
    let tool_call_id = first_string(&fields, 35)?;
    if tool_call_id != previous.tool_call_id {
        return None;
    }

    if let Some(error_bytes) = first_bytes(&fields, 8) {
        let error_fields = parse_fields(&error_bytes)?;
        let summary = first_string(&error_fields, 1).unwrap_or_default();
        let mut next = previous.clone();
        next.status = if summary.starts_with("Permission denied:") {
            ToolResultCardStatus::Denied
        } else {
            ToolResultCardStatus::Error
        };
        next.summary = summary;
        next.is_streaming = false;
        return Some(next);
    }

    let mut next = previous.clone();
    match tool {
        TOOL_EDIT_FILE => {
            if next.kind != ToolResultCardKind::FileEdit {
                return None;
            }
            let result = parse_fields(&first_bytes(&fields, 10)?)?;
            if first_varint(&result, 5).unwrap_or(0) != 0 {
                next.status = ToolResultCardStatus::Rejected;
            } else if first_varint(&result, 3).unwrap_or(0) != 0 || has_field(&result, 11) {
                next.status = ToolResultCardStatus::Error;
            } else if first_varint(&result, 2).unwrap_or(0) != 0 {
                next.status = ToolResultCardStatus::Success;
            } else {
                return None;
            }
            if let Some(diff) = first_bytes(&result, 1) {
                next.diff = diff_text(&diff);
            }
        }
        TOOL_EDIT_FILE_V2 => {
            if !matches!(next.kind, ToolResultCardKind::FileEdit | ToolResultCardKind::FileWrite) {
                return None;
            }
            let result = parse_fields(&first_bytes(&fields, 51)?)?;
            next.kind = if first_varint(&result, 2).unwrap_or(0) != 0 {
                ToolResultCardKind::FileWrite
            } else {
                ToolResultCardKind::FileEdit
            };
            next.status = if first_varint(&result, 4).unwrap_or(0) != 0 {
                ToolResultCardStatus::Rejected
            } else {
                ToolResultCardStatus::Success
            };
            if let Some(diff) = first_bytes(&result, 3) {
                next.diff = diff_text(&diff);
            }
            if next.summary.is_empty() {
                next.summary = first_string(&result, 10).unwrap_or_default();
            }
        }
        TOOL_RUN_TERMINAL_COMMAND_V2 => {
            if next.kind != ToolResultCardKind::Shell {
                return None;
            }
            let result = parse_fields(&first_bytes(&fields, 24)?)?;
            let raw = first_string(&result, 12).filter(|value| !value.is_empty());
            next.output = normalize_text(raw.or_else(|| first_string(&result, 1)).unwrap_or_default());
            if first_varint(&result, 3).unwrap_or(0) != 0 {
                next.status = ToolResultCardStatus::Rejected;
            } else if first_varint(&result, 4).unwrap_or(0) != 0
                || first_varint(&result, 5).unwrap_or(0) != 0
            {
                next.status = ToolResultCardStatus::Background;
                next.is_background = true;
            } else {
                next.status = match first_varint(&result, 9).unwrap_or(0) {
                    1 => ToolResultCardStatus::Success,
                    2 => ToolResultCardStatus::Cancelled,
                    3 | 4 | 5 => ToolResultCardStatus::Error,
                    _ => return None,
                };
            }
        }
        _ => return None,
    }
    next.is_streaming = false;
    Some(next)
}

fn merge_streaming_call(
    previous: &ToolResultCardSnapshot,
    update: ToolResultCardSnapshot,
) -> Option<ToolResultCardSnapshot> {
    if previous.tool_call_id != update.tool_call_id || previous.kind != update.kind {
        return None;
    }
    if !matches!(previous.status, ToolResultCardStatus::Running) {
        return None;
    }
    let mut merged = update;
    if !merged.output.is_empty() {
        merged.output = format!("{}{}", previous.output, merged.output);
    } else {
        merged.output = previous.output.clone();
    }
    if merged.diff.is_empty() {
        merged.diff = previous.diff.clone();
    }
    if merged.path.is_none() {
        merged.path = previous.path.clone();
    }
    if merged.command.is_none() {
        merged.command = previous.command.clone();
    }
    if merged.working_directory.is_none() {
        merged.working_directory = previous.working_directory.clone();
    }
    Some(merged)
}

fn materialize(
    event: &ToolTransportEvent,
    projection: Option<ToolResultCardSnapshot>,
) -> Option<RendererToolEvent> {
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
            projection: None,
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
        projection,
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

        let decoded = match event.kind {
            ToolMessageKind::Reset => {
                if event.message.is_some() {
                    return None;
                }
                None
            }
            ToolMessageKind::Call | ToolMessageKind::Result => {
                let (bytes, tool_call_id) = decode_message(event.kind, event.message.as_ref()?)?;
                Some((bytes, tool_call_id))
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
            fence.projections_by_tool_call_id.clear();
        }
        if event.sequence <= fence.sequence {
            return None;
        }

        let projection = match event.kind {
            ToolMessageKind::Reset => {
                fence.updates_by_tool_call_id.clear();
                fence.projections_by_tool_call_id.clear();
                None
            }
            ToolMessageKind::Call => {
                let (bytes, tool_call_id) = decoded?;
                let fields = parse_fields(&bytes)?;
                let tool = first_varint(&fields, 1);
                let supported = tool.is_some_and(|value| {
                    matches!(value, TOOL_EDIT_FILE | TOOL_RUN_TERMINAL_COMMAND_V2 | TOOL_EDIT_FILE_V2)
                });
                let call = if supported {
                    Some(call_projection(&bytes, &tool_call_id)?)
                } else {
                    None
                };
                if let Some(lifecycle) = fence.updates_by_tool_call_id.get(&tool_call_id) {
                    if lifecycle.iter().any(|row| row.kind == ToolMessageKind::Result) {
                        return None;
                    }
                    if call.is_none() {
                        return None;
                    }
                }
                let projection = match call {
                    Some(call) => {
                        let next = if let Some(previous) = fence.projections_by_tool_call_id.get(&tool_call_id) {
                            merge_streaming_call(previous, call)?
                        } else {
                            call
                        };
                        fence
                            .projections_by_tool_call_id
                            .insert(tool_call_id.clone(), next.clone());
                        Some(next)
                    }
                    None => None,
                };
                fence
                    .updates_by_tool_call_id
                    .entry(tool_call_id)
                    .or_default()
                    .push(event.clone());
                projection
            }
            ToolMessageKind::Result => {
                let (bytes, tool_call_id) = decoded?;
                let lifecycle = fence.updates_by_tool_call_id.get_mut(&tool_call_id)?;
                if lifecycle.is_empty()
                    || lifecycle.iter().any(|row| row.kind == ToolMessageKind::Result)
                {
                    return None;
                }
                let projection = if let Some(previous) =
                    fence.projections_by_tool_call_id.get(&tool_call_id).cloned()
                {
                    let next = result_projection(&bytes, &previous)?;
                    fence
                        .projections_by_tool_call_id
                        .insert(tool_call_id.clone(), next.clone());
                    Some(next)
                } else {
                    None
                };
                lifecycle.push(event.clone());
                projection
            }
        };

        fence.sequence = event.sequence;
        materialize(&event, projection)
    }

    pub fn replay(&self) -> Vec<RendererToolEvent> {
        let mut events = self
            .agents
            .values()
            .flat_map(|fence| fence.updates_by_tool_call_id.values())
            .flat_map(|updates| updates.iter().cloned())
            .collect::<Vec<_>>();
        events.sort_by_key(|event| event.sequence);

        let mut replay = ClientSideToolV2Relay::default();
        events
            .into_iter()
            .filter_map(|event| replay.accept(event))
            .collect()
    }

    pub fn snapshot(&self) -> Vec<ToolResultCardSnapshot> {
        let mut rows = self
            .agents
            .values()
            .flat_map(|fence| fence.projections_by_tool_call_id.values().cloned())
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| left.tool_call_id.cmp(&right.tool_call_id));
        rows
    }

    pub fn retire_for_account_switch(&mut self) {
        for fence in self.agents.values_mut() {
            if !fence.epoch.is_empty() {
                fence.retired_epochs.insert(std::mem::take(&mut fence.epoch));
            }
            fence.sequence = 0;
            fence.updates_by_tool_call_id.clear();
            fence.projections_by_tool_call_id.clear();
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

    fn field_varint(field: u32, value: u64) -> Vec<u8> {
        let mut out = varint(u64::from(field << 3));
        out.extend(varint(value));
        out
    }

    fn field_bytes(field: u32, value: &[u8]) -> Vec<u8> {
        let mut out = varint(u64::from((field << 3) | 2));
        out.extend(varint(value.len() as u64));
        out.extend(value);
        out
    }

    fn field_string(field: u32, value: &str) -> Vec<u8> {
        field_bytes(field, value.as_bytes())
    }

    fn bare_message(kind: ToolMessageKind, id: &str) -> EncodedToolMessage {
        let (message_type, field) = match kind {
            ToolMessageKind::Call => ("aiserver.v1.ClientSideToolV2Call", 3),
            ToolMessageKind::Result => ("aiserver.v1.ClientSideToolV2Result", 35),
            ToolMessageKind::Reset => panic!("reset has no message"),
        };
        EncodedToolMessage {
            encoding: "protobuf-base64".into(),
            message_type: message_type.into(),
            bytes: STANDARD.encode(field_string(field, id)),
        }
    }

    fn encoded(kind: ToolMessageKind, bytes: Vec<u8>) -> EncodedToolMessage {
        EncodedToolMessage {
            encoding: "protobuf-base64".into(),
            message_type: match kind {
                ToolMessageKind::Call => "aiserver.v1.ClientSideToolV2Call",
                ToolMessageKind::Result => "aiserver.v1.ClientSideToolV2Result",
                ToolMessageKind::Reset => panic!("reset has no message"),
            }
            .into(),
            bytes: STANDARD.encode(bytes),
        }
    }

    fn event_with_message(
        sequence: u64,
        kind: ToolMessageKind,
        message: EncodedToolMessage,
    ) -> ToolTransportEvent {
        ToolTransportEvent {
            version: CLIENT_SIDE_TOOL_V2_WIRE_VERSION,
            kind,
            account_slot: CLIENT_SIDE_TOOL_V2_ACCOUNT_SLOT.into(),
            agent_id: "agent-a".into(),
            epoch: "epoch-1".into(),
            sequence,
            message: Some(message),
        }
    }

    fn shell_call(sequence: u64, id: &str, command: &str) -> ToolTransportEvent {
        let mut params = field_string(1, command);
        params.extend(field_string(2, "/work"));
        let mut bytes = field_varint(1, TOOL_RUN_TERMINAL_COMMAND_V2);
        bytes.extend(field_string(3, id));
        bytes.extend(field_bytes(23, &params));
        event_with_message(sequence, ToolMessageKind::Call, encoded(ToolMessageKind::Call, bytes))
    }

    fn shell_result(sequence: u64, id: &str, output: &str, ended_reason: u64) -> ToolTransportEvent {
        let mut result = field_string(12, output);
        result.extend(field_varint(9, ended_reason));
        let mut bytes = field_varint(1, TOOL_RUN_TERMINAL_COMMAND_V2);
        bytes.extend(field_string(35, id));
        bytes.extend(field_bytes(24, &result));
        event_with_message(sequence, ToolMessageKind::Result, encoded(ToolMessageKind::Result, bytes))
    }

    fn edit_v2_call(sequence: u64, id: &str, delta: &str) -> ToolTransportEvent {
        let mut params = field_string(1, "src/main.rs");
        params.extend(field_string(9, delta));
        let mut bytes = field_varint(1, TOOL_EDIT_FILE_V2);
        bytes.extend(field_string(3, id));
        bytes.extend(field_varint(14, 1));
        bytes.extend(field_bytes(50, &params));
        event_with_message(sequence, ToolMessageKind::Call, encoded(ToolMessageKind::Call, bytes))
    }

    #[test]
    fn canonical_shell_call_result_and_replay_project_card() {
        let mut relay = ClientSideToolV2Relay::default();
        let call = relay.accept(shell_call(1, "call-1", "printf hi")).unwrap();
        let card = call.projection.unwrap();
        assert_eq!(card.kind, ToolResultCardKind::Shell);
        assert_eq!(card.status, ToolResultCardStatus::Running);
        assert_eq!(card.command.as_deref(), Some("printf hi"));

        let result = relay
            .accept(shell_result(2, "call-1", "one\r\ntwo\r", 1))
            .unwrap();
        let card = result.projection.unwrap();
        assert_eq!(card.status, ToolResultCardStatus::Success);
        assert_eq!(card.output, "one\ntwo\n");
        let replay = relay.replay();
        assert_eq!(replay.len(), 2);
        assert_eq!(
            replay.last().unwrap().projection.as_ref().unwrap().status,
            ToolResultCardStatus::Success
        );
    }

    #[test]
    fn streaming_edit_call_merges_same_tool_call_id_by_sequence() {
        let mut relay = ClientSideToolV2Relay::default();
        relay.accept(edit_v2_call(1, "edit-1", "a\r\n")).unwrap();
        let second = relay.accept(edit_v2_call(2, "edit-1", "b\r")).unwrap();
        let card = second.projection.unwrap();
        assert_eq!(card.output, "a\nb\n");
        let snapshot = relay.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].output, "a\nb\n");
    }

    #[test]
    fn malformed_wire_version_account_base64_type_and_identity_fail_closed() {
        let mut relay = ClientSideToolV2Relay::default();

        let mut wrong_version = shell_call(1, "call-1", "echo one");
        wrong_version.version = 2;
        assert!(relay.accept(wrong_version).is_none());

        let mut wrong_slot = shell_call(1, "call-1", "echo one");
        wrong_slot.account_slot = "account".into();
        assert!(relay.accept(wrong_slot).is_none());

        let mut noncanonical = shell_call(1, "call-1", "echo one");
        noncanonical.message.as_mut().unwrap().bytes.pop();
        assert!(relay.accept(noncanonical).is_none());

        let mut wrong_type = shell_call(1, "call-1", "echo one");
        wrong_type.message.as_mut().unwrap().message_type =
            "aiserver.v1.ClientSideToolV2Result".into();
        assert!(relay.accept(wrong_type).is_none());

        let mut empty_agent = shell_call(1, "call-1", "echo one");
        empty_agent.agent_id.clear();
        assert!(relay.accept(empty_agent).is_none());

        let mut empty_epoch = shell_call(1, "call-1", "echo one");
        empty_epoch.epoch.clear();
        assert!(relay.accept(empty_epoch).is_none());

        let raw = json!({
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
        assert!(relay.accept_value(raw).is_none());
    }

    #[test]
    fn duplicate_out_of_order_inconsistent_result_and_stale_epoch_are_rejected() {
        let mut relay = ClientSideToolV2Relay::default();
        assert!(relay.accept(shell_call(1, "call-1", "echo one")).is_some());
        assert!(relay.accept(shell_call(1, "call-2", "echo two")).is_none());
        assert!(relay.accept(shell_result(2, "other-call", "no", 1)).is_none());
        assert!(relay.accept(shell_result(2, "call-1", "ok", 1)).is_some());
        assert!(relay.accept(shell_result(3, "call-1", "again", 1)).is_none());

        let mut next_epoch = shell_call(1, "call-2", "echo two");
        next_epoch.epoch = "epoch-2".into();
        assert!(relay.accept(next_epoch).is_some());

        let mut retired = shell_call(4, "call-old", "echo old");
        retired.epoch = "epoch-1".into();
        assert!(relay.accept(retired).is_none());
    }

    #[test]
    fn reset_account_switch_and_serialized_reopen_replay_are_fenced() {
        let mut relay = ClientSideToolV2Relay::default();
        assert!(relay.accept(shell_call(1, "call-1", "echo one")).is_some());
        let reset = ToolTransportEvent {
            version: 1,
            kind: ToolMessageKind::Reset,
            account_slot: "host".into(),
            agent_id: "agent-a".into(),
            epoch: "epoch-1".into(),
            sequence: 2,
            message: None,
        };
        assert!(relay.accept(reset).is_some());
        assert!(relay.accept(shell_result(3, "call-1", "late", 1)).is_none());
        assert!(relay.accept(shell_call(3, "call-2", "echo two")).is_some());
        assert!(relay.accept(shell_result(4, "call-2", "done", 1)).is_some());

        let encoded = serde_json::to_vec(&relay).unwrap();
        let reopened: ClientSideToolV2Relay = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(reopened.replay().len(), 2);
        assert_eq!(
            {
                let snapshot = reopened.snapshot();
                assert_eq!(snapshot.len(), 1);
                snapshot[0].status.clone()
            },
            ToolResultCardStatus::Success
        );

        let mut switched = reopened;
        switched.retire_for_account_switch();
        assert!(switched.replay().is_empty());
        let mut stale = shell_call(5, "stale-account-call", "echo stale");
        stale.epoch = "epoch-1".into();
        assert!(switched.accept(stale).is_none());
    }
}
