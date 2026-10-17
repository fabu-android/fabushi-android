use prost::{Message, Oneof};
use prost_types::{value::Kind, ListValue, Struct, Value as ProtoValue};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const CURSOR_INFERENCE_STREAM_PATH: &str = "/aiserver.v1.InferenceService/Stream";
pub const CONNECT_STREAM_CONTENT_TYPE: &str = "application/connect+proto";
pub const CONNECT_COMPRESSED_FLAG: u8 = 0x01;
pub const CONNECT_END_STREAM_FLAG: u8 = 0x02;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorRequestedModelParameter {
    pub id: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorRequestedModel {
    pub model_id: String,
    pub max_mode: bool,
    pub parameters: Vec<CursorRequestedModelParameter>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CursorInferenceToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorInferenceInputMessage {
    Text { role: String, text: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorInferenceEvent {
    TextDelta(String),
    ThinkingDelta { text: String, signature: Option<String> },
    ToolCall {
        tool_call_id: String,
        tool_name: String,
        args: String,
        complete: bool,
        tool_index: Option<i32>,
    },
    Usage {
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_write_tokens: u64,
    },
    Finished,
}

#[derive(Clone, PartialEq, Message)]
struct InferenceStreamRequest {
    #[prost(message, repeated, tag = "1")]
    messages: Vec<InferenceCoreMessage>,
    #[prost(message, repeated, tag = "2")]
    tools: Vec<InferenceAgentTool>,
    #[prost(message, optional, tag = "7")]
    requested_model: Option<InferenceRequestedModel>,
    #[prost(string, optional, tag = "8")]
    conversation_id: Option<String>,
    #[prost(int32, optional, tag = "11")]
    inference_reason: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
struct InferenceRequestedModel {
    #[prost(string, tag = "1")]
    model_id: String,
    #[prost(bool, tag = "2")]
    max_mode: bool,
    #[prost(message, repeated, tag = "3")]
    parameters: Vec<InferenceModelParameterValue>,
}

#[derive(Clone, PartialEq, Message)]
struct InferenceModelParameterValue {
    #[prost(string, tag = "1")]
    id: String,
    #[prost(string, tag = "2")]
    value: String,
}

#[derive(Clone, PartialEq, Message)]
struct InferenceCoreMessage {
    #[prost(int32, tag = "1")]
    role: i32,
    #[prost(oneof = "inference_core_message::Content", tags = "2")]
    content: Option<inference_core_message::Content>,
}

mod inference_core_message {
    use prost::Oneof;

    #[derive(Clone, PartialEq, Oneof)]
    pub enum Content {
        #[prost(string, tag = "2")]
        Text(String),
    }
}

#[derive(Clone, PartialEq, Message)]
struct InferenceAgentTool {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(string, tag = "2")]
    description: String,
    #[prost(message, optional, tag = "3")]
    parameters: Option<Struct>,
}

#[derive(Clone, PartialEq, Message)]
struct InferenceStreamResponse {
    #[prost(oneof = "inference_stream_response::Response", tags = "1, 2, 3, 5, 8, 9")]
    response: Option<inference_stream_response::Response>,
}

mod inference_stream_response {
    use super::{
        InferenceExtendedUsageInfo, InferenceStreamError, InferenceTextStreamPart,
        InferenceThinkingStreamPart, InferenceToolCallStreamPart, InferenceUsageInfo,
    };
    use prost::Oneof;

    #[derive(Clone, PartialEq, Oneof)]
    pub enum Response {
        #[prost(message, tag = "1")]
        TextPart(InferenceTextStreamPart),
        #[prost(message, tag = "2")]
        ToolCallPart(InferenceToolCallStreamPart),
        #[prost(message, tag = "3")]
        Usage(InferenceUsageInfo),
        #[prost(message, tag = "5")]
        ExtendedUsage(InferenceExtendedUsageInfo),
        #[prost(message, tag = "8")]
        Error(InferenceStreamError),
        #[prost(message, tag = "9")]
        ThinkingPart(InferenceThinkingStreamPart),
    }
}

#[derive(Clone, PartialEq, Message)]
struct InferenceTextStreamPart {
    #[prost(string, tag = "1")]
    text: String,
    #[prost(bool, tag = "2")]
    is_final: bool,
}

#[derive(Clone, PartialEq, Message)]
struct InferenceThinkingStreamPart {
    #[prost(string, tag = "1")]
    text: String,
    #[prost(string, optional, tag = "2")]
    signature: Option<String>,
    #[prost(bool, tag = "3")]
    is_final: bool,
}

#[derive(Clone, PartialEq, Message)]
struct InferenceToolCallStreamPart {
    #[prost(string, tag = "1")]
    tool_call_id: String,
    #[prost(string, tag = "2")]
    tool_name: String,
    #[prost(string, tag = "3")]
    args: String,
    #[prost(bool, tag = "4")]
    is_complete: bool,
    #[prost(int32, optional, tag = "5")]
    tool_index: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
struct InferenceUsageInfo {
    #[prost(int32, tag = "1")]
    prompt_tokens: i32,
    #[prost(int32, tag = "2")]
    completion_tokens: i32,
    #[prost(int32, optional, tag = "3")]
    total_tokens: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
struct InferenceExtendedUsageInfo {
    #[prost(int32, tag = "1")]
    input_tokens: i32,
    #[prost(int32, tag = "2")]
    output_tokens: i32,
    #[prost(int32, tag = "3")]
    cache_read_tokens: i32,
    #[prost(int32, tag = "4")]
    cache_write_tokens: i32,
    #[prost(int32, tag = "5")]
    max_tokens: i32,
}

#[derive(Clone, PartialEq, Message)]
struct InferenceStreamError {
    #[prost(string, tag = "1")]
    message: String,
    #[prost(string, tag = "2")]
    code: String,
    #[prost(bool, tag = "3")]
    is_input_token_limit_error: bool,
    #[prost(bool, tag = "4")]
    is_output_token_limit_error: bool,
    #[prost(int32, tag = "5")]
    error_type: i32,
}

pub fn encode_cursor_inference_request(
    messages: &[CursorInferenceInputMessage],
    tools: &[CursorInferenceToolDefinition],
    requested_model: &CursorRequestedModel,
    conversation_id: Option<&str>,
    inference_reason: Option<i32>,
) -> Result<Vec<u8>, String> {
    validate_requested_model(requested_model)?;
    let request = InferenceStreamRequest {
        messages: messages.iter().map(message_to_proto).collect(),
        tools: tools.iter().map(tool_to_proto).collect(),
        requested_model: Some(InferenceRequestedModel {
            model_id: requested_model.model_id.clone(),
            max_mode: requested_model.max_mode,
            parameters: requested_model
                .parameters
                .iter()
                .map(|parameter| InferenceModelParameterValue {
                    id: parameter.id.clone(),
                    value: parameter.value.clone(),
                })
                .collect(),
        }),
        conversation_id: conversation_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        inference_reason,
    };
    let mut payload = Vec::with_capacity(request.encoded_len());
    request
        .encode(&mut payload)
        .map_err(|error| format!("could not encode Cursor inference protobuf request: {error}"))?;
    Ok(encode_connect_envelope(&payload))
}

pub fn encode_connect_envelope(message: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(message.len() + 5);
    frame.push(0);
    frame.extend_from_slice(&(message.len() as u32).to_be_bytes());
    frame.extend_from_slice(message);
    frame
}

pub fn decode_cursor_connect_frames(buffer: &mut Vec<u8>) -> Result<Vec<CursorInferenceEvent>, String> {
    let mut events = Vec::new();
    let mut consumed = 0usize;
    while buffer.len().saturating_sub(consumed) >= 5 {
        let flags = buffer[consumed];
        let length = u32::from_be_bytes([
            buffer[consumed + 1],
            buffer[consumed + 2],
            buffer[consumed + 3],
            buffer[consumed + 4],
        ]) as usize;
        if buffer.len().saturating_sub(consumed + 5) < length {
            break;
        }
        let payload_start = consumed + 5;
        let payload_end = payload_start + length;
        let payload = &buffer[payload_start..payload_end];
        if flags & CONNECT_COMPRESSED_FLAG != 0 {
            return Err("Cursor inference returned compressed Connect envelope without negotiated encoding".into());
        }
        if flags & CONNECT_END_STREAM_FLAG != 0 {
            if !payload.is_empty() {
                let metadata: Value = serde_json::from_slice(payload)
                    .map_err(|error| format!("invalid Connect end-stream metadata: {error}"))?;
                if let Some(error) = metadata.get("error") {
                    return Err(format!("Cursor inference Connect stream ended with error: {error}"));
                }
            }
            consumed = payload_end;
            continue;
        }
        if flags != 0 {
            return Err(format!("unsupported Connect envelope flags: 0x{flags:02x}"));
        }
        let response = InferenceStreamResponse::decode(payload)
            .map_err(|error| format!("invalid Cursor inference protobuf response: {error}"))?;
        if let Some(event) = response_event(response)? {
            events.push(event);
        }
        consumed = payload_end;
    }
    if consumed > 0 {
        buffer.drain(..consumed);
    }
    Ok(events)
}

fn validate_requested_model(model: &CursorRequestedModel) -> Result<(), String> {
    let model_id = model.model_id.trim();
    if model_id.is_empty()
        || model_id.len() > 128
        || model_id.chars().any(|value| value.is_control() || value.is_whitespace())
    {
        return Err("Cursor inference model id is invalid".into());
    }
    if model.parameters.len() > 16 {
        return Err("Cursor inference model parameter count exceeds bound".into());
    }
    let mut seen = BTreeSet::new();
    for parameter in &model.parameters {
        if parameter.id.trim().is_empty()
            || parameter.id.len() > 128
            || parameter.value.len() > 256
            || !seen.insert(parameter.id.as_str())
        {
            return Err("Cursor inference model parameter is invalid".into());
        }
    }
    Ok(())
}

fn message_to_proto(message: &CursorInferenceInputMessage) -> InferenceCoreMessage {
    match message {
        CursorInferenceInputMessage::Text { role, text } => InferenceCoreMessage {
            role: role_number(role),
            content: Some(inference_core_message::Content::Text(text.clone())),
        },
    }
}

fn tool_to_proto(tool: &CursorInferenceToolDefinition) -> InferenceAgentTool {
    InferenceAgentTool {
        name: tool.name.clone(),
        description: tool.description.clone(),
        parameters: Some(json_object_to_proto(&tool.input_schema)),
    }
}

fn role_number(role: &str) -> i32 {
    match role {
        "user" => 1,
        "assistant" => 2,
        "tool" => 3,
        "system" => 4,
        _ => 0,
    }
}

fn response_event(response: InferenceStreamResponse) -> Result<Option<CursorInferenceEvent>, String> {
    use inference_stream_response::Response;
    match response.response {
        Some(Response::TextPart(part)) if part.is_final => Ok(Some(CursorInferenceEvent::Finished)),
        Some(Response::TextPart(part)) if !part.text.is_empty() => Ok(Some(CursorInferenceEvent::TextDelta(part.text))),
        Some(Response::ThinkingPart(part)) if !part.text.is_empty() || part.signature.is_some() => {
            Ok(Some(CursorInferenceEvent::ThinkingDelta { text: part.text, signature: part.signature }))
        }
        Some(Response::ToolCallPart(part)) => Ok(Some(CursorInferenceEvent::ToolCall {
            tool_call_id: part.tool_call_id,
            tool_name: part.tool_name,
            args: part.args,
            complete: part.is_complete,
            tool_index: part.tool_index,
        })),
        Some(Response::ExtendedUsage(usage)) => Ok(Some(CursorInferenceEvent::Usage {
            input_tokens: non_negative(usage.input_tokens),
            output_tokens: non_negative(usage.output_tokens),
            cache_read_tokens: non_negative(usage.cache_read_tokens),
            cache_write_tokens: non_negative(usage.cache_write_tokens),
        })),
        Some(Response::Usage(usage)) => Ok(Some(CursorInferenceEvent::Usage {
            input_tokens: non_negative(usage.prompt_tokens),
            output_tokens: non_negative(usage.completion_tokens),
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        })),
        Some(Response::Error(error)) => Err(format!(
            "Cursor inference stream failed code={} type={}: {}",
            error.code, error.error_type, error.message
        )),
        _ => Ok(None),
    }
}

fn non_negative(value: i32) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

fn json_to_proto(value: &Value) -> ProtoValue {
    let kind = match value {
        Value::Null => Kind::NullValue(0),
        Value::Bool(value) => Kind::BoolValue(*value),
        Value::Number(value) => Kind::NumberValue(value.as_f64().unwrap_or_default()),
        Value::String(value) => Kind::StringValue(value.clone()),
        Value::Array(values) => Kind::ListValue(ListValue {
            values: values.iter().map(json_to_proto).collect(),
        }),
        Value::Object(values) => Kind::StructValue(Struct {
            fields: values.iter().map(|(key, value)| (key.clone(), json_to_proto(value))).collect::<BTreeMap<_, _>>(),
        }),
    };
    ProtoValue { kind: Some(kind) }
}

fn json_object_to_proto(value: &Value) -> Struct {
    match json_to_proto(value).kind {
        Some(Kind::StructValue(value)) => value,
        _ => Struct::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_preserves_requested_model_max_mode_and_parameters() {
        let frame = encode_cursor_inference_request(
            &[CursorInferenceInputMessage::Text { role: "user".into(), text: "hello".into() }],
            &[CursorInferenceToolDefinition {
                name: "WebSearch".into(),
                description: "Search".into(),
                input_schema: serde_json::json!({"type":"object"}),
            }],
            &CursorRequestedModel {
                model_id: "claude-opus-4-8".into(),
                max_mode: true,
                parameters: vec![
                    CursorRequestedModelParameter { id: "thinking".into(), value: "true".into() },
                    CursorRequestedModelParameter { id: "effort".into(), value: "medium".into() },
                ],
            },
            Some("conversation-a"),
            Some(1),
        ).unwrap();
        assert_eq!(frame[0], 0);
        let size = u32::from_be_bytes([frame[1], frame[2], frame[3], frame[4]]) as usize;
        assert_eq!(size, frame.len() - 5);
        let decoded = InferenceStreamRequest::decode(&frame[5..]).unwrap();
        let model = decoded.requested_model.unwrap();
        assert_eq!(model.model_id, "claude-opus-4-8");
        assert!(model.max_mode);
        assert_eq!(model.parameters.len(), 2);
        assert_eq!(decoded.conversation_id.as_deref(), Some("conversation-a"));
    }

    #[test]
    fn decoder_handles_text_thinking_usage_and_finish() {
        let responses = vec![
            InferenceStreamResponse { response: Some(inference_stream_response::Response::TextPart(InferenceTextStreamPart { text: "hi".into(), is_final: false })) },
            InferenceStreamResponse { response: Some(inference_stream_response::Response::ThinkingPart(InferenceThinkingStreamPart { text: "reason".into(), signature: Some("sig".into()), is_final: false })) },
            InferenceStreamResponse { response: Some(inference_stream_response::Response::ExtendedUsage(InferenceExtendedUsageInfo { input_tokens: 10, output_tokens: 4, cache_read_tokens: 3, cache_write_tokens: 2, max_tokens: 100 })) },
            InferenceStreamResponse { response: Some(inference_stream_response::Response::TextPart(InferenceTextStreamPart { text: String::new(), is_final: true })) },
        ];
        let mut buffer = Vec::new();
        for response in responses {
            let mut payload = Vec::new();
            response.encode(&mut payload).unwrap();
            buffer.extend(encode_connect_envelope(&payload));
        }
        let events = decode_cursor_connect_frames(&mut buffer).unwrap();
        assert_eq!(events.len(), 4);
        assert!(matches!(&events[0], CursorInferenceEvent::TextDelta(text) if text == "hi"));
        assert!(matches!(&events[1], CursorInferenceEvent::ThinkingDelta { text, signature } if text == "reason" && signature.as_deref() == Some("sig")));
        assert!(matches!(events[2], CursorInferenceEvent::Usage { input_tokens: 10, output_tokens: 4, cache_read_tokens: 3, cache_write_tokens: 2 }));
        assert_eq!(events[3], CursorInferenceEvent::Finished);
        assert!(buffer.is_empty());
    }

    #[test]
    fn decoder_retains_partial_frame_and_rejects_end_stream_error() {
        let mut partial = vec![0, 0, 0, 0, 3, 1];
        assert!(decode_cursor_connect_frames(&mut partial).unwrap().is_empty());
        assert_eq!(partial, vec![0, 0, 0, 0, 3, 1]);
        let metadata = br#"{"error":{"code":"unavailable"}}"#;
        let mut end = Vec::with_capacity(metadata.len() + 5);
        end.push(CONNECT_END_STREAM_FLAG);
        end.extend_from_slice(&(metadata.len() as u32).to_be_bytes());
        end.extend_from_slice(metadata);
        assert!(decode_cursor_connect_frames(&mut end).unwrap_err().contains("unavailable"));
    }

    #[test]
    fn duplicate_model_parameters_are_rejected() {
        let error = encode_cursor_inference_request(
            &[],
            &[],
            &CursorRequestedModel {
                model_id: "model".into(),
                max_mode: false,
                parameters: vec![
                    CursorRequestedModelParameter { id: "effort".into(), value: "low".into() },
                    CursorRequestedModelParameter { id: "effort".into(), value: "high".into() },
                ],
            },
            None,
            None,
        ).unwrap_err();
        assert!(error.contains("parameter"));
    }
}
