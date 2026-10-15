//! Frozen generated ConversationMessage -> no-preamble trace projection.
//!
//! Field numbers come from Desktop c33 generated aiserver.v1 chat/tools descriptors. Unknown
//! generated fields are skipped, so additive server schema changes do not become a second owner.

use serde_json::{json, Map, Number, Value};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ToolResult {
    tool_call_id: String,
    tool_name: String,
    args: String,
    raw_args: String,
    content: Option<String>,
    result: Option<RawToolResult>,
    started_at_ms: Option<u64>,
    completed_at_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawToolResult {
    result_type: &'static str,
    payload_hex: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ConversationMessage {
    text: String,
    message_type: i32,
    thinking: Option<String>,
    tool_results: Vec<ToolResult>,
}

pub fn decode_generated_conversation_trace(conversation: &[Vec<u8>]) -> Result<Vec<Value>, String> {
    let mut output = Vec::new();
    for bytes in conversation {
        let message = decode_conversation_message(bytes)?;
        let role = match message.message_type {
            1 => "user",
            2 => "assistant",
            _ => "unknown",
        };
        let mut trace = Map::new();
        trace.insert("role".into(), Value::String(role.into()));
        if !message.text.trim().is_empty() {
            trace.insert("text".into(), Value::String(message.text.clone()));
        }
        if let Some(thinking) = message
            .thinking
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            trace.insert("thinking".into(), Value::String(thinking.into()));
        }
        if message.message_type == 2 && !message.tool_results.is_empty() {
            trace.insert(
                "tool_calls".into(),
                Value::Array(message.tool_results.iter().map(project_tool_call).collect()),
            );
        }
        output.push(Value::Object(trace));
        if message.message_type == 2 {
            output.extend(
                message
                    .tool_results
                    .iter()
                    .map(|result| Value::Object(project_tool_result(result))),
            );
        }
    }
    Ok(output)
}

pub fn trace_to_jsonl(trace: &[Value]) -> Result<String, String> {
    let mut output = String::new();
    for value in trace {
        output.push_str(&serde_json::to_string(value).map_err(|error| error.to_string())?);
        output.push('\n');
    }
    Ok(output)
}

fn project_tool_call(result: &ToolResult) -> Value {
    let mut call = Map::new();
    apply_tool_identity_and_args(&mut call, result);
    apply_timestamps(&mut call, result);
    Value::Object(call)
}

fn project_tool_result(result: &ToolResult) -> Map<String, Value> {
    let mut tool = Map::new();
    tool.insert("role".into(), Value::String("tool".into()));
    apply_tool_identity_and_args(&mut tool, result);
    if let Some(content) = result.content.as_deref().filter(|value| !value.is_empty()) {
        tool.insert("tool_result".into(), parse_json_or_string(content));
    } else if let Some(raw) = &result.result {
        tool.insert(
            "tool_result".into(),
            json!({
                "resultType": raw.result_type,
                "value": {"protobufHex": raw.payload_hex},
            }),
        );
    }
    apply_timestamps(&mut tool, result);
    tool
}

fn apply_tool_identity_and_args(target: &mut Map<String, Value>, result: &ToolResult) {
    if !result.tool_call_id.is_empty() {
        target.insert(
            "tool_call_id".into(),
            Value::String(result.tool_call_id.clone()),
        );
    }
    if !result.tool_name.is_empty() {
        target.insert("tool_name".into(), Value::String(result.tool_name.clone()));
    }
    let args = if !result.raw_args.is_empty() {
        result.raw_args.as_str()
    } else {
        result.args.as_str()
    };
    if !args.is_empty() {
        target.insert("tool_args".into(), parse_json_or_string(args));
    }
}

fn apply_timestamps(target: &mut Map<String, Value>, result: &ToolResult) {
    if let Some(started) = result.started_at_ms {
        target.insert("started_at_ms".into(), Value::Number(Number::from(started)));
    }
    if let Some(completed) = result.completed_at_ms {
        target.insert(
            "completed_at_ms".into(),
            Value::Number(Number::from(completed)),
        );
    }
    if let (Some(started), Some(completed)) = (result.started_at_ms, result.completed_at_ms) {
        if completed >= started {
            target.insert(
                "duration_ms".into(),
                Value::Number(Number::from(completed - started)),
            );
        }
    }
}

fn parse_json_or_string(value: &str) -> Value {
    serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()))
}

fn decode_conversation_message(input: &[u8]) -> Result<ConversationMessage, String> {
    let mut cursor = 0usize;
    let mut output = ConversationMessage::default();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 7) as u8;
        match (field, wire) {
            (1, 2) => output.text = read_string(input, &mut cursor)?,
            (2, 0) => output.message_type = read_varint(input, &mut cursor)? as i32,
            (18, 2) => {
                let bytes = read_bytes(input, &mut cursor)?;
                output.tool_results.push(decode_tool_result(bytes)?);
            }
            (45, 2) => {
                let bytes = read_bytes(input, &mut cursor)?;
                output.thinking = decode_thinking(bytes)?;
            }
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    Ok(output)
}

fn decode_thinking(input: &[u8]) -> Result<Option<String>, String> {
    let mut cursor = 0usize;
    let mut text = None;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 7) as u8;
        if field == 1 && wire == 2 {
            text = Some(read_string(input, &mut cursor)?);
        } else {
            skip_field(input, &mut cursor, wire)?;
        }
    }
    Ok(text)
}

fn decode_tool_result(input: &[u8]) -> Result<ToolResult, String> {
    let mut cursor = 0usize;
    let mut output = ToolResult::default();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 7) as u8;
        match (field, wire) {
            (1, 2) => output.tool_call_id = read_string(input, &mut cursor)?,
            (2, 2) => output.tool_name = read_string(input, &mut cursor)?,
            (4, 2) => output.args = read_string(input, &mut cursor)?,
            (5, 2) => output.raw_args = read_string(input, &mut cursor)?,
            (7, 2) => output.content = Some(read_string(input, &mut cursor)?),
            (8, 2) => {
                let bytes = read_bytes(input, &mut cursor)?;
                output.result = decode_client_tool_result(bytes)?;
            }
            (13, 0) => output.started_at_ms = Some(read_varint(input, &mut cursor)?),
            (14, 0) => output.completed_at_ms = Some(read_varint(input, &mut cursor)?),
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    Ok(output)
}

fn decode_client_tool_result(input: &[u8]) -> Result<Option<RawToolResult>, String> {
    let mut cursor = 0usize;
    let mut result = None;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 7) as u8;
        if let Some(name) = client_tool_result_name(field) {
            if wire != 2 {
                return Err(format!(
                    "generated ClientSideToolV2Result field {field} has invalid wire {wire}"
                ));
            }
            let payload = read_bytes(input, &mut cursor)?;
            result = Some(RawToolResult {
                result_type: name,
                payload_hex: encode_hex(payload),
            });
        } else {
            skip_field(input, &mut cursor, wire)?;
        }
    }
    Ok(result)
}

fn client_tool_result_name(field: u64) -> Option<&'static str> {
    Some(match field {
        2 => "readSemsearchFilesResult",
        4 => "ripgrepSearchResult",
        6 => "readFileResult",
        9 => "listDirResult",
        10 => "editFileResult",
        11 => "fileSearchResult",
        18 => "semanticSearchFullResult",
        20 => "deleteFileResult",
        21 => "reapplyResult",
        24 => "runTerminalCommandV2Result",
        25 => "fetchRulesResult",
        27 => "webSearchResult",
        28 => "mcpResult",
        32 => "searchSymbolsResult",
        33 => "backgroundComposerFollowupResult",
        34 => "knowledgeBaseResult",
        36 => "fetchPullRequestResult",
        37 => "deepSearchResult",
        38 => "createDiagramResult",
        39 => "fixLintsResult",
        40 => "readLintsResult",
        41 => "gotodefResult",
        42 => "taskResult",
        43 => "awaitTaskResult",
        44 => "todoReadResult",
        45 => "todoWriteResult",
        51 => "editFileV2Result",
        52 => "listDirV2Result",
        53 => "readFileV2Result",
        54 => "ripgrepRawSearchResult",
        55 => "globFileSearchResult",
        56 => "createPlanResult",
        57 => "listMcpResourcesResult",
        58 => "readMcpResourceResult",
        59 => "readProjectResult",
        60 => "updateProjectResult",
        61 => "taskV2Result",
        62 => "callMcpToolResult",
        63 => "applyAgentDiffResult",
        64 => "askQuestionResult",
        65 => "switchModeResult",
        66 => "computerUseResult",
        67 => "generateImageResult",
        68 => "writeShellStdinResult",
        69 => "recordScreenResult",
        70 => "webFetchResult",
        71 => "reportBugfixResultsResult",
        72 => "aiAttributionResult",
        73 => "mcpAuthResult",
        74 => "getMcpToolsResult",
        75 => "connectScmResult",
        _ => return None,
    })
}

fn read_string(input: &[u8], cursor: &mut usize) -> Result<String, String> {
    String::from_utf8(read_bytes(input, cursor)?.to_vec())
        .map_err(|_| "generated CloudAgent string is not UTF-8".to_string())
}

fn read_bytes<'a>(input: &'a [u8], cursor: &mut usize) -> Result<&'a [u8], String> {
    let length = usize::try_from(read_varint(input, cursor)?)
        .map_err(|_| "generated CloudAgent length overflow".to_string())?;
    let end = cursor
        .checked_add(length)
        .ok_or_else(|| "generated CloudAgent length overflow".to_string())?;
    if end > input.len() {
        return Err("truncated generated CloudAgent protobuf".into());
    }
    let value = &input[*cursor..end];
    *cursor = end;
    Ok(value)
}

fn read_varint(input: &[u8], cursor: &mut usize) -> Result<u64, String> {
    let mut value = 0u64;
    let mut shift = 0u32;
    while *cursor < input.len() && shift < 64 {
        let byte = input[*cursor];
        *cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
    }
    Err("malformed generated CloudAgent varint".into())
}

fn skip_field(input: &[u8], cursor: &mut usize, wire: u8) -> Result<(), String> {
    match wire {
        0 => {
            let _ = read_varint(input, cursor)?;
        }
        1 => {
            *cursor = cursor
                .checked_add(8)
                .ok_or_else(|| "field overflow".to_string())?
        }
        2 => {
            let _ = read_bytes(input, cursor)?;
        }
        5 => {
            *cursor = cursor
                .checked_add(4)
                .ok_or_else(|| "field overflow".to_string())?
        }
        other => {
            return Err(format!(
                "unsupported generated CloudAgent wire type {other}"
            ))
        }
    }
    if *cursor > input.len() {
        return Err("truncated generated CloudAgent protobuf".into());
    }
    Ok(())
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn field_bytes(field: u64, bytes: &[u8]) -> Vec<u8> {
        let mut out = varint((field << 3) | 2);
        out.extend(varint(bytes.len() as u64));
        out.extend_from_slice(bytes);
        out
    }

    #[test]
    fn decodes_user_assistant_thinking_and_tool_projection() {
        let mut user = field_bytes(1, b"hello");
        user.extend(varint(2 << 3));
        user.extend(varint(1));

        let mut thinking = field_bytes(1, b"reason");
        let mut tool = field_bytes(1, b"call-1");
        tool.extend(field_bytes(2, b"shell"));
        tool.extend(field_bytes(5, br#"{"cmd":"pwd"}"#));
        tool.extend(field_bytes(7, br#"{"ok":true}"#));
        tool.extend(varint(13 << 3));
        tool.extend(varint(100));
        tool.extend(varint(14 << 3));
        tool.extend(varint(125));

        let mut assistant = field_bytes(1, b"done");
        assistant.extend(varint(2 << 3));
        assistant.extend(varint(2));
        assistant.extend(field_bytes(18, &tool));
        assistant.extend(field_bytes(45, &thinking));

        let trace = decode_generated_conversation_trace(&[user, assistant]).unwrap();
        assert_eq!(trace[0]["role"], "user");
        assert_eq!(trace[1]["thinking"], "reason");
        assert_eq!(trace[1]["tool_calls"][0]["tool_name"], "shell");
        assert_eq!(trace[2]["tool_result"]["ok"], true);
        assert_eq!(trace[2]["duration_ms"], 25);
    }

    #[test]
    fn additive_unknown_fields_are_skipped() {
        let mut message = field_bytes(1, b"ok");
        message.extend(varint((999 << 3) | 2));
        message.extend(varint(3));
        message.extend_from_slice(b"new");
        let trace = decode_generated_conversation_trace(&[message]).unwrap();
        assert_eq!(trace[0]["text"], "ok");
    }
}
