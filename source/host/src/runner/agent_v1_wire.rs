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

pub const FROZEN_COMMUNICATE_TOOL_NAMES: &[&str] = &[
    "SendToAgent",
    "CreateAgent",
    "UpdateAgent",
    "CheckSubagent",
    "MessageSubagent",
    "StopSubagent",
    "CopyToBox",
    "CopyFromBox",
    "update_state",
];

pub fn is_agent_v1_observed_tool(name: &str) -> bool {
    matches!(name, "SendMessage" | "send_message")
        || FROZEN_COMMUNICATE_TOOL_NAMES.contains(&name)
}

fn encode_markdown_image_destination(url: &str) -> String {
    let mut output = String::new();
    for ch in url.chars() {
        match ch {
            '<' => output.push_str("%3C"),
            '>' => output.push_str("%3E"),
            '\r' => output.push_str("%0D"),
            '\n' => output.push_str("%0A"),
            _ => output.push(ch),
        }
    }
    format!("<{output}>")
}

fn send_message_text_content(message: &serde_json::Value) -> Result<String, String> {
    let content = required_string(
        message.get("content").or_else(|| message.get("text")),
        "SendMessage text content",
    )?;
    let Some(images) = message.get("images").and_then(serde_json::Value::as_array) else {
        return Ok(content);
    };
    if images.is_empty() {
        return Ok(content);
    }
    let markdown = images
        .iter()
        .map(|image| {
            let url = image
                .get("url")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let alt = image
                .get("alt")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .chars()
                .map(|ch| if matches!(ch, '[' | ']' | '\n') { ' ' } else { ch })
                .collect::<String>();
            format!("![{}]({})", alt.trim(), encode_markdown_image_destination(url))
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!("{content}\n\n{markdown}"))
}

fn widget_summary(message: &serde_json::Value) -> Result<String, String> {
    let widget = message
        .get("widget")
        .ok_or_else(|| "SendMessage widget payload is required".to_string())?;
    let prompt = required_string(widget.get("prompt"), "SendMessage widget prompt")?;
    let labels = widget
        .get("options")
        .and_then(serde_json::Value::as_array)
        .map(|options| {
            options
                .iter()
                .filter_map(|option| option.get("label").and_then(serde_json::Value::as_str))
                .filter(|label| !label.trim().is_empty())
                .map(str::trim)
                .collect::<Vec<_>>()
                .join(" / ")
        })
        .unwrap_or_default();
    Ok(if labels.is_empty() {
        prompt
    } else {
        format!("{prompt} — {labels}")
    })
}

fn send_message_args_from_json(args: &serde_json::Value) -> Result<SendMessageArgs, String> {
    let message = args.get("message").unwrap_or(args);
    let kind = message
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("text");
    let message = match kind {
        "text" => send_message_args::Message::Text(SendMessageText {
            content: send_message_text_content(message)?,
        }),
        "attachment" => {
            let url = required_string(message.get("url"), "SendMessage attachment url")?;
            let alt = message
                .get("alt")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned);
            send_message_args::Message::Attachment(SendMessageAttachment { url, alt })
        }
        "widget" => send_message_args::Message::Text(SendMessageText {
            content: widget_summary(message)?,
        }),
        "cursor-agent" => {
            let bc_id = required_string(message.get("bcId"), "SendMessage cursor-agent bcId")?;
            send_message_args::Message::Text(SendMessageText {
                content: format!("Referenced Cursor cloud agent {bc_id}"),
            })
        },
        "secret-request" => {
            let secret = message.get("secret").unwrap_or(message);
            let label = required_string(secret.get("label"), "SendMessage secret label")?;
            send_message_args::Message::Text(SendMessageText {
                content: format!("Requested a secret from the user securely: {}",
                    label.split_whitespace().collect::<Vec<_>>().join(" ")
                ),
            })
        },
        _ => return Err("unsupported SendMessage canonical message type".into()),
    };
    Ok(SendMessageArgs { message: Some(message) })
}

fn encode_sand_step(fields: &[(&str, String)]) -> String {
    let mut payload = serde_json::Map::new();
    payload.insert("__sand_tool__".into(), serde_json::Value::Bool(true));
    for (key, value) in fields {
        payload.insert((*key).to_string(), serde_json::Value::String(value.clone()));
    }
    serde_json::Value::Object(payload).to_string()
}

fn communicate_result_text(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

/// Encode Desktop's initial Runner observation before a routed side effect starts.
///
/// Unknown tools remain outside this protobuf subset. The returned AgentToolCall has
/// started_at_ms but intentionally no completed_at_ms/result, matching Desktop's
/// SendMessageToolBridge / CommunicateRoutedToolBridge initial phase.
pub fn encode_routed_tool_call_initial_observation(
    name: &str,
    args: &serde_json::Value,
    tool_call_id: &str,
    started_at_ms: u64,
) -> Result<Option<Vec<u8>>, String> {
    let tool_call_id = tool_call_id.trim();
    if tool_call_id.is_empty() {
        return Err("tool_call_id is required".into());
    }
    let tool = match name {
        "SendMessage" | "send_message" => {
            agent_tool_call::Tool::SendMessageToolCall(SendMessageToolCall {
                args: Some(send_message_args_from_json(args)?),
                result: None,
            })
        }
        name if FROZEN_COMMUNICATE_TOOL_NAMES.contains(&name) => {
            let current_step = encode_sand_step(&[
                ("phase", "executing".to_string()),
                ("tool", name.to_string()),
            ]);
            agent_tool_call::Tool::CommunicateUpdateToolCall(CommunicateUpdateToolCall {
                args: Some(CommunicateUpdateArgs {
                    current_step: Some(current_step),
                    final_summary: None,
                    completed_subtitle: None,
                }),
                result: None,
            })
        }
        _ => return Ok(None),
    };
    Ok(Some(
        AgentToolCall {
            tool: Some(tool),
            tool_call_id: Some(tool_call_id.to_string()),
            started_at_ms: Some(started_at_ms),
            completed_at_ms: None,
        }
        .encode_to_vec(),
    ))
}

/// Encode the canonical completed agent.v1 wire observation for shipping Runner dispatches.
///
/// Unknown tools are outside this protobuf subset and return `Ok(None)`. SendMessage
/// and the frozen Desktop communicate-wrapper tool set fail closed rather than
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
        name if FROZEN_COMMUNICATE_TOOL_NAMES.contains(&name) => {
            // Desktop does not expose a literal CommunicateUpdate tool. Instead it wraps
            // these routed tool executions in field-48 CommunicateUpdate observations.
            // Preserve the real routed tool name on the outer observation while the wire
            // payload carries Desktop's canonical __sand_tool__ completion projection.
            let (current_step, result) = match result {
                Ok(value) => {
                    let text = communicate_result_text(value);
                    (
                        encode_sand_step(&[("result", text.clone())]),
                        communicate_update_result::Result::Success(CommunicateUpdateSuccess {
                            current_step: text,
                            message_index: 0,
                        }),
                    )
                }
                Err(error) => (
                    encode_sand_step(&[(
                        "error",
                        if error.is_empty() {
                            "Tool failed.".to_string()
                        } else {
                            error.to_string()
                        },
                    )]),
                    communicate_update_result::Result::Error(CommunicateUpdateError {
                        error: error.to_string(),
                    }),
                ),
            };
            agent_tool_call::Tool::CommunicateUpdateToolCall(CommunicateUpdateToolCall {
                args: Some(CommunicateUpdateArgs {
                    current_step: Some(current_step),
                    final_summary: None,
                    completed_subtitle: None,
                }),
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
    fn initial_observation_matches_desktop_phase_and_has_no_completed_result() {
        let send = encode_routed_tool_call_initial_observation(
            "SendMessage",
            &serde_json::json!({"type":"text","content":"hello"}),
            "send-initial",
            10,
        )
        .unwrap()
        .unwrap();
        let decoded = decode_agent_tool_call(&send).unwrap();
        assert_eq!(decoded.tool_call_id.as_deref(), Some("send-initial"));
        assert_eq!(decoded.started_at_ms, Some(10));
        assert_eq!(decoded.completed_at_ms, None);
        match decoded.tool.unwrap() {
            agent_tool_call::Tool::SendMessageToolCall(call) => assert!(call.result.is_none()),
            _ => panic!("expected SendMessage field 55"),
        }

        let communicate = encode_routed_tool_call_initial_observation(
            "UpdateAgent",
            &serde_json::json!({"id":"agent-1","description":"work"}),
            "update-initial",
            20,
        )
        .unwrap()
        .unwrap();
        let decoded = decode_agent_tool_call(&communicate).unwrap();
        assert_eq!(decoded.completed_at_ms, None);
        match decoded.tool.unwrap() {
            agent_tool_call::Tool::CommunicateUpdateToolCall(call) => {
                assert_eq!(
                    call.args.unwrap().current_step.as_deref(),
                    Some(r#"{"__sand_tool__":true,"phase":"executing","tool":"UpdateAgent"}"#)
                );
                assert!(call.result.is_none());
            }
            _ => panic!("expected CommunicateUpdate field 48"),
        }
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

        let update_args = serde_json::json!({"id":"agent-7","description":"working"});
        let update_result = serde_json::json!({"updated":true});
        let bytes = encode_routed_tool_call_observation(
            "UpdateAgent",
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
        match decoded.tool.unwrap() {
            agent_tool_call::Tool::CommunicateUpdateToolCall(call) => {
                assert_eq!(
                    call.args.unwrap().current_step.as_deref(),
                    Some(r#"{"__sand_tool__":true,"result":"{\"updated\":true}"}"#)
                );
                assert!(matches!(
                    call.result.unwrap().result.unwrap(),
                    communicate_update_result::Result::Success(result)
                        if result.current_step == r#"{"updated":true}"# && result.message_index == 0
                ));
            }
            _ => panic!("wrong AgentToolCall tag"),
        }
    }

    #[test]
    fn cursor_agent_projects_to_frozen_send_message_text() {
        let args = serde_json::json!({"type":"cursor-agent","bcId":"bc-cloud-42"});
        let result = serde_json::json!({"messageId":"cursor-card-1"});
        let wire = encode_routed_tool_call_observation(
            "SendMessage",
            &args,
            Ok(&result),
            "cursor-call",
            20,
            21,
        )
        .unwrap()
        .unwrap();
        assert_eq!(&wire[..2], &[0xba, 0x03]);
        let decoded = decode_agent_tool_call(&wire).unwrap();
        assert_eq!(decoded.tool_call_id.as_deref(), Some("cursor-call"));
        match decoded.tool.unwrap() {
            agent_tool_call::Tool::SendMessageToolCall(call) => {
                match call.args.unwrap().message.unwrap() {
                    send_message_args::Message::Text(text) => {
                        assert_eq!(text.content, "Referenced Cursor cloud agent bc-cloud-42");
                    }
                    _ => panic!("cursor-agent must project to canonical SendMessageText"),
                }
            }
            _ => panic!("cursor-agent must remain field 55 SendMessageToolCall"),
        }
    }

    #[test]
    fn secret_request_projects_to_frozen_send_message_text_without_secret_material() {
        let args = serde_json::json!({
            "type":"secret-request",
            "secret":{
                "label":" Discord   bot token ",
                "description":"never a credential value",
                "connector":"discord",
                "field":"token"
            }
        });
        let result = serde_json::json!({"messageId":"secret-card-1"});
        let wire = encode_routed_tool_call_observation(
            "SendMessage",
            &args,
            Ok(&result),
            "secret-call",
            10,
            11,
        )
        .unwrap()
        .unwrap();
        let decoded = decode_agent_tool_call(&wire).unwrap();
        match decoded.tool.unwrap() {
            agent_tool_call::Tool::SendMessageToolCall(call) => {
                match call.args.unwrap().message.unwrap() {
                    send_message_args::Message::Text(text) => {
                        assert_eq!(
                            text.content,
                            "Requested a secret from the user securely: Discord bot token"
                        );
                        assert!(!text.content.contains("credential value"));
                    }
                    _ => panic!("secret request must project to canonical SendMessageText"),
                }
            }
            _ => panic!("secret request must remain field 55 SendMessageToolCall"),
        }
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
        let widget = serde_json::json!({
            "type":"widget",
            "widget":{"prompt":"Choose","options":[{"label":"Alpha"},{"label":"Beta"}]}
        });
        let widget_result = serde_json::json!({"messageId":"widget-message"});
        let widget_wire = encode_routed_tool_call_observation(
            "SendMessage",
            &widget,
            Ok(&widget_result),
            "widget-call",
            1,
            2,
        )
        .unwrap()
        .unwrap();
        let widget_decoded = decode_agent_tool_call(&widget_wire).unwrap();
        match widget_decoded.tool.unwrap() {
            agent_tool_call::Tool::SendMessageToolCall(call) => match call.args.unwrap().message.unwrap() {
                send_message_args::Message::Text(text) => assert_eq!(text.content, "Choose — Alpha / Beta"),
                _ => panic!("widget must project to canonical SendMessageText"),
            },
            _ => panic!("widget must remain field 55 SendMessageToolCall"),
        }
        assert!(encode_routed_tool_call_observation(
            "CommunicateUpdate",
            &serde_json::json!({}),
            Ok(&serde_json::json!({})),
            "legacy-literal",
            1,
            2,
        )
        .unwrap()
        .is_none());
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
