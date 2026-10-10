use crate::mcp_auth::{
    CursorDashboardMcpAuthBackend, ProcessSandMcpCredentialProvider, SandMcpCredentialProvider,
    SandPrivacyMode,
};
use crate::mcp_auth::dashboard_mcp_auth_backend::create_cursor_checksum;
use std::env;
use std::io::Read;
use std::time::Duration;
use url::Url;

pub const GROK_BOT_RECREATE_PATH: &str =
    "/aiserver.v1.GrokBotService/RecreateSandBox";
pub const GROK_BOT_FORCE_RECREATE_PATH: &str =
    "/aiserver.v1.GrokBotService/ForceRecreateSandBox";
pub const GROK_BOT_WATCH_MIGRATION_PATH: &str =
    "/aiserver.v1.GrokBotService/WatchSandBoxMigration";
pub const MIGRATION_WATCH_STALL_MS: u64 = 30_000;
const CONTROL_RPC_TIMEOUT_MS: u64 = 30_000;
const MAX_RPC_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_REQUEST_ID_BYTES: usize = 240;
const MAX_OFFSET_KEY_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputerRecreateReply {
    pub started: bool,
    pub reason: String,
    pub operation_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComputerMigrationPhase {
    BackingUp,
    Creating,
    Moving,
    CleaningUp,
    Wiping,
    Done,
    Failed,
}

impl ComputerMigrationPhase {
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::BackingUp => "backing-up",
            Self::Creating => "creating",
            Self::Moving => "moving",
            Self::CleaningUp => "cleaning-up",
            Self::Wiping => "wiping",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }

    fn from_proto(value: u64) -> Result<Self, String> {
        match value {
            1 => Ok(Self::BackingUp),
            2 => Ok(Self::Creating),
            3 => Ok(Self::Moving),
            4 => Ok(Self::CleaningUp),
            5 => Ok(Self::Wiping),
            6 => Ok(Self::Done),
            7 => Ok(Self::Failed),
            _ => Err("GrokBotService returned an unsupported migration phase".into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputerMigrationEvent {
    pub operation_id: Option<String>,
    pub phase: ComputerMigrationPhase,
    pub detail: String,
    pub at_ms: u64,
    pub offset_key: String,
}

/// Host-owned client for the Desktop GrokBotService computer-rebuild backend.
///
/// This is intentionally separate from the Fabushi /v1/computers control plane. Credentials come
/// only from the dedicated Sand renewal credential provider already used by the production Host;
/// pairing clientToken/mobileToken/deviceSecret values never enter this client.
pub struct AndroidBoxRebuildBackend {
    credentials: ProcessSandMcpCredentialProvider,
    privacy_backend: CursorDashboardMcpAuthBackend,
}

impl AndroidBoxRebuildBackend {
    pub fn from_process_environment(machine_id: impl Into<String>) -> Result<Self, String> {
        let machine_id = machine_id.into();
        Ok(Self {
            credentials: ProcessSandMcpCredentialProvider::from_environment(machine_id.clone())?,
            privacy_backend: CursorDashboardMcpAuthBackend::from_process_environment(machine_id)?,
        })
    }

    pub fn recreate(
        &self,
        request_id: &str,
        preserve_data: bool,
        force: bool,
    ) -> Result<ComputerRecreateReply, String> {
        let mut body = Vec::new();
        encode_bool(1, preserve_data, &mut body);
        encode_bool(2, force, &mut body);
        let response = self.send_unary(
            GROK_BOT_RECREATE_PATH,
            &body,
            request_id,
            CONTROL_RPC_TIMEOUT_MS,
        )?;
        decode_recreate_response(&response)
    }

    pub fn force_recreate(&self, request_id: &str) -> Result<ComputerRecreateReply, String> {
        let response = self.send_unary(
            GROK_BOT_FORCE_RECREATE_PATH,
            &[],
            request_id,
            CONTROL_RPC_TIMEOUT_MS,
        )?;
        decode_recreate_response(&response)
    }

    /// Attach to the canonical migration stream and return the next event.
    ///
    /// A 30-second read timeout is the Android equivalent of Desktop's stall watchdog aborting the
    /// current stream attempt. The Coordinator owns the 3-second reconnect and durable offset.
    pub fn watch_migration_once(
        &self,
        request_id: &str,
        from_offset_key: &str,
    ) -> Result<ComputerMigrationEvent, String> {
        validate_request_id(request_id)?;
        validate_offset_key(from_offset_key)?;
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.ghost_mode();
        let mut proto = Vec::new();
        if !from_offset_key.is_empty() {
            encode_string(1, from_offset_key, &mut proto);
        }
        encode_bool(2, true, &mut proto);
        let body = connect_stream_frame(&proto);
        let response = self
            .request(
                &credentials,
                GROK_BOT_WATCH_MIGRATION_PATH,
                request_id,
                ghost_mode,
                MIGRATION_WATCH_STALL_MS,
                "application/connect+proto",
            )?
            .send_bytes(&body)
            .map_err(|error| format!("GrokBotService migration watch failed: {error}"))?;
        let mut reader = response.into_reader();
        let payload = read_connect_message(&mut reader)?;
        decode_migration_event(&payload)
    }

    fn send_unary(
        &self,
        path: &str,
        body: &[u8],
        request_id: &str,
        timeout_ms: u64,
    ) -> Result<Vec<u8>, String> {
        validate_request_id(request_id)?;
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.ghost_mode();
        let response = self
            .request(
                &credentials,
                path,
                request_id,
                ghost_mode,
                timeout_ms,
                "application/proto",
            )?
            .send_bytes(body)
            .map_err(|error| format!("GrokBotService RPC failed: {error}"))?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_RPC_RESPONSE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("GrokBotService response read failed: {error}"))?;
        if bytes.len() > MAX_RPC_RESPONSE_BYTES {
            return Err("GrokBotService response exceeds bounded size".into());
        }
        Ok(bytes)
    }

    fn request(
        &self,
        credentials: &crate::mcp_auth::SandMcpBackendCredentials,
        path: &str,
        request_id: &str,
        ghost_mode: &'static str,
        timeout_ms: u64,
        content_type: &str,
    ) -> Result<ureq::Request, String> {
        let base = Url::parse(&credentials.backend_url)
            .map_err(|error| format!("invalid Sand backend URL: {error}"))?;
        let url = base
            .join(path)
            .map_err(|error| format!("invalid GrokBotService RPC URL: {error}"))?;
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(timeout.min(Duration::from_secs(10)))
            .timeout_read(timeout)
            .timeout_write(timeout.min(Duration::from_secs(20)))
            .redirects(0)
            .build();
        let authorization = format!("Bearer {}", credentials.access_token);
        let checksum = create_cursor_checksum(&credentials.machine_id, system_now_ms());
        let mut request = agent
            .post(url.as_str())
            .set("content-type", content_type)
            .set("connect-protocol-version", "1")
            .set("authorization", &authorization)
            .set("x-cursor-checksum", &checksum)
            .set("x-cursor-client-type", "sand")
            .set("x-cursor-client-version", &credentials.client_version)
            .set("x-sand-box-namespace", &credentials.box_namespace)
            .set("x-ghost-mode", ghost_mode)
            .set("x-request-id", request_id);
        if env::var("CURSOR_AGENT_CLI_LOCAL_MODE").ok().as_deref() == Some("true") {
            request = request.set("local-cli-mode", "true");
        }
        Ok(request)
    }

    fn ghost_mode(&self) -> &'static str {
        match self.privacy_backend.resolve_sand_privacy_mode() {
            Some(SandPrivacyMode::UsageDataTrainingAllowed)
            | Some(SandPrivacyMode::UsageCodebaseTrainingAllowed) => "false",
            _ => "true",
        }
    }
}

fn validate_request_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > MAX_REQUEST_ID_BYTES
        || value.chars().any(|character| character.is_control())
    {
        return Err("computer rebuild request identity is invalid".into());
    }
    Ok(())
}

fn validate_offset_key(value: &str) -> Result<(), String> {
    if value.len() > MAX_OFFSET_KEY_BYTES || value.chars().any(|character| character.is_control()) {
        return Err("computer rebuild migration offset is invalid".into());
    }
    Ok(())
}

fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn encode_key(field: u64, wire: u64, output: &mut Vec<u8>) {
    encode_varint((field << 3) | wire, output);
}

fn encode_varint(mut value: u64, output: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn encode_bool(field: u64, value: bool, output: &mut Vec<u8>) {
    if !value {
        return;
    }
    encode_key(field, 0, output);
    output.push(1);
}

fn encode_string(field: u64, value: &str, output: &mut Vec<u8>) {
    if value.is_empty() {
        return;
    }
    encode_key(field, 2, output);
    encode_varint(value.len() as u64, output);
    output.extend_from_slice(value.as_bytes());
}

fn connect_stream_frame(message: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(message.len() + 5);
    output.push(0);
    output.extend_from_slice(&(message.len() as u32).to_be_bytes());
    output.extend_from_slice(message);
    output
}

fn read_connect_message(reader: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut header = [0_u8; 5];
    reader
        .read_exact(&mut header)
        .map_err(|error| format!("migration stream stalled or closed before an event: {error}"))?;
    let flags = header[0];
    let len = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;
    if len > MAX_RPC_RESPONSE_BYTES {
        return Err("migration stream frame exceeds bounded size".into());
    }
    let mut payload = vec![0_u8; len];
    reader
        .read_exact(&mut payload)
        .map_err(|error| format!("migration stream frame was truncated: {error}"))?;
    if flags & 0x02 != 0 {
        let detail = String::from_utf8_lossy(&payload);
        return Err(format!("migration stream ended before an event: {detail}"));
    }
    if flags != 0 {
        return Err("compressed or reserved migration stream frame is unsupported".into());
    }
    Ok(payload)
}

fn decode_recreate_response(input: &[u8]) -> Result<ComputerRecreateReply, String> {
    let mut cursor = 0usize;
    let mut started = false;
    let mut reason = String::new();
    let mut operation_id = None;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 0x07) as u8;
        match (field, wire) {
            (1, 0) => started = read_varint(input, &mut cursor)? != 0,
            (2, 2) => reason = read_string(input, &mut cursor)?,
            (3, 2) => {
                operation_id = read_string(input, &mut cursor)?
                    .trim()
                    .to_string()
                    .into();
            }
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    let operation_id = operation_id.filter(|value: &String| !value.is_empty());
    Ok(ComputerRecreateReply {
        started,
        reason,
        operation_id,
    })
}

fn decode_migration_event(input: &[u8]) -> Result<ComputerMigrationEvent, String> {
    let mut cursor = 0usize;
    let mut phase = None;
    let mut detail = String::new();
    let mut at_ms = 0_u64;
    let mut offset_key = String::new();
    let mut operation_id = None;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 0x07) as u8;
        match (field, wire) {
            (1, 0) => phase = Some(ComputerMigrationPhase::from_proto(read_varint(input, &mut cursor)?)?),
            (2, 2) => detail = read_string(input, &mut cursor)?,
            (3, 0) => at_ms = read_varint(input, &mut cursor)?,
            (4, 2) => offset_key = read_string(input, &mut cursor)?,
            (5, 2) => {
                operation_id = read_string(input, &mut cursor)?
                    .trim()
                    .to_string()
                    .into();
            }
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    validate_offset_key(&offset_key)?;
    Ok(ComputerMigrationEvent {
        operation_id: operation_id.filter(|value: &String| !value.is_empty()),
        phase: phase.ok_or("migration event omitted phase")?,
        detail,
        at_ms,
        offset_key,
    })
}

fn read_varint(input: &[u8], cursor: &mut usize) -> Result<u64, String> {
    let mut value = 0_u64;
    for shift in (0..70).step_by(7) {
        let byte = *input.get(*cursor).ok_or("truncated protobuf varint")?;
        *cursor += 1;
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err("malformed protobuf varint".into())
}

fn read_len_delimited<'a>(input: &'a [u8], cursor: &mut usize) -> Result<&'a [u8], String> {
    let len = read_varint(input, cursor)? as usize;
    let end = cursor.checked_add(len).ok_or("protobuf length overflow")?;
    let value = input.get(*cursor..end).ok_or("truncated protobuf field")?;
    *cursor = end;
    Ok(value)
}

fn read_string(input: &[u8], cursor: &mut usize) -> Result<String, String> {
    String::from_utf8(read_len_delimited(input, cursor)?.to_vec())
        .map_err(|_| "GrokBotService protobuf string is not UTF-8".into())
}

fn skip_field(input: &[u8], cursor: &mut usize, wire: u8) -> Result<(), String> {
    match wire {
        0 => {
            let _ = read_varint(input, cursor)?;
        }
        1 => {
            *cursor = cursor.checked_add(8).ok_or("protobuf fixed64 overflow")?;
            if *cursor > input.len() {
                return Err("truncated protobuf fixed64".into());
            }
        }
        2 => {
            let _ = read_len_delimited(input, cursor)?;
        }
        5 => {
            *cursor = cursor.checked_add(4).ok_or("protobuf fixed32 overflow")?;
            if *cursor > input.len() {
                return Err("truncated protobuf fixed32".into());
            }
        }
        _ => return Err("unsupported protobuf wire type".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn recreate_request_matches_desktop_proto_fields() {
        let mut update = Vec::new();
        encode_bool(1, true, &mut update);
        encode_bool(2, false, &mut update);
        assert_eq!(update, vec![0x08, 0x01]);

        let mut reset = Vec::new();
        encode_bool(1, false, &mut reset);
        encode_bool(2, true, &mut reset);
        assert_eq!(reset, vec![0x10, 0x01]);
    }

    #[test]
    fn recreate_response_requires_no_fabricated_operation_identity() {
        let response = vec![
            0x08, 0x01,
            0x12, 0x00,
            0x1a, 0x04, b'o', b'p', b'-', b'7',
        ];
        assert_eq!(
            decode_recreate_response(&response).unwrap(),
            ComputerRecreateReply {
                started: true,
                reason: String::new(),
                operation_id: Some("op-7".into()),
            }
        );
        assert_eq!(
            decode_recreate_response(&[0x08, 0x01]).unwrap().operation_id,
            None
        );
    }

    #[test]
    fn migration_event_decodes_operation_phase_offset_and_detail() {
        let mut payload = Vec::new();
        encode_key(1, 0, &mut payload);
        encode_varint(2, &mut payload);
        encode_string(2, "creating", &mut payload);
        encode_key(3, 0, &mut payload);
        encode_varint(77, &mut payload);
        encode_string(4, "offset-8", &mut payload);
        encode_string(5, "operation-9", &mut payload);
        let event = decode_migration_event(&payload).unwrap();
        assert_eq!(event.phase, ComputerMigrationPhase::Creating);
        assert_eq!(event.detail, "creating");
        assert_eq!(event.at_ms, 77);
        assert_eq!(event.offset_key, "offset-8");
        assert_eq!(event.operation_id.as_deref(), Some("operation-9"));
    }

    #[test]
    fn connect_stream_frame_is_five_byte_big_endian_envelope() {
        let framed = connect_stream_frame(&[1, 2, 3]);
        assert_eq!(framed, vec![0, 0, 0, 0, 3, 1, 2, 3]);
        assert_eq!(
            read_connect_message(&mut Cursor::new(framed)).unwrap(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn connect_stream_end_or_compression_fails_closed() {
        let end = vec![2, 0, 0, 0, 2, b'{', b'}'];
        assert!(read_connect_message(&mut Cursor::new(end))
            .unwrap_err()
            .contains("ended before an event"));
        let compressed = vec![1, 0, 0, 0, 0];
        assert!(read_connect_message(&mut Cursor::new(compressed))
            .unwrap_err()
            .contains("compressed"));
    }

    #[test]
    fn request_and_offset_identity_reject_control_characters() {
        assert!(validate_request_id("rebuild-1").is_ok());
        assert!(validate_request_id("bad\nrequest").is_err());
        assert!(validate_offset_key("offset-1").is_ok());
        assert!(validate_offset_key("bad\roffset").is_err());
    }
}
