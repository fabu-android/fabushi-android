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
    fn descriptor_authority_is_pinned_and_json_fallback_is_not_part_of_wire_api() {
        assert_eq!(FROZEN_AGENT_PB_BLOB_SHA, "3cd3d020e69b54d1a6df27c0f967afe7e9fc942c");
        assert_eq!(FROZEN_SEND_MESSAGE_PB_BLOB_SHA, "fff9a91c8bf835d33b9d38b855c8785a9ebab03e");
        assert_eq!(FROZEN_COMMUNICATE_UPDATE_PB_BLOB_SHA, "20af9fc29e574015c5a40ab314e15002dd232cc5");
        let wire = encode_send_message_text_success("c", "{\"json\":false}", 1, 2, "m").unwrap();
        assert_ne!(wire.first().copied(), Some(b'{'));
    }
}
