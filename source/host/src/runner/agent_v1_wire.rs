//! Prost owner for canonical agent.v1 Runner tool-call observations.
//!
//! Field numbers and oneof tags are copied from the Desktop canonical generated
//! descriptors and must not be replaced with guessed JSON/tag mappings.

use prost::Message;

pub const FROZEN_AGENT_PB_BLOB_SHA: &str = "3cd3d020e69b54d1a6df27c0f967afe7e9fc942c";
pub const FROZEN_SEND_MESSAGE_PB_BLOB_SHA: &str = "fff9a91c8bf835d33b9d38b855c8785a9ebab03e";
pub const FROZEN_COMMUNICATE_UPDATE_PB_BLOB_SHA: &str = "20af9fc29e574015c5a40ab314e15002dd232cc5";

#[derive(Clone, PartialEq, Message)]
pub struct SendMessageText {
    #[prost(string, tag = "1")]
    pub content: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct SendMessageAttachment {
    #[prost(string, tag = "1")]
    pub url: String,
    #[prost(string, optional, tag = "2")]
    pub alt: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct SendMessageArgs {
    #[prost(oneof = "send_message_args::Message", tags = "1, 2")]
    pub message: Option<send_message_args::Message>,
}
pub mod send_message_args {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Message {
        #[prost(message, tag = "1")]
        Text(super::SendMessageText),
        #[prost(message, tag = "2")]
        Attachment(super::SendMessageAttachment),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct SendMessageSuccess {
    #[prost(uint64, tag = "1")]
    pub timestamp: u64,
    #[prost(string, tag = "2")]
    pub message_id: String,
}
#[derive(Clone, PartialEq, Message)]
pub struct SendMessageError {
    #[prost(string, tag = "1")]
    pub error: String,
}
#[derive(Clone, PartialEq, Message)]
pub struct SendMessageResult {
    #[prost(oneof = "send_message_result::Result", tags = "1, 2")]
    pub result: Option<send_message_result::Result>,
}
pub mod send_message_result {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Result {
        #[prost(message, tag = "1")]
        Success(super::SendMessageSuccess),
        #[prost(message, tag = "2")]
        Error(super::SendMessageError),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct SendMessageToolCall {
    #[prost(message, optional, tag = "1")]
    pub args: Option<SendMessageArgs>,
    #[prost(message, optional, tag = "2")]
    pub result: Option<SendMessageResult>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CommunicateUpdateArgs {
    #[prost(string, optional, tag = "1")]
    pub current_step: Option<String>,
    #[prost(string, optional, tag = "3")]
    pub final_summary: Option<String>,
    #[prost(string, optional, tag = "4")]
    pub completed_subtitle: Option<String>,
}
#[derive(Clone, PartialEq, Message)]
pub struct CommunicateUpdateSuccess {
    #[prost(string, tag = "1")]
    pub current_step: String,
    #[prost(uint32, tag = "3")]
    pub message_index: u32,
}
#[derive(Clone, PartialEq, Message)]
pub struct CommunicateUpdateError {
    #[prost(string, tag = "1")]
    pub error: String,
}
#[derive(Clone, PartialEq, Message)]
pub struct CommunicateUpdateResult {
    #[prost(oneof = "communicate_update_result::Result", tags = "1, 2")]
    pub result: Option<communicate_update_result::Result>,
}
pub mod communicate_update_result {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Result {
        #[prost(message, tag = "1")]
        Success(super::CommunicateUpdateSuccess),
        #[prost(message, tag = "2")]
        Error(super::CommunicateUpdateError),
    }
}
#[derive(Clone, PartialEq, Message)]
pub struct CommunicateUpdateToolCall {
    #[prost(message, optional, tag = "1")]
    pub args: Option<CommunicateUpdateArgs>,
    #[prost(message, optional, tag = "2")]
    pub result: Option<CommunicateUpdateResult>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AgentToolCall {
    #[prost(oneof = "agent_tool_call::Tool", tags = "48, 55")]
    pub tool: Option<agent_tool_call::Tool>,
    #[prost(string, optional, tag = "57")]
    pub tool_call_id: Option<String>,
    #[prost(uint64, optional, tag = "59")]
    pub started_at_ms: Option<u64>,
    #[prost(uint64, optional, tag = "60")]
    pub completed_at_ms: Option<u64>,
}
pub mod agent_tool_call {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Tool {
        #[prost(message, tag = "48")]
        CommunicateUpdateToolCall(super::CommunicateUpdateToolCall),
        #[prost(message, tag = "55")]
        SendMessageToolCall(super::SendMessageToolCall),
    }
}

pub fn encode_send_message_text_success(
    tool_call_id: &str,
    content: &str,
    started_at_ms: u64,
    completed_at_ms: u64,
    message_id: &str,
) -> Result<Vec<u8>, &'static str> {
    if tool_call_id.trim().is_empty() || message_id.trim().is_empty() {
        return Err("tool_call_id and message_id are required");
    }
    if completed_at_ms < started_at_ms {
        return Err("completed timestamp precedes started timestamp");
    }
    let call = AgentToolCall {
        tool: Some(agent_tool_call::Tool::SendMessageToolCall(SendMessageToolCall {
            args: Some(SendMessageArgs {
                message: Some(send_message_args::Message::Text(SendMessageText {
                    content: content.to_string(),
                })),
            }),
            result: Some(SendMessageResult {
                result: Some(send_message_result::Result::Success(SendMessageSuccess {
                    timestamp: completed_at_ms,
                    message_id: message_id.to_string(),
                })),
            }),
        })),
        tool_call_id: Some(tool_call_id.to_string()),
        started_at_ms: Some(started_at_ms),
        completed_at_ms: Some(completed_at_ms),
    };
    Ok(call.encode_to_vec())
}

pub fn decode_agent_tool_call(bytes: &[u8]) -> Result<AgentToolCall, prost::DecodeError> {
    AgentToolCall::decode(bytes)
}


fn required_string(value: Option<&serde_json::Value>, field: &'static str) -> Result<String, String> {
    value
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("{field} is required"))
}

fn send_message_args_from_json(args: &serde_json::Value) -> Result<SendMessageArgs, String> {
    let message = args.get("message").unwrap_or(args);
    let kind = message
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("text");
    let message = match kind {
        "text" => {
            let content = required_string(
                message.get("content").or_else(|| message.get("text")),
                "SendMessage text content",
            )?;
            send_message_args::Message::Text(SendMessageText { content })
        }
        "attachment" => {
            let url = required_string(message.get("url"), "SendMessage attachment url")?;
            let alt = message
                .get("alt")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned);
            send_message_args::Message::Attachment(SendMessageAttachment { url, alt })
        }
        _ => return Err("unsupported SendMessage canonical message type".into()),
    };
    Ok(SendMessageArgs { message: Some(message) })
}

fn communicate_update_args_from_json(
    args: &serde_json::Value,
) -> Result<CommunicateUpdateArgs, String> {
    let optional = |snake: &str, camel: &str| {
        args.get(snake)
            .or_else(|| args.get(camel))
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned)
    };
    let decoded = CommunicateUpdateArgs {
        current_step: optional("current_step", "currentStep"),
        final_summary: optional("final_summary", "finalSummary"),
        completed_subtitle: optional("completed_subtitle", "completedSubtitle"),
    };
    if decoded.current_step.is_none()
        && decoded.final_summary.is_none()
        && decoded.completed_subtitle.is_none()
    {
        return Err("CommunicateUpdate requires at least one canonical field".into());
    }
    Ok(decoded)
}

/// Encode the canonical agent.v1 wire observation for shipping Runner dispatches.
///
/// Unknown tools are outside this protobuf subset and return `Ok(None)`. The two
/// canonical tools owned here fail closed on malformed args/results instead of
/// falling back to JSON bytes.
pub fn encode_routed_tool_call_observation(
    name: &str,
    args: &serde_json::Value,
    result: Result<&serde_json::Value, &str>,
    tool_call_id: &str,
    started_at_ms: u64,
    completed_at_ms: u64,
) -> Result<Option<Vec<u8>>, String> {
    let tool_call_id = tool_call_id.trim();
    if tool_call_id.is_empty() {
        return Err("tool_call_id is required".into());
    }
    if completed_at_ms < started_at_ms {
        return Err("completed timestamp precedes started timestamp".into());
    }

    let tool = match name {
        "SendMessage" | "send_message" => {
            let args = send_message_args_from_json(args)?;
            let result = match result {
                Ok(value) => {
                    let message_id = required_string(
                        value.get("messageId").or_else(|| value.get("message_id")),
                        "SendMessage result message_id",
                    )?;
                    send_message_result::Result::Success(SendMessageSuccess {
                        timestamp: completed_at_ms,
                        message_id,
                    })
                }
                Err(error) => send_message_result::Result::Error(SendMessageError {
                    error: error.to_string(),
                }),
            };
            agent_tool_call::Tool::SendMessageToolCall(SendMessageToolCall {
                args: Some(args),
                result: Some(SendMessageResult {
                    result: Some(result),
                }),
            })
        }
        "CommunicateUpdate" | "communicate_update" => {
            let args = communicate_update_args_from_json(args)?;
            let result = match result {
                Ok(value) => {
                    let current_step = required_string(
                        value.get("currentStep").or_else(|| value.get("current_step")),
                        "CommunicateUpdate result current_step",
                    )?;
                    let message_index = value
                        .get("messageIndex")
                        .or_else(|| value.get("message_index"))
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|value| u32::try_from(value).ok())
                        .ok_or_else(|| {
                            "CommunicateUpdate result message_index is required".to_string()
                        })?;
                    communicate_update_result::Result::Success(CommunicateUpdateSuccess {
                        current_step,
                        message_index,
                    })
                }
                Err(error) => {
                    communicate_update_result::Result::Error(CommunicateUpdateError {
                        error: error.to_string(),
                    })
                }
            };
            agent_tool_call::Tool::CommunicateUpdateToolCall(CommunicateUpdateToolCall {
                args: Some(args),
                result: Some(CommunicateUpdateResult {
                    result: Some(result),
                }),
            })
        }
        _ => return Ok(None),
    };

    Ok(Some(
        AgentToolCall {
            tool: Some(tool),
            tool_call_id: Some(tool_call_id.to_string()),
            started_at_ms: Some(started_at_ms),
            completed_at_ms: Some(completed_at_ms),
        }
        .encode_to_vec(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_message_round_trip_preserves_canonical_tags_and_metadata() {
        let bytes = encode_send_message_text_success("call-7", "hello", 10, 12, "msg-7").unwrap();
        let decoded = decode_agent_tool_call(&bytes).unwrap();
        assert_eq!(decoded.tool_call_id.as_deref(), Some("call-7"));
        assert_eq!(decoded.started_at_ms, Some(10));
        assert_eq!(decoded.completed_at_ms, Some(12));
        match decoded.tool.unwrap() {
            agent_tool_call::Tool::SendMessageToolCall(call) => {
                match call.args.unwrap().message.unwrap() {
                    send_message_args::Message::Text(text) => assert_eq!(text.content, "hello"),
                    _ => panic!("wrong send-message oneof"),
                }
                match call.result.unwrap().result.unwrap() {
                    send_message_result::Result::Success(result) => {
                        assert_eq!(result.message_id, "msg-7");
                        assert_eq!(result.timestamp, 12);
                    }
                    _ => panic!("wrong send-message result oneof"),
                }
            }
            _ => panic!("wrong AgentToolCall tag"),
        }
    }

    #[test]
    fn communicate_round_trip_uses_tag_48_and_result_oneof() {
        let call = AgentToolCall {
            tool: Some(agent_tool_call::Tool::CommunicateUpdateToolCall(
                CommunicateUpdateToolCall {
                    args: Some(CommunicateUpdateArgs {
                        current_step: Some("step".into()),
                        final_summary: Some("done".into()),
                        completed_subtitle: None,
                    }),
                    result: Some(CommunicateUpdateResult {
                        result: Some(communicate_update_result::Result::Success(
                            CommunicateUpdateSuccess { current_step: "step".into(), message_index: 3 }
                        )),
                    }),
                },
            )),
            tool_call_id: Some("communicate-1".into()),
            started_at_ms: Some(20),
            completed_at_ms: Some(21),
        };
        let decoded = AgentToolCall::decode(call.encode_to_vec().as_slice()).unwrap();
        assert!(matches!(decoded.tool, Some(agent_tool_call::Tool::CommunicateUpdateToolCall(_))));
    }

    #[test]
    fn invalid_wire_and_timestamp_regressions_fail_closed() {
        assert!(decode_agent_tool_call(&[0x80]).is_err());
        assert!(encode_send_message_text_success("", "x", 1, 2, "m").is_err());
        assert!(encode_send_message_text_success("c", "x", 3, 2, "m").is_err());
    }

    #[test]
    fn shipping_observation_encoder_preserves_send_message_and_communicate_update_contracts() {
        let send_args = serde_json::json!({"message":{"type":"text","content":"hello"}});
        let send_result = serde_json::json!({"sent":true,"messageId":"msg-9"});
        let bytes = encode_routed_tool_call_observation(
            "SendMessage",
            &send_args,
            Ok(&send_result),
            "call-9",
            100,
            125,
        )
        .unwrap()
        .unwrap();
        assert_ne!(bytes.first().copied(), Some(b'{'));
        let decoded = decode_agent_tool_call(&bytes).unwrap();
        assert_eq!(decoded.tool_call_id.as_deref(), Some("call-9"));
        assert_eq!(decoded.started_at_ms, Some(100));
        assert_eq!(decoded.completed_at_ms, Some(125));
        assert!(matches!(
            decoded.tool,
            Some(agent_tool_call::Tool::SendMessageToolCall(_))
        ));

        let update_args = serde_json::json!({
            "currentStep":"working",
            "finalSummary":"done",
            "completedSubtitle":"complete"
        });
        let update_result = serde_json::json!({"currentStep":"working","messageIndex":7});
        let bytes = encode_routed_tool_call_observation(
            "CommunicateUpdate",
            &update_args,
            Ok(&update_result),
            "update-7",
            200,
            240,
        )
        .unwrap()
        .unwrap();
        let decoded = decode_agent_tool_call(&bytes).unwrap();
        assert_eq!(decoded.tool_call_id.as_deref(), Some("update-7"));
        assert!(matches!(
            decoded.tool,
            Some(agent_tool_call::Tool::CommunicateUpdateToolCall(_))
        ));
    }

    #[test]
    fn shipping_observation_encoder_fails_closed_without_json_fallback() {
        let bad_args = serde_json::json!({"message":{"type":"widget"}});
        let result = serde_json::json!({"messageId":"msg"});
        assert!(encode_routed_tool_call_observation(
            "SendMessage",
            &bad_args,
            Ok(&result),
            "call",
            1,
            2,
        )
        .is_err());
        assert!(encode_routed_tool_call_observation(
            "SendMessage",
            &serde_json::json!({"message":{"type":"text","content":"x"}}),
            Ok(&result),
            "",
            1,
            2,
        )
        .is_err());
        assert!(encode_routed_tool_call_observation(
            "SendMessage",
            &serde_json::json!({"message":{"type":"text","content":"x"}}),
            Ok(&result),
            "call",
            3,
            2,
        )
        .is_err());
        assert!(decode_agent_tool_call(&[0x80]).is_err());
        assert!(encode_routed_tool_call_observation(
            "OtherTool",
            &serde_json::json!({}),
            Ok(&serde_json::json!({})),
            "other",
            1,
            2,
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn descriptor_authority_is_pinned_and_json_fallback_is_not_part_of_wire_api() {
        assert_eq!(FROZEN_AGENT_PB_BLOB_SHA, "3cd3d020e69b54d1a6df27c0f967afe7e9fc942c");
        assert_eq!(FROZEN_SEND_MESSAGE_PB_BLOB_SHA, "fff9a91c8bf835d33b9d38b855c8785a9ebab03e");
        assert_eq!(FROZEN_COMMUNICATE_UPDATE_PB_BLOB_SHA, "20af9fc29e574015c5a40ab314e15002dd232cc5");
        let wire = encode_send_message_text_success("c", "{\"json\":false}", 1, 2, "m").unwrap();
        assert_ne!(wire.first().copied(), Some(b'{'));
    }
}
