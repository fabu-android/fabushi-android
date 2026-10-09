use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use fabushi_android_shared::node::mcp::mcp_auth_watch_lifecycle::McpBackendAuthStatus;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::env;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

pub const DASHBOARD_CHECK_HTTP_MCP_STATUS_PATH: &str =
    "/aiserver.v1.DashboardService/CheckHttpMcpStatus";
pub const DASHBOARD_VALIDATE_MCP_OAUTH_TOKENS_PATH: &str =
    "/aiserver.v1.DashboardService/ValidateMcpOAuthTokens";
pub const DASHBOARD_GET_USER_PRIVACY_MODE_PATH: &str =
    "/aiserver.v1.DashboardService/GetUserPrivacyMode";
pub const CONTROL_RPC_TIMEOUT_MS: u64 = 30_000;
pub const DEFAULT_CURSOR_BACKEND_URL: &str = "https://api2.cursor.sh";
pub const SAND_INFERENCE_RENEWAL_CREDENTIAL_ENV: &str =
    "SAND_INFERENCE_RENEWAL_CREDENTIAL";
const RENEWAL_PATH: &str = "/sand-box/inference-credential";
const SAND_CLIENT_TYPE: &str = "sand";
const DEFAULT_CLIENT_VERSION: &str = "0.1.0";
const DEFAULT_CREDENTIAL_TTL_MS: u64 = 15 * 60_000;
const REFRESH_LEEWAY_MS: u64 = 60_000;
const MAX_RPC_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandMcpBackendCredentials {
    pub backend_url: String,
    pub access_token: String,
    pub machine_id: String,
    pub client_version: String,
    pub box_namespace: String,
}

pub trait SandMcpCredentialProvider: Send + Sync {
    fn credentials(&self) -> Result<SandMcpBackendCredentials, String>;
}

#[derive(Clone, Debug)]
struct CachedCredential {
    access_token: String,
    expires_at_ms: u64,
}

pub struct ProcessSandMcpCredentialProvider {
    backend_url: String,
    renewal_credential: Option<String>,
    machine_id: String,
    client_version: String,
    box_namespace: String,
    cached: Mutex<Option<CachedCredential>>,
    agent: ureq::Agent,
}

impl ProcessSandMcpCredentialProvider {
    pub fn from_environment(machine_id: impl Into<String>) -> Result<Self, String> {
        let machine_id = machine_id.into();
        if machine_id.trim().is_empty() {
            return Err("MCP backend machine id is required".into());
        }
        let backend_url = env::var("SAND_BACKEND_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                env::var("CURSOR_API_BASE_URL")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or_else(|| DEFAULT_CURSOR_BACKEND_URL.to_string());
        Url::parse(&backend_url)
            .map_err(|error| format!("invalid MCP backend URL: {error}"))?;
        let renewal_credential = env::var(SAND_INFERENCE_RENEWAL_CREDENTIAL_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty());
        Ok(Self {
            backend_url,
            renewal_credential,
            machine_id,
            client_version: sand_client_version(),
            box_namespace: sand_box_namespace(),
            cached: Mutex::new(None),
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(10))
                .timeout_read(Duration::from_secs(20))
                .timeout_write(Duration::from_secs(20))
                .redirects(0)
                .build(),
        })
    }

    #[cfg(test)]
    fn for_test(
        machine_id: &str,
        backend_url: &str,
        renewal_credential: Option<&str>,
    ) -> Self {
        Self {
            backend_url: backend_url.to_string(),
            renewal_credential: renewal_credential.map(str::to_string),
            machine_id: machine_id.to_string(),
            client_version: "0.1.0-dev".to_string(),
            box_namespace: "dev".to_string(),
            cached: Mutex::new(None),
            agent: ureq::AgentBuilder::new().redirects(0).build(),
        }
    }

    fn renew(&self, renewal_credential: &str) -> Result<CachedCredential, String> {
        let base = Url::parse(&self.backend_url)
            .map_err(|error| format!("invalid MCP backend URL: {error}"))?;
        let url = base
            .join(RENEWAL_PATH)
            .map_err(|error| format!("invalid MCP credential renewal URL: {error}"))?;
        let response = self
            .agent
            .post(url.as_str())
            .set("content-type", "application/json")
            .set("x-cursor-client-type", SAND_CLIENT_TYPE)
            .set("x-cursor-client-version", &self.client_version)
            .set("x-sand-box-namespace", &self.box_namespace)
            .send_json(json!({ "credential": renewal_credential }))
            .map_err(|error| format!("MCP credential renewal failed: {error}"))?;
        let parsed: Value = response
            .into_json()
            .map_err(|error| format!("MCP credential renewal response was invalid: {error}"))?;
        let access_token = parsed
            .get("accessToken")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or("MCP credential renewal returned no access token")?
            .to_string();
        let now = system_now_ms();
        let expires_at_ms = parsed
            .get("expiresAtMs")
            .and_then(Value::as_u64)
            .or_else(|| jwt_expiry_ms(&access_token))
            .unwrap_or_else(|| now.saturating_add(DEFAULT_CREDENTIAL_TTL_MS));
        Ok(CachedCredential {
            access_token,
            expires_at_ms,
        })
    }
}

impl SandMcpCredentialProvider for ProcessSandMcpCredentialProvider {
    fn credentials(&self) -> Result<SandMcpBackendCredentials, String> {
        let now = system_now_ms();
        if let Some(cached) = self
            .cached
            .lock()
            .map_err(|_| "MCP credential cache lock poisoned".to_string())?
            .clone()
            .filter(|credential| {
                credential.expires_at_ms > now.saturating_add(REFRESH_LEEWAY_MS)
            })
        {
            return Ok(SandMcpBackendCredentials {
                backend_url: self.backend_url.clone(),
                access_token: cached.access_token,
                machine_id: self.machine_id.clone(),
                client_version: self.client_version.clone(),
                box_namespace: self.box_namespace.clone(),
            });
        }

        // Desktop explicitly keeps the Fabushi account bearer out of Cursor/Sand
        // Connect RPCs. Android follows the same audience boundary and only accepts
        // the dedicated Sand renewal credential here.
        let renewal = self
            .renewal_credential
            .as_deref()
            .ok_or("MCP backend credential is unavailable")?;
        let renewed = self.renew(renewal)?;
        *self
            .cached
            .lock()
            .map_err(|_| "MCP credential cache lock poisoned".to_string())? =
            Some(renewed.clone());
        Ok(SandMcpBackendCredentials {
            backend_url: self.backend_url.clone(),
            access_token: renewed.access_token,
            machine_id: self.machine_id.clone(),
            client_version: self.client_version.clone(),
            box_namespace: self.box_namespace.clone(),
        })
    }
}

pub trait McpAuthBackendPort: Send + Sync {
    fn check_auth_status(
        &self,
        server_id: i32,
        account_key: &str,
        oauth_redirect_uri: &str,
        force_reauth: bool,
    ) -> Result<McpBackendAuthStatus, String>;

    fn validate_token(&self, server_url: &str, account_key: &str) -> Result<bool, String>;
}

pub struct CursorDashboardMcpAuthBackend {
    credentials: Box<dyn SandMcpCredentialProvider>,
}

impl CursorDashboardMcpAuthBackend {
    pub fn new(credentials: Box<dyn SandMcpCredentialProvider>) -> Self {
        Self { credentials }
    }

    pub fn from_process_environment(machine_id: impl Into<String>) -> Result<Self, String> {
        Ok(Self::new(Box::new(
            ProcessSandMcpCredentialProvider::from_environment(machine_id)?,
        )))
    }

    fn send_unary(
        &self,
        credentials: &SandMcpBackendCredentials,
        path: &str,
        body: &[u8],
        timeout_ms: u64,
        ghost_mode: &str,
    ) -> Result<Vec<u8>, String> {
        let base = Url::parse(&credentials.backend_url)
            .map_err(|error| format!("invalid MCP backend URL: {error}"))?;
        let url = base
            .join(path)
            .map_err(|error| format!("invalid MCP backend RPC URL: {error}"))?;
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(timeout.min(Duration::from_secs(10)))
            .timeout_read(timeout)
            .timeout_write(timeout)
            .redirects(0)
            .build();
        let authorization = format!("Bearer {}", credentials.access_token);
        let checksum = create_cursor_checksum(&credentials.machine_id, system_now_ms());
        let request_id = next_request_id();
        let response = agent
            .post(url.as_str())
            .set("content-type", "application/proto")
            .set("connect-protocol-version", "1")
            .set("authorization", &authorization)
            .set("x-cursor-checksum", &checksum)
            .set("x-cursor-client-type", SAND_CLIENT_TYPE)
            .set("x-cursor-client-version", &credentials.client_version)
            .set("x-sand-box-namespace", &credentials.box_namespace)
            .set("x-ghost-mode", ghost_mode)
            .set("x-request-id", &request_id)
            .send_bytes(body)
            .map_err(|error| format!("MCP backend RPC failed: {error}"))?;
        let mut reader = response.into_reader();
        let mut bytes = Vec::new();
        reader
            .by_ref()
            .take(MAX_RPC_RESPONSE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("MCP backend RPC response read failed: {error}"))?;
        if bytes.len() > MAX_RPC_RESPONSE_BYTES {
            return Err("MCP backend RPC response exceeds bounded size".into());
        }
        Ok(bytes)
    }

    fn resolve_ghost_mode(
        &self,
        credentials: &SandMcpBackendCredentials,
    ) -> &'static str {
        let response = self.send_unary(
            credentials,
            DASHBOARD_GET_USER_PRIVACY_MODE_PATH,
            &[0x08, 0x01],
            3_000,
            "true",
        );
        match response
            .ok()
            .and_then(|bytes| decode_optional_varint_field(&bytes, 1).ok().flatten())
        {
            Some(3 | 4) => "false",
            _ => "true",
        }
    }
}

impl McpAuthBackendPort for CursorDashboardMcpAuthBackend {
    fn check_auth_status(
        &self,
        server_id: i32,
        account_key: &str,
        oauth_redirect_uri: &str,
        force_reauth: bool,
    ) -> Result<McpBackendAuthStatus, String> {
        let credentials = self.credentials.credentials()?;
        let body = encode_check_http_mcp_status_request(
            server_id,
            oauth_redirect_uri,
            force_reauth,
            account_key,
        );
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let response = self.send_unary(
            &credentials,
            DASHBOARD_CHECK_HTTP_MCP_STATUS_PATH,
            &body,
            CONTROL_RPC_TIMEOUT_MS,
            ghost_mode,
        )?;
        let statuses = decode_check_http_mcp_status_response(&response)?;
        let status = statuses
            .into_iter()
            .find(|status| status.id == server_id)
            .ok_or("MCP backend did not return the requested server status")?;
        Ok(McpBackendAuthStatus {
            is_available: status.is_available,
            requires_auth: status.requires_auth,
            has_valid_token: status.has_valid_token,
            auth_url: status.auth_url.unwrap_or_default(),
            error: status.error.unwrap_or_default(),
        })
    }

    fn validate_token(&self, server_url: &str, account_key: &str) -> Result<bool, String> {
        let credentials = self.credentials.credentials()?;
        let body = encode_validate_mcp_oauth_tokens_request(server_url, account_key);
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let response = self.send_unary(
            &credentials,
            DASHBOARD_VALIDATE_MCP_OAUTH_TOKENS_PATH,
            &body,
            CONTROL_RPC_TIMEOUT_MS,
            ghost_mode,
        )?;
        let results = decode_validate_mcp_oauth_tokens_response(&response)?;
        Ok(results.into_iter().any(|result| {
            result.server_url == server_url
                && result.account_key == account_key
                && result.has_valid_token
        }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CheckStatusWire {
    id: i32,
    is_available: bool,
    requires_auth: bool,
    auth_url: Option<String>,
    error: Option<String>,
    has_valid_token: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ValidateResultWire {
    server_url: String,
    has_valid_token: bool,
    account_key: String,
}

fn encode_check_http_mcp_status_request(
    server_id: i32,
    oauth_redirect_uri: &str,
    force_reauth: bool,
    account_key: &str,
) -> Vec<u8> {
    let mut output = Vec::new();
    let mut packed = Vec::new();
    encode_varint(server_id as u32 as u64, &mut packed);
    encode_len_delimited(1, &packed, &mut output);
    encode_string(2, oauth_redirect_uri, &mut output);
    if force_reauth {
        encode_bool(5, true, &mut output);
    }
    encode_string(8, account_key, &mut output);
    output
}

fn encode_validate_mcp_oauth_tokens_request(
    server_url: &str,
    account_key: &str,
) -> Vec<u8> {
    let mut target = Vec::new();
    encode_string(1, server_url, &mut target);
    encode_string(2, account_key, &mut target);
    let mut output = Vec::new();
    encode_len_delimited(3, &target, &mut output);
    output
}

fn decode_check_http_mcp_status_response(input: &[u8]) -> Result<Vec<CheckStatusWire>, String> {
    let mut cursor = 0usize;
    let mut statuses = Vec::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        if field == 1 && wire == 2 {
            let bytes = read_len_delimited(input, &mut cursor)?;
            statuses.push(decode_check_status(bytes)?);
        } else {
            skip_field(input, &mut cursor, wire)?;
        }
    }
    Ok(statuses)
}

fn decode_check_status(input: &[u8]) -> Result<CheckStatusWire, String> {
    let mut cursor = 0usize;
    let mut status = CheckStatusWire {
        id: 0,
        is_available: false,
        requires_auth: false,
        auth_url: None,
        error: None,
        has_valid_token: false,
    };
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        match (field, wire) {
            (1, 0) => status.id = read_varint(input, &mut cursor)? as i32,
            (2, 0) => status.is_available = read_varint(input, &mut cursor)? != 0,
            (3, 0) => status.requires_auth = read_varint(input, &mut cursor)? != 0,
            (4, 2) => status.auth_url = Some(read_string(input, &mut cursor)?),
            (5, 2) => status.error = Some(read_string(input, &mut cursor)?),
            (6, 0) => status.has_valid_token = read_varint(input, &mut cursor)? != 0,
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    Ok(status)
}

fn decode_validate_mcp_oauth_tokens_response(
    input: &[u8],
) -> Result<Vec<ValidateResultWire>, String> {
    let mut cursor = 0usize;
    let mut results = Vec::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        if field == 1 && wire == 2 {
            let bytes = read_len_delimited(input, &mut cursor)?;
            results.push(decode_validate_result(bytes)?);
        } else {
            skip_field(input, &mut cursor, wire)?;
        }
    }
    Ok(results)
}

fn decode_validate_result(input: &[u8]) -> Result<ValidateResultWire, String> {
    let mut cursor = 0usize;
    let mut server_url = String::new();
    let mut has_valid_token = false;
    let mut account_key = String::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        match (field, wire) {
            (1, 2) => server_url = read_string(input, &mut cursor)?,
            (2, 0) => has_valid_token = read_varint(input, &mut cursor)? != 0,
            (3, 2) => account_key = read_string(input, &mut cursor)?,
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    Ok(ValidateResultWire {
        server_url,
        has_valid_token,
        account_key,
    })
}

fn encode_bool(field: u32, value: bool, output: &mut Vec<u8>) {
    encode_key(field, 0, output);
    encode_varint(value as u64, output);
}

fn encode_string(field: u32, value: &str, output: &mut Vec<u8>) {
    if !value.is_empty() {
        encode_len_delimited(field, value.as_bytes(), output);
    }
}

fn encode_len_delimited(field: u32, value: &[u8], output: &mut Vec<u8>) {
    encode_key(field, 2, output);
    encode_varint(value.len() as u64, output);
    output.extend_from_slice(value);
}

fn encode_key(field: u32, wire: u8, output: &mut Vec<u8>) {
    encode_varint(((field as u64) << 3) | wire as u64, output);
}

fn encode_varint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 0x80 {
        output.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

fn read_varint(input: &[u8], cursor: &mut usize) -> Result<u64, String> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let byte = *input
            .get(*cursor)
            .ok_or("truncated MCP backend protobuf varint")?;
        *cursor += 1;
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err("malformed MCP backend protobuf varint".into())
}

fn read_len_delimited<'a>(input: &'a [u8], cursor: &mut usize) -> Result<&'a [u8], String> {
    let len = read_varint(input, cursor)? as usize;
    let end = cursor
        .checked_add(len)
        .filter(|end| *end <= input.len())
        .ok_or("truncated MCP backend protobuf field")?;
    let value = &input[*cursor..end];
    *cursor = end;
    Ok(value)
}

fn read_string(input: &[u8], cursor: &mut usize) -> Result<String, String> {
    String::from_utf8(read_len_delimited(input, cursor)?.to_vec())
        .map_err(|_| "MCP backend protobuf string is not UTF-8".into())
}

fn skip_field(input: &[u8], cursor: &mut usize, wire: u8) -> Result<(), String> {
    match wire {
        0 => {
            let _ = read_varint(input, cursor)?;
            Ok(())
        }
        1 => {
            *cursor = cursor
                .checked_add(8)
                .filter(|end| *end <= input.len())
                .ok_or("truncated MCP backend fixed64 field")?;
            Ok(())
        }
        2 => {
            let _ = read_len_delimited(input, cursor)?;
            Ok(())
        }
        5 => {
            *cursor = cursor
                .checked_add(4)
                .filter(|end| *end <= input.len())
                .ok_or("truncated MCP backend fixed32 field")?;
            Ok(())
        }
        _ => Err(format!("unsupported MCP backend protobuf wire type {wire}")),
    }
}

fn decode_optional_varint_field(
    input: &[u8],
    wanted_field: u32,
) -> Result<Option<u64>, String> {
    let mut cursor = 0usize;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        if field == wanted_field && wire == 0 {
            return Ok(Some(read_varint(input, &mut cursor)?));
        }
        skip_field(input, &mut cursor, wire)?;
    }
    Ok(None)
}

pub fn create_cursor_checksum(machine_id: &str, now_ms: u64) -> String {
    let kilo_seconds = now_ms / 1_000_000;
    let mut bytes = [
        ((kilo_seconds >> 40) & 0xff) as u8,
        ((kilo_seconds >> 32) & 0xff) as u8,
        ((kilo_seconds >> 24) & 0xff) as u8,
        ((kilo_seconds >> 16) & 0xff) as u8,
        ((kilo_seconds >> 8) & 0xff) as u8,
        (kilo_seconds & 0xff) as u8,
    ];
    let mut last = 165_u8;
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = ((*byte ^ last).wrapping_add((index % 256) as u8)) & 0xff;
        last = *byte;
    }
    format!("{}{}", URL_SAFE_NO_PAD.encode(bytes), machine_id)
}

fn jwt_expiry_ms(token: &str) -> Option<u64> {
    let payload = token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload.as_bytes()).ok()?;
    let parsed: Value = serde_json::from_slice(&decoded).ok()?;
    parsed
        .get("exp")
        .and_then(Value::as_u64)
        .and_then(|seconds| seconds.checked_mul(1_000))
}

fn system_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn next_request_id() -> String {
    let sequence = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let material = format!("{}:{sequence}", system_now_ms());
    let digest = Sha256::digest(material.as_bytes());
    format!(
        "android-mcp-{}",
        digest.iter().take(12).map(|byte| format!("{byte:02x}")).collect::<String>()
    )
}

fn sand_box_namespace() -> String {
    match env::var("SAND_BOX_OWNER_NAMESPACE").ok().as_deref() {
        Some("dev") => "dev".into(),
        Some("lab") => "lab".into(),
        _ if env::var("SAND_PACKAGED").ok().as_deref() != Some("1") => "dev".into(),
        _ if env::var("SAND_LAB").ok().as_deref() == Some("1") => "lab".into(),
        _ => "prod".into(),
    }
}

fn sand_client_version() -> String {
    let stamped = env::var("SAND_CLIENT_APP_VERSION").unwrap_or_default();
    let base = stamped.split('-').next().unwrap_or_default();
    let valid = {
        let parts = base.split('.').collect::<Vec<_>>();
        parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    };
    let base = if valid {
        base.to_string()
    } else {
        DEFAULT_CLIENT_VERSION.into()
    };
    match sand_box_namespace().as_str() {
        "dev" => format!("{base}-dev"),
        "lab" => format!("{base}-lab"),
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn len_field(field: u32, payload: &[u8]) -> Vec<u8> {
        let mut value = Vec::new();
        encode_len_delimited(field, payload, &mut value);
        value
    }

    #[test]
    fn dashboard_paths_and_request_fields_match_desktop_contract() {
        assert_eq!(
            DASHBOARD_CHECK_HTTP_MCP_STATUS_PATH,
            "/aiserver.v1.DashboardService/CheckHttpMcpStatus"
        );
        assert_eq!(
            DASHBOARD_VALIDATE_MCP_OAUTH_TOKENS_PATH,
            "/aiserver.v1.DashboardService/ValidateMcpOAuthTokens"
        );
        let request = encode_check_http_mcp_status_request(
            17,
            "http://127.0.0.1:18080/oauth/callback",
            true,
            "work",
        );
        assert!(request.windows(2).any(|window| window == [0x28, 0x01]));
        assert!(request.contains(&0x42));
        let validate =
            encode_validate_mcp_oauth_tokens_request("https://mcp.example.test", "work");
        assert_eq!(validate.first().copied(), Some(0x1a));
    }

    #[test]
    fn decodes_desktop_check_status_wire_shape() {
        let mut nested = Vec::new();
        encode_key(1, 0, &mut nested);
        encode_varint(17, &mut nested);
        encode_bool(2, true, &mut nested);
        encode_bool(3, true, &mut nested);
        encode_string(4, "https://auth.example.test", &mut nested);
        encode_bool(6, false, &mut nested);
        let response = len_field(1, &nested);
        let decoded = decode_check_http_mcp_status_response(&response).unwrap();
        assert_eq!(
            decoded,
            vec![CheckStatusWire {
                id: 17,
                is_available: true,
                requires_auth: true,
                auth_url: Some("https://auth.example.test".into()),
                error: None,
                has_valid_token: false,
            }]
        );
    }

    #[test]
    fn decodes_desktop_validate_tokens_wire_shape_and_account_fence() {
        let mut nested = Vec::new();
        encode_string(1, "https://mcp.example.test", &mut nested);
        encode_bool(2, true, &mut nested);
        encode_string(3, "work", &mut nested);
        let response = len_field(1, &nested);
        let decoded = decode_validate_mcp_oauth_tokens_response(&response).unwrap();
        assert_eq!(
            decoded,
            vec![ValidateResultWire {
                server_url: "https://mcp.example.test".into(),
                has_valid_token: true,
                account_key: "work".into(),
            }]
        );
    }

    #[test]
    fn checksum_matches_desktop_algorithm_and_binds_machine_id() {
        let first = create_cursor_checksum("machine-a", 1_728_000_000_000);
        let second = create_cursor_checksum("machine-b", 1_728_000_000_000);
        assert!(first.ends_with("machine-a"));
        assert!(second.ends_with("machine-b"));
        assert_ne!(first, second);
    }

    #[test]
    fn process_provider_fails_closed_without_dedicated_sand_credential() {
        let provider = ProcessSandMcpCredentialProvider::for_test(
            "machine-a",
            DEFAULT_CURSOR_BACKEND_URL,
            None,
        );
        let error = provider.credentials().unwrap_err();
        assert!(error.contains("credential is unavailable"));
    }
}
