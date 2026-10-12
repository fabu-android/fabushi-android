// Direct Rust-to-Rust Android port from bhrumom/fabushi-desktop@371b72f1cd20de03dc4dff9a15ed761054968093.
// This module is platform-neutral protocol/state logic; Desktop process spawning is intentionally not copied.
use std::collections::BTreeMap;

use serde_json::{Map, Value, json};
use thiserror::Error;

pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct CsnapsProtocolError {
    message: String,
}

impl CsnapsProtocolError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CsnapsOperation {
    Ping,
    Shutdown,
    Initialize(Map<String, Value>),
    ApplyCodebaseSpecs(Map<String, Value>),
    Snapshot(Map<String, Value>),
    TriggerUpload(Map<String, Value>),
    FlushPendingUploads(Map<String, Value>),
}

impl CsnapsOperation {
    pub fn method(&self) -> &'static str {
        match self {
            Self::Ping => "ping",
            Self::Shutdown => "shutdown",
            Self::Initialize(_) => "initialize",
            Self::ApplyCodebaseSpecs(_) => "apply_codebase_specs",
            Self::Snapshot(_) => "snapshot",
            Self::TriggerUpload(_) => "trigger_upload",
            Self::FlushPendingUploads(_) => "flush_pending_uploads",
        }
    }

    pub fn params(&self) -> Map<String, Value> {
        match self {
            Self::Ping | Self::Shutdown => Map::new(),
            Self::Initialize(value)
            | Self::ApplyCodebaseSpecs(value)
            | Self::Snapshot(value)
            | Self::TriggerUpload(value)
            | Self::FlushPendingUploads(value) => value.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CsnapsResponse {
    Success { id: u64, result: Value },
    Error { id: u64, error: CsnapsErrorCode },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsnapsErrorCode {
    UnknownMethod,
    InvalidParams,
    AlreadyInitialized,
    NotInitialized,
    CodebasesNotReconciled,
    ShuttingDown,
    ServerBusy,
    InvalidConfiguration,
    InvalidCodebases,
    CodebaseEnvironmentConflict,
    CorruptUuidState,
    StateIo,
    SnapshotFailed,
    UploadDisabled,
    InternalError,
}

impl CsnapsErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnknownMethod => "unknown_method",
            Self::InvalidParams => "invalid_params",
            Self::AlreadyInitialized => "already_initialized",
            Self::NotInitialized => "not_initialized",
            Self::CodebasesNotReconciled => "codebases_not_reconciled",
            Self::ShuttingDown => "shutting_down",
            Self::ServerBusy => "server_busy",
            Self::InvalidConfiguration => "invalid_configuration",
            Self::InvalidCodebases => "invalid_codebases",
            Self::CodebaseEnvironmentConflict => "codebase_environment_conflict",
            Self::CorruptUuidState => "corrupt_uuid_state",
            Self::StateIo => "state_io",
            Self::SnapshotFailed => "snapshot_failed",
            Self::UploadDisabled => "upload_disabled",
            Self::InternalError => "internal_error",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "unknown_method" => Self::UnknownMethod,
            "invalid_params" => Self::InvalidParams,
            "already_initialized" => Self::AlreadyInitialized,
            "not_initialized" => Self::NotInitialized,
            "codebases_not_reconciled" => Self::CodebasesNotReconciled,
            "shutting_down" => Self::ShuttingDown,
            "server_busy" => Self::ServerBusy,
            "invalid_configuration" => Self::InvalidConfiguration,
            "invalid_codebases" => Self::InvalidCodebases,
            "codebase_environment_conflict" => Self::CodebaseEnvironmentConflict,
            "corrupt_uuid_state" => Self::CorruptUuidState,
            "state_io" => Self::StateIo,
            "snapshot_failed" => Self::SnapshotFailed,
            "upload_disabled" => Self::UploadDisabled,
            "internal_error" => Self::InternalError,
            _ => return None,
        })
    }
}

pub fn encode_frame(payload: &[u8]) -> Result<Vec<u8>, CsnapsProtocolError> {
    if payload.len() > MAX_FRAME_BYTES {
        return Err(CsnapsProtocolError::new(format!(
            "csnaps request frame length {} exceeds {MAX_FRAME_BYTES}-byte limit",
            payload.len()
        )));
    }
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

pub fn decode_frame_payloads(
    input: &[u8],
) -> Result<(Vec<Vec<u8>>, Vec<u8>), CsnapsProtocolError> {
    let mut payloads = Vec::new();
    let mut offset = 0usize;
    while input.len().saturating_sub(offset) >= 4 {
        let length = u32::from_be_bytes([
            input[offset],
            input[offset + 1],
            input[offset + 2],
            input[offset + 3],
        ]) as usize;
        if length > MAX_FRAME_BYTES {
            return Err(CsnapsProtocolError::new(format!(
                "csnaps response frame length {length} exceeds {MAX_FRAME_BYTES}-byte limit"
            )));
        }
        if input.len().saturating_sub(offset) < 4 + length {
            break;
        }
        payloads.push(input[offset + 4..offset + 4 + length].to_vec());
        offset += 4 + length;
    }
    Ok((payloads, input[offset..].to_vec()))
}

pub fn encode_csnaps_request(
    id: u64,
    operation: &CsnapsOperation,
) -> Result<Vec<u8>, CsnapsProtocolError> {
    // JS Number.isSafeInteger parity: the frozen protocol never emits an id
    // that cannot round-trip through IEEE-754 integer precision.
    if id > 9_007_199_254_740_991 {
        return Err(CsnapsProtocolError::new("invalid csnaps request ID"));
    }
    let payload = serde_json::to_vec(&json!({
        "id": id,
        "method": operation.method(),
        "params": operation.params(),
    }))
    .map_err(|error| CsnapsProtocolError::new(format!(
        "could not encode csnaps request: {error}"
    )))?;
    encode_frame(&payload)
}

pub fn parse_csnaps_response(payload: &[u8]) -> Result<CsnapsResponse, CsnapsProtocolError> {
    let value: Value = serde_json::from_slice(payload)
        .map_err(|_| CsnapsProtocolError::new("invalid csnaps response JSON"))?;
    let object = value.as_object().ok_or_else(|| {
        CsnapsProtocolError::new("invalid csnaps response envelope")
    })?;
    let id = safe_non_negative_integer(object.get("id")).ok_or_else(|| {
        CsnapsProtocolError::new("invalid csnaps response envelope")
    })?;
    let ok = object.get("ok").and_then(Value::as_bool).ok_or_else(|| {
        CsnapsProtocolError::new("invalid csnaps response envelope")
    })?;
    if ok {
        if !has_exact_keys(object, &["id", "ok", "result"]) {
            return Err(CsnapsProtocolError::new(
                "invalid csnaps success response",
            ));
        }
        return Ok(CsnapsResponse::Success {
            id,
            result: object.get("result").cloned().unwrap_or(Value::Null),
        });
    }
    if !has_exact_keys(object, &["error", "id", "ok"]) {
        return Err(CsnapsProtocolError::new(
            "invalid csnaps error response",
        ));
    }
    let error = object
        .get("error")
        .and_then(Value::as_str)
        .and_then(CsnapsErrorCode::parse)
        .ok_or_else(|| CsnapsProtocolError::new("invalid csnaps error response"))?;
    Ok(CsnapsResponse::Error { id, error })
}

#[derive(Debug, Default)]
pub struct CsnapsResponseDecoder {
    buffer: Vec<u8>,
}

impl CsnapsResponseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn decode(
        &mut self,
        chunk: &[u8],
    ) -> Result<Vec<CsnapsResponse>, CsnapsProtocolError> {
        let mut input = Vec::with_capacity(self.buffer.len() + chunk.len());
        input.extend_from_slice(&self.buffer);
        input.extend_from_slice(chunk);
        let (payloads, remainder) = decode_frame_payloads(&input)?;
        self.buffer = remainder;
        payloads
            .iter()
            .map(|payload| parse_csnaps_response(payload))
            .collect()
    }

    pub fn assert_complete(&self) -> Result<(), CsnapsProtocolError> {
        if self.buffer.is_empty() {
            Ok(())
        } else {
            Err(CsnapsProtocolError::new(
                "truncated csnaps response frame",
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CsnapsTrackedCodebase {
    pub codebase_uuid: String,
    pub path: String,
    pub kind: f64,
    pub environment: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CsnapsRejectedCodebase {
    pub path: String,
    pub kind: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CsnapsServiceState {
    pub tracked: Vec<CsnapsTrackedCodebase>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CsnapsOperationResult {
    Empty,
    Initialize { state: CsnapsServiceState },
    ApplyCodebaseSpecs {
        state: CsnapsServiceState,
        rejected: Vec<CsnapsRejectedCodebase>,
    },
}

pub fn parse_csnaps_operation_result(
    operation: &CsnapsOperation,
    value: &Value,
) -> Result<CsnapsOperationResult, CsnapsProtocolError> {
    match operation {
        CsnapsOperation::Ping
        | CsnapsOperation::Snapshot(_)
        | CsnapsOperation::TriggerUpload(_)
        | CsnapsOperation::FlushPendingUploads(_)
        | CsnapsOperation::Shutdown => {
            let object = value.as_object().ok_or_else(|| {
                CsnapsProtocolError::new("invalid empty csnaps result")
            })?;
            if !object.is_empty() {
                return Err(CsnapsProtocolError::new(
                    "invalid empty csnaps result",
                ));
            }
            Ok(CsnapsOperationResult::Empty)
        }
        CsnapsOperation::Initialize(_) => {
            let object = value.as_object().ok_or_else(|| {
                CsnapsProtocolError::new("invalid csnaps initialize result")
            })?;
            if !has_exact_keys(object, &["state"]) {
                return Err(CsnapsProtocolError::new(
                    "invalid csnaps initialize result",
                ));
            }
            Ok(CsnapsOperationResult::Initialize {
                state: parse_state(object.get("state").unwrap())?,
            })
        }
        CsnapsOperation::ApplyCodebaseSpecs(_) => {
            let object = value.as_object().ok_or_else(|| {
                CsnapsProtocolError::new("invalid csnaps apply result")
            })?;
            if !has_exact_keys(object, &["rejected", "state"]) {
                return Err(CsnapsProtocolError::new(
                    "invalid csnaps apply result",
                ));
            }
            let rejected = object
                .get("rejected")
                .and_then(Value::as_array)
                .ok_or_else(|| CsnapsProtocolError::new("invalid csnaps apply result"))?
                .iter()
                .map(parse_rejected)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(CsnapsOperationResult::ApplyCodebaseSpecs {
                state: parse_state(object.get("state").unwrap())?,
                rejected,
            })
        }
    }
}

fn parse_state(value: &Value) -> Result<CsnapsServiceState, CsnapsProtocolError> {
    let object = value.as_object().ok_or_else(|| {
        CsnapsProtocolError::new("invalid csnaps service state")
    })?;
    if !has_exact_keys(object, &["tracked"]) {
        return Err(CsnapsProtocolError::new(
            "invalid csnaps service state",
        ));
    }
    let tracked = object
        .get("tracked")
        .and_then(Value::as_array)
        .ok_or_else(|| CsnapsProtocolError::new("invalid csnaps service state"))?
        .iter()
        .map(parse_tracked)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CsnapsServiceState { tracked })
}

fn parse_tracked(value: &Value) -> Result<CsnapsTrackedCodebase, CsnapsProtocolError> {
    let object = value.as_object().ok_or_else(|| {
        CsnapsProtocolError::new("invalid tracked csnaps codebase")
    })?;
    if !has_exact_keys(object, &["codebaseUuid", "environment", "kind", "path"]) {
        return Err(CsnapsProtocolError::new(
            "invalid tracked csnaps codebase",
        ));
    }
    let codebase_uuid = object
        .get("codebaseUuid")
        .and_then(Value::as_str)
        .filter(|value| is_uuid_v4(value))
        .ok_or_else(|| CsnapsProtocolError::new("invalid tracked csnaps codebase"))?
        .to_string();
    let path = object
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| CsnapsProtocolError::new("invalid tracked csnaps codebase"))?
        .to_string();
    let kind = number(object.get("kind"))
        .ok_or_else(|| CsnapsProtocolError::new("invalid tracked csnaps codebase"))?;
    let environment = number(object.get("environment"))
        .ok_or_else(|| CsnapsProtocolError::new("invalid tracked csnaps codebase"))?;
    Ok(CsnapsTrackedCodebase {
        codebase_uuid,
        path,
        kind,
        environment,
    })
}

fn parse_rejected(value: &Value) -> Result<CsnapsRejectedCodebase, CsnapsProtocolError> {
    let object = value.as_object().ok_or_else(|| {
        CsnapsProtocolError::new("invalid rejected csnaps codebase")
    })?;
    if !has_exact_keys(object, &["kind", "path"]) {
        return Err(CsnapsProtocolError::new(
            "invalid rejected csnaps codebase",
        ));
    }
    let path = object
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| CsnapsProtocolError::new("invalid rejected csnaps codebase"))?
        .to_string();
    let kind = number(object.get("kind"))
        .ok_or_else(|| CsnapsProtocolError::new("invalid rejected csnaps codebase"))?;
    Ok(CsnapsRejectedCodebase { path, kind })
}

fn has_exact_keys(object: &Map<String, Value>, keys: &[&str]) -> bool {
    if object.len() != keys.len() {
        return false;
    }
    keys.iter().all(|key| object.contains_key(*key))
}

fn safe_non_negative_integer(value: Option<&Value>) -> Option<u64> {
    let number = value?.as_f64()?;
    if !number.is_finite()
        || number < 0.0
        || number.fract() != 0.0
        || number > 9_007_199_254_740_991.0
    {
        return None;
    }
    Some(number as u64)
}

fn number(value: Option<&Value>) -> Option<f64> {
    value?.as_f64()
}

fn is_uuid_v4(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36
        || bytes[8] != b'-'
        || bytes[13] != b'-'
        || bytes[18] != b'-'
        || bytes[23] != b'-'
        || bytes[14] != b'4'
        || !matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
    {
        return false;
    }
    bytes.iter().enumerate().all(|(index, byte)| {
        matches!(index, 8 | 13 | 18 | 23)
            || byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()
    })
}

#[allow(dead_code)]
fn _ordered_map_anchor() -> BTreeMap<String, Value> {
    BTreeMap::new()
}
