use prost::Message;
use serde_json::Value;

pub const MCP_TOOL_EXEC_FIELD_NUMBER: u32 = 11;
pub const MCP_STATE_EXEC_FIELD_NUMBER: u32 = 36;

#[derive(Clone, PartialEq, Message)]
pub struct CanonicalMcpStateExecArgs {
    #[prost(string, repeated, tag = "1")]
    pub server_identifiers: Vec<String>,
    #[prost(bool, tag = "2")]
    pub kick_only: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct CanonicalMcpToolDefinition {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub description: String,
    #[prost(message, optional, tag = "3")]
    pub input_schema: Option<pbjson_types::Value>,
    #[prost(string, tag = "4")]
    pub provider_identifier: String,
    #[prost(string, tag = "5")]
    pub tool_name: String,
    #[prost(string, optional, tag = "6")]
    pub input_schema_json: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CanonicalMcpInstructions {}

#[derive(Clone, PartialEq, Message)]
pub struct CanonicalMcpStateServer {
    #[prost(string, tag = "1")]
    pub server_name: String,
    #[prost(string, tag = "2")]
    pub server_identifier: String,
    #[prost(string, optional, tag = "3")]
    pub plugin: Option<String>,
    #[prost(string, optional, tag = "4")]
    pub marketplace: Option<String>,
    #[prost(message, repeated, tag = "5")]
    pub tools: Vec<CanonicalMcpToolDefinition>,
    #[prost(message, repeated, tag = "6")]
    pub instructions: Vec<CanonicalMcpInstructions>,
    #[prost(string, optional, tag = "7")]
    pub status: Option<String>,
    #[prost(string, optional, tag = "8")]
    pub error_message: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CanonicalMcpStateSuccess {
    #[prost(message, repeated, tag = "1")]
    pub servers: Vec<CanonicalMcpStateServer>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CanonicalMcpStateError {
    #[prost(string, tag = "1")]
    pub error: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct CanonicalMcpStateRejected {
    #[prost(string, tag = "1")]
    pub reason: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct CanonicalMcpStateExecResult {
    #[prost(oneof = "canonical_mcp_state_exec_result::Result", tags = "1, 2, 3")]
    pub result: Option<canonical_mcp_state_exec_result::Result>,
}

pub mod canonical_mcp_state_exec_result {
    use super::{CanonicalMcpStateError, CanonicalMcpStateRejected, CanonicalMcpStateSuccess};
    use prost::Oneof;

    #[derive(Clone, PartialEq, Oneof)]
    pub enum Result {
        #[prost(message, tag = "1")]
        Success(CanonicalMcpStateSuccess),
        #[prost(message, tag = "2")]
        Error(CanonicalMcpStateError),
        #[prost(message, tag = "3")]
        Rejected(CanonicalMcpStateRejected),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpStateToolDefinition {
    pub name: String,
    pub provider_identifier: String,
    pub tool_name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpStateServer {
    pub server_identifier: String,
    pub server_name: String,
    pub status: String,
    pub error_message: Option<String>,
    pub tools: Vec<McpStateToolDefinition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpStateSuccess {
    pub servers: Vec<McpStateServer>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum McpStateExecResult {
    Success(McpStateSuccess),
}

pub fn encode_canonical_mcp_state_args(
    server_identifiers: &[String],
    kick_only: bool,
) -> Vec<u8> {
    CanonicalMcpStateExecArgs {
        server_identifiers: server_identifiers.to_vec(),
        kick_only,
    }
    .encode_to_vec()
}

pub fn decode_canonical_mcp_state_args(
    bytes: &[u8],
) -> Result<CanonicalMcpStateExecArgs, String> {
    CanonicalMcpStateExecArgs::decode(bytes)
        .map_err(|error| format!("invalid agent.v1.McpStateExecArgs protobuf: {error}"))
}

fn json_to_proto_value(value: Value) -> Result<pbjson_types::Value, String> {
    serde_json::from_value(value)
        .map_err(|error| format!("invalid MCP input schema protobuf Value: {error}"))
}

fn proto_to_json_value(value: pbjson_types::Value) -> Result<Value, String> {
    serde_json::to_value(value)
        .map_err(|error| format!("invalid MCP input schema JSON projection: {error}"))
}

pub fn encode_canonical_mcp_state_result(
    result: &McpStateExecResult,
) -> Result<Vec<u8>, String> {
    let McpStateExecResult::Success(McpStateSuccess { servers }) = result;
    let mut wire_servers = Vec::with_capacity(servers.len());
    for server in servers {
        let mut tools = Vec::with_capacity(server.tools.len());
        for tool in &server.tools {
            tools.push(CanonicalMcpToolDefinition {
                name: tool.name.clone(),
                description: tool.description.clone().unwrap_or_default(),
                input_schema: Some(json_to_proto_value(tool.input_schema.clone())?),
                provider_identifier: tool.provider_identifier.clone(),
                tool_name: tool.tool_name.clone(),
                input_schema_json: None,
            });
        }
        wire_servers.push(CanonicalMcpStateServer {
            server_name: server.server_name.clone(),
            server_identifier: server.server_identifier.clone(),
            plugin: None,
            marketplace: None,
            tools,
            instructions: Vec::new(),
            status: Some(server.status.clone()),
            error_message: server.error_message.clone(),
        });
    }
    Ok(CanonicalMcpStateExecResult {
        result: Some(canonical_mcp_state_exec_result::Result::Success(
            CanonicalMcpStateSuccess {
                servers: wire_servers,
            },
        )),
    }
    .encode_to_vec())
}

pub fn decode_canonical_mcp_state_result(
    bytes: &[u8],
) -> Result<McpStateExecResult, String> {
    let result = CanonicalMcpStateExecResult::decode(bytes)
        .map_err(|error| format!("invalid agent.v1.McpStateExecResult protobuf: {error}"))?;
    let success = match result.result {
        Some(canonical_mcp_state_exec_result::Result::Success(success)) => success,
        Some(canonical_mcp_state_exec_result::Result::Error(error)) => {
            return Err(format!("MCP state failed: {}", error.error));
        }
        Some(canonical_mcp_state_exec_result::Result::Rejected(rejected)) => {
            return Err(format!("MCP state rejected: {}", rejected.reason));
        }
        None => return Err("MCP state returned no result".into()),
    };

    let mut servers = Vec::with_capacity(success.servers.len());
    for server in success.servers {
        let mut tools = Vec::with_capacity(server.tools.len());
        for tool in server.tools {
            let input_schema = match tool.input_schema {
                Some(value) => proto_to_json_value(value)?,
                None => tool
                    .input_schema_json
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .map(serde_json::from_str)
                    .transpose()
                    .map_err(|error| format!("invalid MCP input_schema_json: {error}"))?
                    .unwrap_or(Value::Null),
            };
            tools.push(McpStateToolDefinition {
                name: tool.name,
                provider_identifier: tool.provider_identifier,
                tool_name: tool.tool_name,
                description: (!tool.description.is_empty()).then_some(tool.description),
                input_schema,
            });
        }
        servers.push(McpStateServer {
            server_identifier: server.server_identifier,
            server_name: server.server_name,
            status: server.status.unwrap_or_else(|| "connected".into()),
            error_message: server.error_message,
            tools,
        });
    }
    Ok(McpStateExecResult::Success(McpStateSuccess { servers }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn desktop_exec_service_field_numbers_remain_stable() {
        assert_eq!(MCP_TOOL_EXEC_FIELD_NUMBER, 11);
        assert_eq!(MCP_STATE_EXEC_FIELD_NUMBER, 36);
    }

    #[test]
    fn state_args_round_trip_preserves_server_filter_and_kick_only() {
        let encoded =
            encode_canonical_mcp_state_args(&["calendar".into(), "github".into()], true);
        let decoded = decode_canonical_mcp_state_args(&encoded).unwrap();
        assert_eq!(decoded.server_identifiers, vec!["calendar", "github"]);
        assert!(decoded.kick_only);
    }

    #[test]
    fn state_result_round_trip_preserves_tool_identity_and_schema() {
        let input = McpStateExecResult::Success(McpStateSuccess {
            servers: vec![McpStateServer {
                server_identifier: "calendar".into(),
                server_name: "Calendar".into(),
                status: "connected".into(),
                error_message: None,
                tools: vec![McpStateToolDefinition {
                    name: "search".into(),
                    provider_identifier: "calendar".into(),
                    tool_name: "search".into(),
                    description: Some("Search calendar".into()),
                    input_schema: json!({"type":"object","required":["query"]}),
                }],
            }],
        });
        let encoded = encode_canonical_mcp_state_result(&input).unwrap();
        assert_eq!(decode_canonical_mcp_state_result(&encoded).unwrap(), input);
    }

    #[test]
    fn rejected_and_error_wire_results_fail_closed() {
        for result in [
            canonical_mcp_state_exec_result::Result::Error(CanonicalMcpStateError {
                error: "backend unavailable".into(),
            }),
            canonical_mcp_state_exec_result::Result::Rejected(CanonicalMcpStateRejected {
                reason: "permission revoked".into(),
            }),
        ] {
            let bytes = CanonicalMcpStateExecResult {
                result: Some(result),
            }
            .encode_to_vec();
            assert!(decode_canonical_mcp_state_result(&bytes).is_err());
        }
    }

    #[test]
    fn legacy_input_schema_json_fallback_is_preserved() {
        let bytes = CanonicalMcpStateExecResult {
            result: Some(canonical_mcp_state_exec_result::Result::Success(
                CanonicalMcpStateSuccess {
                    servers: vec![CanonicalMcpStateServer {
                        server_name: "Legacy".into(),
                        server_identifier: "legacy".into(),
                        plugin: None,
                        marketplace: None,
                        tools: vec![CanonicalMcpToolDefinition {
                            name: "read".into(),
                            description: String::new(),
                            input_schema: None,
                            provider_identifier: "legacy".into(),
                            tool_name: "read".into(),
                            input_schema_json: Some(r#"{"type":"object"}"#.into()),
                        }],
                        instructions: Vec::new(),
                        status: None,
                        error_message: None,
                    }],
                },
            )),
        }
        .encode_to_vec();

        let McpStateExecResult::Success(result) =
            decode_canonical_mcp_state_result(&bytes).unwrap();
        assert_eq!(result.servers[0].status, "connected");
        assert_eq!(result.servers[0].tools[0].input_schema, json!({"type":"object"}));
    }
}
