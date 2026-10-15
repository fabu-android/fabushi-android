use crate::server::{
    RemoteBackendCancelOutcome, RemoteBackendReconcileOutcome, RemoteBackendStartOutcome,
    RemoteExecutionBackend, RemoteRequestIdentity,
};
use fabushi_android_shared::{ExecutionError, ExecutionRequest};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::Read;
use std::time::Duration;
use url::Url;

pub const BOX_ENVIRONMENT_CAPABILITY: &str = "box.environment.apply";
pub const BOX_MCP_CAPABILITY: &str = "box.mcp";
pub const MCP_TOOL_EXEC_FIELD_NUMBER: u32 = 11;
pub const MCP_STATE_EXEC_FIELD_NUMBER: u32 = 36;

const UPDATE_ENVIRONMENT_VARIABLES_PATH: &str =
    "/agent.v1.ControlService/UpdateEnvironmentVariables";
const LOAD_MCP_SERVERS_PATH: &str = "/agent.v1.ControlService/LoadMcpServers";
const EXEC_PATH: &str = "/agent.v1.ExecService/Exec";
const CONNECT_PROTOCOL_VERSION: &str = "1";
const CONNECT_STREAM_CONTENT_TYPE: &str = "application/connect+proto";
const CONNECT_STREAM_END_FLAG: u8 = 0x02;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct ProductionBoxControlConfig {
    pub base_url: String,
    pub headers: BTreeMap<String, String>,
    pub timeout_ms: u64,
}

impl ProductionBoxControlConfig {
    pub fn validate(&self) -> Result<(), ExecutionError> {
        let url = Url::parse(&self.base_url)
            .map_err(|error| ExecutionError::InvalidRequest(format!("invalid Box URL: {error}")))?;
        if url.scheme() != "http" {
            return Err(ExecutionError::InvalidRequest(
                "Box control transport requires http:// loopback".into(),
            ));
        }
        let host = url.host_str().unwrap_or_default();
        if !matches!(host, "127.0.0.1" | "localhost" | "::1") {
            return Err(ExecutionError::InvalidRequest(
                "Box control transport must remain loopback-scoped".into(),
            ));
        }
        if self.timeout_ms == 0 || self.timeout_ms > 120_000 {
            return Err(ExecutionError::InvalidRequest(
                "Box control timeout must be between 1 and 120000 ms".into(),
            ));
        }
        for (name, value) in &self.headers {
            if name.is_empty()
                || name.bytes().any(|byte| byte <= b' ' || byte == b':' || byte >= 0x7f)
                || value.contains('\r')
                || value.contains('\n')
            {
                return Err(ExecutionError::InvalidRequest(format!(
                    "invalid Box control header {name:?}"
                )));
            }
        }
        Ok(())
    }
}

pub struct ProductionBoxControlBackend {
    config: ProductionBoxControlConfig,
    agent: ureq::Agent,
}

impl ProductionBoxControlBackend {
    pub fn new(config: ProductionBoxControlConfig) -> Result<Self, ExecutionError> {
        config.validate()?;
        let timeout = Duration::from_millis(config.timeout_ms);
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(timeout.min(Duration::from_secs(10)))
            .timeout_read(timeout)
            .timeout_write(timeout.min(Duration::from_secs(20)))
            .redirects(0)
            .build();
        Ok(Self { config, agent })
    }

    fn endpoint(&self, path: &str) -> Result<Url, ExecutionError> {
        Url::parse(&self.config.base_url)
            .and_then(|base| base.join(path))
            .map_err(|error| ExecutionError::Transport(format!("invalid Box RPC URL: {error}")))
    }

    fn post(&self, path: &str, content_type: &str, body: &[u8]) -> Result<Vec<u8>, ExecutionError> {
        let url = self.endpoint(path)?;
        let mut request = self
            .agent
            .post(url.as_str())
            .set("content-type", content_type)
            .set("connect-protocol-version", CONNECT_PROTOCOL_VERSION);
        if content_type == CONNECT_STREAM_CONTENT_TYPE {
            request = request.set("accept", CONNECT_STREAM_CONTENT_TYPE);
        }
        for (name, value) in &self.config.headers {
            request = request.set(name, value);
        }
        let response = request
            .send_bytes(body)
            .map_err(|error| ExecutionError::Transport(format!("Box RPC failed: {error}")))?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_RESPONSE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| ExecutionError::Transport(format!("read Box RPC response: {error}")))?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(ExecutionError::Transport(
                "Box RPC response exceeds bounded size".into(),
            ));
        }
        Ok(bytes)
    }

    fn execute_environment(&self, request: &ExecutionRequest) -> Result<String, ExecutionError> {
        let input: EnvironmentInput = serde_json::from_str(&request.input_json)
            .map_err(|error| ExecutionError::InvalidRequest(format!("invalid Box environment input: {error}")))?;
        if input.version != 1 {
            return Err(ExecutionError::InvalidRequest(
                "unsupported Box environment input version".into(),
            ));
        }
        let body = encode_environment_update(&input.update)?;
        self.post(UPDATE_ENVIRONMENT_VARIABLES_PATH, "application/proto", &body)?;
        Ok(r#"{"ok":true}"#.into())
    }

    fn execute_mcp(&self, request: &ExecutionRequest) -> Result<String, ExecutionError> {
        let input: McpInput = serde_json::from_str(&request.input_json)
            .map_err(|error| ExecutionError::InvalidRequest(format!("invalid Box MCP input: {error}")))?;
        if input.version != 1 {
            return Err(ExecutionError::InvalidRequest(
                "unsupported Box MCP input version".into(),
            ));
        }
        match input.action {
            McpAction::LoadServers => {
                let config = input.mcp_config_json.ok_or_else(|| {
                    ExecutionError::InvalidRequest("Box MCP load requires mcpConfigJson".into())
                })?;
                serde_json::from_str::<serde_json::Value>(&config).map_err(|error| {
                    ExecutionError::InvalidRequest(format!("invalid Box MCP config JSON: {error}"))
                })?;
                let body = encode_load_mcp_servers(&config, input.remove_missing);
                let response = self.post(LOAD_MCP_SERVERS_PATH, "application/proto", &body)?;
                let names = decode_loaded_server_names(&response)?;
                serde_json::to_string(&serde_json::json!({"loadedServerNames": names}))
                    .map_err(|error| ExecutionError::Transport(error.to_string()))
            }
            McpAction::ExecuteRaw => {
                let field = input.field_number.ok_or_else(|| {
                    ExecutionError::InvalidRequest("Box MCP raw execution requires fieldNumber".into())
                })?;
                if !matches!(field, MCP_TOOL_EXEC_FIELD_NUMBER | MCP_STATE_EXEC_FIELD_NUMBER) {
                    return Err(ExecutionError::InvalidRequest(format!(
                        "unsupported Box MCP ExecService field {field}"
                    )));
                }
                let payload_hex = input.payload_hex.ok_or_else(|| {
                    ExecutionError::InvalidRequest("Box MCP raw execution requires payloadHex".into())
                })?;
                let payload = decode_hex(&payload_hex)?;
                let exec = encode_exec_raw_request(field, &payload);
                let stream_body = encode_connect_envelope(0, &exec);
                let response = self.post(EXEC_PATH, CONNECT_STREAM_CONTENT_TYPE, &stream_body)?;
                let result = decode_raw_exec_result(&response, field)?;
                Ok(serde_json::json!({"payloadHex": encode_hex(&result)}).to_string())
            }
        }
    }

    fn execute_request(&self, request: &ExecutionRequest) -> Result<String, ExecutionError> {
        match request.capability_id.as_str() {
            BOX_ENVIRONMENT_CAPABILITY => self.execute_environment(request),
            BOX_MCP_CAPABILITY => self.execute_mcp(request),
            other => Err(ExecutionError::CapabilityUnavailable(other.into())),
        }
    }

    fn reconcile_is_safe(request: &ExecutionRequest) -> bool {
        if request.capability_id == BOX_ENVIRONMENT_CAPABILITY {
            return true;
        }
        if request.capability_id != BOX_MCP_CAPABILITY {
            return false;
        }
        serde_json::from_str::<McpInput>(&request.input_json)
            .ok()
            .is_some_and(|input| match input.action {
                McpAction::LoadServers => true,
                McpAction::ExecuteRaw => input.field_number == Some(MCP_STATE_EXEC_FIELD_NUMBER),
            })
    }
}

impl RemoteExecutionBackend for ProductionBoxControlBackend {
    fn start(
        &mut self,
        _identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendStartOutcome, ExecutionError> {
        let output_json = self.execute_request(request)?;
        Ok(RemoteBackendStartOutcome::Completed { output_json })
    }

    fn reconcile(
        &mut self,
        _identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
        _progress_json: Option<&str>,
    ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
        if !Self::reconcile_is_safe(request) {
            return Ok(RemoteBackendReconcileOutcome::Pending);
        }
        let output_json = self.execute_request(request)?;
        Ok(RemoteBackendReconcileOutcome::Completed { output_json })
    }

    fn cancel(
        &mut self,
        _identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
        _progress_json: Option<&str>,
    ) -> Result<RemoteBackendCancelOutcome, ExecutionError> {
        if Self::reconcile_is_safe(request) {
            Ok(RemoteBackendCancelOutcome::Confirmed)
        } else {
            Ok(RemoteBackendCancelOutcome::OutcomeUnknown {
                reason: "Box MCP tool execution may already have crossed the side-effect boundary".into(),
            })
        }
    }
}

pub fn open_production_box_control_service<A: crate::server::RemoteExecutionAuthorizer>(
    journal_path: impl Into<std::path::PathBuf>,
    authorizer: A,
    config: ProductionBoxControlConfig,
    now_ms: u64,
) -> Result<crate::server::RemoteExecutionService<A, ProductionBoxControlBackend>, ExecutionError> {
    crate::server::RemoteExecutionService::open(
        journal_path,
        authorizer,
        ProductionBoxControlBackend::new(config)?,
        now_ms,
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EnvironmentInput {
    version: u32,
    update: EnvironmentUpdate,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EnvironmentUpdate {
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    replace: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum McpAction {
    LoadServers,
    ExecuteRaw,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpInput {
    version: u32,
    action: McpAction,
    #[serde(default)]
    mcp_config_json: Option<String>,
    #[serde(default)]
    remove_missing: bool,
    #[serde(default)]
    field_number: Option<u32>,
    #[serde(default)]
    payload_hex: Option<String>,
}

fn encode_key(field: u32, wire: u8, out: &mut Vec<u8>) {
    encode_varint(((u64::from(field)) << 3) | u64::from(wire), out);
}

fn encode_varint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn encode_len(field: u32, value: &[u8], out: &mut Vec<u8>) {
    encode_key(field, 2, out);
    encode_varint(value.len() as u64, out);
    out.extend_from_slice(value);
}

fn encode_environment_update(update: &EnvironmentUpdate) -> Result<Vec<u8>, ExecutionError> {
    if update.env.len() > 512 {
        return Err(ExecutionError::InvalidRequest(
            "Box environment update has too many entries".into(),
        ));
    }
    let mut body = Vec::new();
    for (name, value) in &update.env {
        if name.trim().is_empty()
            || name.len() > 512
            || name.chars().any(char::is_control)
            || value.len() > 16 * 1024
            || value.chars().any(|ch| ch == '\0')
        {
            return Err(ExecutionError::InvalidRequest(
                "Box environment update contains an invalid entry".into(),
            ));
        }
        let mut entry = Vec::new();
        encode_len(1, name.as_bytes(), &mut entry);
        encode_len(2, value.as_bytes(), &mut entry);
        encode_len(1, &entry, &mut body);
    }
    if update.replace {
        encode_key(2, 0, &mut body);
        encode_varint(1, &mut body);
    }
    Ok(body)
}

fn encode_load_mcp_servers(config_json: &str, remove_missing: bool) -> Vec<u8> {
    let mut body = Vec::new();
    if !config_json.is_empty() {
        encode_len(1, config_json.as_bytes(), &mut body);
    }
    if remove_missing {
        encode_key(2, 0, &mut body);
        encode_varint(1, &mut body);
    }
    body
}

fn encode_exec_raw_request(field: u32, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    encode_key(1, 0, &mut body);
    encode_varint(1, &mut body);
    encode_len(field, payload, &mut body);
    body
}

fn encode_connect_envelope(flags: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 5);
    out.push(flags);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

fn read_varint(input: &[u8], cursor: &mut usize) -> Result<u64, ExecutionError> {
    let mut value = 0_u64;
    let mut shift = 0_u32;
    while *cursor < input.len() && shift < 64 {
        let byte = input[*cursor];
        *cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
    }
    Err(ExecutionError::Transport("malformed Box protobuf varint".into()))
}

fn read_len<'a>(input: &'a [u8], cursor: &mut usize) -> Result<&'a [u8], ExecutionError> {
    let length = usize::try_from(read_varint(input, cursor)?)
        .map_err(|_| ExecutionError::Transport("Box protobuf length overflow".into()))?;
    let end = cursor
        .checked_add(length)
        .ok_or_else(|| ExecutionError::Transport("Box protobuf length overflow".into()))?;
    if end > input.len() {
        return Err(ExecutionError::Transport("truncated Box protobuf".into()));
    }
    let value = &input[*cursor..end];
    *cursor = end;
    Ok(value)
}

fn skip_field(input: &[u8], cursor: &mut usize, wire: u8) -> Result<(), ExecutionError> {
    match wire {
        0 => {
            let _ = read_varint(input, cursor)?;
        }
        1 => *cursor = cursor.saturating_add(8),
        2 => {
            let _ = read_len(input, cursor)?;
        }
        5 => *cursor = cursor.saturating_add(4),
        _ => return Err(ExecutionError::Transport(format!("unsupported Box protobuf wire type {wire}"))),
    }
    if *cursor > input.len() {
        return Err(ExecutionError::Transport("truncated Box protobuf".into()));
    }
    Ok(())
}

fn decode_loaded_server_names(input: &[u8]) -> Result<Vec<String>, ExecutionError> {
    let mut cursor = 0;
    let mut names = Vec::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 7) as u8;
        if field == 1 && wire == 2 {
            let raw = read_len(input, &mut cursor)?;
            names.push(
                std::str::from_utf8(raw)
                    .map_err(|_| ExecutionError::Transport("non-UTF-8 Box MCP server name".into()))?
                    .to_string(),
            );
        } else {
            skip_field(input, &mut cursor, wire)?;
        }
    }
    Ok(names)
}

fn decode_raw_exec_result(body: &[u8], target_field: u32) -> Result<Vec<u8>, ExecutionError> {
    let mut cursor = 0;
    while cursor < body.len() {
        if body.len().saturating_sub(cursor) < 5 {
            return Err(ExecutionError::Transport(
                "truncated Box Connect envelope".into(),
            ));
        }
        let flags = body[cursor];
        let length = u32::from_be_bytes([
            body[cursor + 1],
            body[cursor + 2],
            body[cursor + 3],
            body[cursor + 4],
        ]) as usize;
        cursor += 5;
        let end = cursor
            .checked_add(length)
            .ok_or_else(|| ExecutionError::Transport("Box Connect length overflow".into()))?;
        if end > body.len() {
            return Err(ExecutionError::Transport(
                "truncated Box Connect payload".into(),
            ));
        }
        let payload = &body[cursor..end];
        cursor = end;
        if flags & CONNECT_STREAM_END_FLAG != 0 {
            if !payload.is_empty() {
                let end_stream: serde_json::Value = serde_json::from_slice(payload).map_err(|error| {
                    ExecutionError::Transport(format!("invalid Box Connect end envelope: {error}"))
                })?;
                if end_stream.get("error").is_some() {
                    return Err(ExecutionError::Transport(format!(
                        "Box ExecService Connect error: {}",
                        end_stream["error"]
                    )));
                }
            }
            continue;
        }
        if flags != 0 {
            return Err(ExecutionError::Transport(format!(
                "unsupported Box Connect flags 0x{flags:02x}"
            )));
        }
        if let Some(result) = extract_raw_from_stream_element(payload, target_field)? {
            return Ok(result);
        }
    }
    Err(ExecutionError::Transport(
        "Box ExecService closed without the requested raw-resource result".into(),
    ))
}

fn extract_raw_from_stream_element(
    input: &[u8],
    target_field: u32,
) -> Result<Option<Vec<u8>>, ExecutionError> {
    let mut cursor = 0;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 7) as u8;
        if field == 1 && wire == 2 {
            let client_message = read_len(input, &mut cursor)?;
            return extract_raw_from_client_message(client_message, target_field);
        }
        skip_field(input, &mut cursor, wire)?;
    }
    Ok(None)
}

fn extract_raw_from_client_message(
    input: &[u8],
    target_field: u32,
) -> Result<Option<Vec<u8>>, ExecutionError> {
    let mut cursor = 0;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = u32::try_from(key >> 3).unwrap_or(u32::MAX);
        let wire = (key & 7) as u8;
        if field == target_field && wire == 2 {
            return Ok(Some(read_len(input, &mut cursor)?.to_vec()));
        }
        skip_field(input, &mut cursor, wire)?;
    }
    Ok(None)
}

fn decode_hex(raw: &str) -> Result<Vec<u8>, ExecutionError> {
    if raw.len() % 2 != 0 {
        return Err(ExecutionError::InvalidRequest(
            "Box MCP payload must be even-length hex".into(),
        ));
    }
    raw.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char)
                .to_digit(16)
                .ok_or_else(|| ExecutionError::InvalidRequest("Box MCP payload contains non-hex data".into()))?;
            let low = (pair[1] as char)
                .to_digit(16)
                .ok_or_else(|| ExecutionError::InvalidRequest("Box MCP payload contains non-hex data".into()))?;
            Ok(((high << 4) | low) as u8)
        })
        .collect()
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read as _, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn spawn_rpc_server(
        responses: Vec<Vec<u8>>,
    ) -> (String, Arc<Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let captured = Arc::new(Mutex::new(Vec::new()));
        let worker_captured = Arc::clone(&captured);
        let handle = std::thread::spawn(move || {
            for body in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut data = Vec::new();
                let mut chunk = [0_u8; 4096];
                let header_end = loop {
                    let count = stream.read(&mut chunk).unwrap();
                    assert!(count > 0);
                    data.extend_from_slice(&chunk[..count]);
                    if let Some(index) = data.windows(4).position(|window| window == b"\r\n\r\n") {
                        break index + 4;
                    }
                };
                let header_text = String::from_utf8_lossy(&data[..header_end]);
                let content_length = header_text
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                while data.len() < header_end + content_length {
                    let count = stream.read(&mut chunk).unwrap();
                    if count == 0 { break; }
                    data.extend_from_slice(&chunk[..count]);
                }
                worker_captured.lock().unwrap().push(String::from_utf8_lossy(&data).into_owned());
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len(),
                ).unwrap();
                stream.write_all(&body).unwrap();
                stream.flush().unwrap();
            }
        });
        (format!("http://{address}/"), captured, handle)
    }

    fn backend_with_timeout(base_url: String, timeout_ms: u64) -> ProductionBoxControlBackend {
        ProductionBoxControlBackend::new(ProductionBoxControlConfig {
            base_url,
            headers: BTreeMap::new(),
            timeout_ms,
        }).unwrap()
    }

    fn backend(base_url: String) -> ProductionBoxControlBackend {
        backend_with_timeout(base_url, 1_000)
    }

    fn identity(operation_id: &str) -> RemoteRequestIdentity {
        RemoteRequestIdentity {
            credential_id: "credential-box".into(),
            account_fence: "session:account-a".into(),
            account_epoch: 7,
            operation_id: operation_id.into(),
            request_id: format!("request-{operation_id}"),
            permission_grant_id: format!("grant-{operation_id}"),
            device_id: "device-box".into(),
        }
    }

    #[test]
    fn config_is_loopback_only_and_tool_reconcile_never_replays() {
        assert!(ProductionBoxControlConfig {
            base_url: "http://127.0.0.1:4567".into(),
            headers: BTreeMap::new(),
            timeout_ms: 15_000,
        }
        .validate()
        .is_ok());
        assert!(ProductionBoxControlConfig {
            base_url: "https://example.com".into(),
            headers: BTreeMap::new(),
            timeout_ms: 15_000,
        }
        .validate()
        .is_err());

        let tool = ExecutionRequest {
            operation_id: "op-tool".into(),
            capability_id: BOX_MCP_CAPABILITY.into(),
            input_json: serde_json::json!({
                "version": 1,
                "action": "execute_raw",
                "fieldNumber": MCP_TOOL_EXEC_FIELD_NUMBER,
                "payloadHex": "00"
            })
            .to_string(),
            timeout_ms: 30_000,
        };
        assert!(!ProductionBoxControlBackend::reconcile_is_safe(&tool));

        let state = ExecutionRequest {
            operation_id: "op-state".into(),
            input_json: serde_json::json!({
                "version": 1,
                "action": "execute_raw",
                "fieldNumber": MCP_STATE_EXEC_FIELD_NUMBER,
                "payloadHex": "00"
            })
            .to_string(),
            ..tool
        };
        assert!(ProductionBoxControlBackend::reconcile_is_safe(&state));
    }

    #[test]
    fn production_backend_executes_environment_load_and_state_over_frozen_box_wire() {
        let mut load_response = Vec::new();
        encode_len(1, b"calendar", &mut load_response);
        encode_len(1, b"github", &mut load_response);

        let mut state_client_message = Vec::new();
        encode_len(MCP_STATE_EXEC_FIELD_NUMBER, &[0xde, 0xad], &mut state_client_message);
        let mut state_stream_element = Vec::new();
        encode_len(1, &state_client_message, &mut state_stream_element);
        let state_response = encode_connect_envelope(0, &state_stream_element);
        let mut tool_client_message = Vec::new();
        encode_len(MCP_TOOL_EXEC_FIELD_NUMBER, &[0xbe, 0xef], &mut tool_client_message);
        let mut tool_stream_element = Vec::new();
        encode_len(1, &tool_client_message, &mut tool_stream_element);
        let tool_response = encode_connect_envelope(0, &tool_stream_element);

        let (base_url, captured, handle) = spawn_rpc_server(vec![
            Vec::new(),
            load_response,
            state_response,
            tool_response,
        ]);
        let mut backend = backend(base_url);

        let env = ExecutionRequest {
            operation_id: "box-env".into(),
            capability_id: BOX_ENVIRONMENT_CAPABILITY.into(),
            input_json: serde_json::json!({
                "version":1,
                "update":{"env":{"FOO":"bar"},"replace":true}
            }).to_string(),
            timeout_ms: 30_000,
        };
        assert!(matches!(
            backend.start(&identity("box-env"), &env).unwrap(),
            RemoteBackendStartOutcome::Completed { output_json } if output_json == r#"{"ok":true}"#
        ));

        let load = ExecutionRequest {
            operation_id: "box-load".into(),
            capability_id: BOX_MCP_CAPABILITY.into(),
            input_json: serde_json::json!({
                "version":1,
                "action":"load_servers",
                "mcpConfigJson":"{\"mcpServers\":{}}",
                "removeMissing":true
            }).to_string(),
            timeout_ms: 30_000,
        };
        let RemoteBackendStartOutcome::Completed { output_json } = backend
            .start(&identity("box-load"), &load)
            .unwrap() else { panic!("MCP load must complete in the production Box backend") };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&output_json).unwrap()["loadedServerNames"],
            serde_json::json!(["calendar", "github"]),
        );

        let state = ExecutionRequest {
            operation_id: "box-state".into(),
            capability_id: BOX_MCP_CAPABILITY.into(),
            input_json: serde_json::json!({
                "version":1,
                "action":"execute_raw",
                "fieldNumber":MCP_STATE_EXEC_FIELD_NUMBER,
                "payloadHex":"00ff"
            }).to_string(),
            timeout_ms: 30_000,
        };
        let RemoteBackendReconcileOutcome::Completed { output_json } = backend
            .reconcile(&identity("box-state"), &state, None)
            .unwrap() else { panic!("field 36 state reconcile must be safely repeatable") };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&output_json).unwrap()["payloadHex"],
            "dead",
        );

        let tool = ExecutionRequest {
            operation_id: "box-tool-start".into(),
            capability_id: BOX_MCP_CAPABILITY.into(),
            input_json: serde_json::json!({
                "version":1,
                "action":"execute_raw",
                "fieldNumber":MCP_TOOL_EXEC_FIELD_NUMBER,
                "payloadHex":"00ff"
            }).to_string(),
            timeout_ms: 30_000,
        };
        let RemoteBackendStartOutcome::Completed { output_json } = backend
            .start(&identity("box-tool-start"), &tool)
            .unwrap() else { panic!("field 11 tool execution must complete on first dispatch") };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&output_json).unwrap()["payloadHex"],
            "beef",
        );

        handle.join().unwrap();
        let requests = captured.lock().unwrap();
        assert_eq!(requests.len(), 4);
        assert!(requests[0].contains(UPDATE_ENVIRONMENT_VARIABLES_PATH));
        assert!(requests[1].contains(LOAD_MCP_SERVERS_PATH));
        assert!(requests[2].contains(EXEC_PATH));
        assert!(requests[3].contains(EXEC_PATH));
        assert!(requests[2].to_ascii_lowercase().contains("application/connect+proto"));
        assert!(requests[3].to_ascii_lowercase().contains("application/connect+proto"));
    }

    #[test]
    fn mcp_tool_reconcile_and_cancel_preserve_outcome_unknown_without_second_rpc() {
        let mut backend = backend("http://127.0.0.1:9/".into());
        let request = ExecutionRequest {
            operation_id: "box-tool".into(),
            capability_id: BOX_MCP_CAPABILITY.into(),
            input_json: serde_json::json!({
                "version":1,
                "action":"execute_raw",
                "fieldNumber":MCP_TOOL_EXEC_FIELD_NUMBER,
                "payloadHex":"00ff"
            }).to_string(),
            timeout_ms: 30_000,
        };
        assert!(matches!(
            backend.reconcile(&identity("box-tool"), &request, None).unwrap(),
            RemoteBackendReconcileOutcome::Pending
        ));
        assert!(matches!(
            backend.cancel(&identity("box-tool"), &request, None).unwrap(),
            RemoteBackendCancelOutcome::OutcomeUnknown { reason }
                if reason.contains("side-effect boundary")
        ));
    }

    #[test]
    fn box_rpc_read_timeout_surfaces_transport_error_without_fabricating_completion() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 2048];
            let _ = stream.read(&mut buffer);
            std::thread::sleep(Duration::from_millis(100));
        });
        let mut backend = backend_with_timeout(format!("http://{address}/"), 20);
        let request = ExecutionRequest {
            operation_id: "box-env-timeout".into(),
            capability_id: BOX_ENVIRONMENT_CAPABILITY.into(),
            input_json: serde_json::json!({
                "version":1,
                "update":{"env":{"FOO":"bar"},"replace":false}
            }).to_string(),
            timeout_ms: 30_000,
        };
        assert!(matches!(
            backend.start(&identity("box-env-timeout"), &request),
            Err(ExecutionError::Transport(_))
        ));
        handle.join().unwrap();
    }

    #[test]
    fn reopened_mcp_tool_journal_stays_outcome_unknown_and_reconcile_does_not_replay() {
        use crate::server::{RemoteExecutionAuthorizer, RemoteExecutionService, RemoteHttpRequest};
        use crate::transport::RECONCILE_PATH;
        use tempfile::tempdir;

        struct AllowAuthorizer;
        impl RemoteExecutionAuthorizer for AllowAuthorizer {
            fn authorize(
                &mut self,
                _bearer: &str,
                _identity: &RemoteRequestIdentity,
                _capability_id: Option<&str>,
                _now_ms: u64,
            ) -> Result<(), ExecutionError> {
                Ok(())
            }
        }

        let root = tempdir().unwrap();
        let journal = root.path().join("remote-execution.json");
        std::fs::write(
            &journal,
            serde_json::json!({
                "version":1,
                "records":[{
                    "credentialId":"credential-box",
                    "operationId":"box-tool-reopen",
                    "requestId":"request-box-tool-reopen",
                    "capabilityId":BOX_MCP_CAPABILITY,
                    "inputJson":serde_json::json!({
                        "version":1,
                        "action":"execute_raw",
                        "fieldNumber":MCP_TOOL_EXEC_FIELD_NUMBER,
                        "payloadHex":"00ff"
                    }).to_string(),
                    "timeoutMs":30000,
                    "accountFence":"session:account-a",
                    "accountEpoch":7,
                    "permissionGrantId":"grant-box-tool-reopen",
                    "deviceId":"device-box",
                    "ackId":"ack-box-tool-reopen",
                    "state":"running",
                    "progressJson":null,
                    "outputJson":null,
                    "error":null,
                    "updatedAtMs":1
                }]
            }).to_string(),
        ).unwrap();

        let backend = backend("http://127.0.0.1:9/".into());
        let mut service = RemoteExecutionService::open(&journal, AllowAuthorizer, backend, 10).unwrap();
        let identity = identity("box-tool-reopen");
        let headers = BTreeMap::from([
            ("Authorization".into(), "Bearer real-enough-secret-token-for-test".into()),
            ("X-Fabushi-Credential-Id".into(), identity.credential_id.clone()),
            ("X-Fabushi-Account-Fence".into(), identity.account_fence.clone()),
            ("X-Fabushi-Account-Epoch".into(), identity.account_epoch.to_string()),
            ("X-Fabushi-Operation-Id".into(), identity.operation_id.clone()),
            ("X-Fabushi-Request-Id".into(), identity.request_id.clone()),
            ("X-Fabushi-Permission-Grant-Id".into(), identity.permission_grant_id.clone()),
            ("X-Fabushi-Device-Id".into(), identity.device_id.clone()),
        ]);
        let response = service.handle_http(
            RemoteHttpRequest {
                path: RECONCILE_PATH.into(),
                headers,
                body: serde_json::json!({
                    "version":1,
                    "credentialId":identity.credential_id,
                    "operationId":identity.operation_id,
                    "requestId":identity.request_id,
                    "deviceId":identity.device_id,
                    "accountFence":identity.account_fence,
                    "accountEpoch":identity.account_epoch,
                    "permissionGrantId":identity.permission_grant_id
                }).to_string(),
            },
            11,
        );
        assert_eq!(response.status_code, 200);
        let body: serde_json::Value = serde_json::from_str(&response.body).unwrap();
        assert_eq!(body["status"], "outcome_unknown");
    }

    #[test]
    fn transport_failure_surfaces_error_instead_of_fabricating_box_success() {
        let mut backend = backend("http://127.0.0.1:9/".into());
        let request = ExecutionRequest {
            operation_id: "box-env-failure".into(),
            capability_id: BOX_ENVIRONMENT_CAPABILITY.into(),
            input_json: serde_json::json!({
                "version":1,
                "update":{"env":{"FOO":"bar"},"replace":false}
            }).to_string(),
            timeout_ms: 30_000,
        };
        assert!(matches!(
            backend.start(&identity("box-env-failure"), &request),
            Err(ExecutionError::Transport(_))
        ));
    }

    #[test]
    fn codecs_preserve_desktop_mcp_and_environment_wire() {
        let env = EnvironmentUpdate {
            env: BTreeMap::from([("FOO".into(), "bar".into())]),
            replace: true,
        };
        let encoded = encode_environment_update(&env).unwrap();
        assert!(encoded.ends_with(&[0x10, 0x01]));

        assert_eq!(
            encode_load_mcp_servers("{}", true),
            vec![0x0a, 0x02, b'{', b'}', 0x10, 0x01]
        );
        let mut response = Vec::new();
        encode_len(1, b"calendar", &mut response);
        encode_len(1, b"github", &mut response);
        assert_eq!(
            decode_loaded_server_names(&response).unwrap(),
            vec!["calendar", "github"]
        );

        let exec = encode_exec_raw_request(MCP_STATE_EXEC_FIELD_NUMBER, &[0x12, 0x00]);
        assert!(exec.windows(2).any(|window| window == [0xa2, 0x02]));
    }
}
