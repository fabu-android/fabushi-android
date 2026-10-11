use std::fmt;

pub const COORDINATOR_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinatorRequest {
    pub protocol_version: u32,
    pub request_id: String,
    pub session_id: String,
    pub method: String,
    pub params_json: String,
    pub deadline_ms: Option<u64>,
}

impl CoordinatorRequest {
    pub fn validate(&self) -> Result<(), CoordinatorFailure> {
        if self.protocol_version != COORDINATOR_PROTOCOL_VERSION {
            return Err(CoordinatorFailure::new(
                CoordinatorFailureCode::ProtocolMismatch,
                format!(
                    "unsupported protocol version {}; expected {}",
                    self.protocol_version, COORDINATOR_PROTOCOL_VERSION
                ),
            ));
        }
        if self.request_id.trim().is_empty() || self.session_id.trim().is_empty() || self.method.trim().is_empty() {
            return Err(CoordinatorFailure::new(
                CoordinatorFailureCode::MalformedRequest,
                "request_id, session_id, and method must not be empty",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CancelRequest {
    pub request_id: String,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResyncRequest {
    pub generation: u64,
    pub after_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoordinatorFailureCode {
    ProtocolMismatch,
    ProtocolBreach,
    MalformedRequest,
    DuplicateRequest,
    UnknownRequest,
    Cancelled,
    HostUnavailable,
    HostCrashed,
    StaleGeneration,
    ReplayUnavailable,
    GatewayUnavailable,
    Internal,
}

impl fmt::Display for CoordinatorFailureCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ProtocolMismatch => "protocol-mismatch",
            Self::ProtocolBreach => "protocol-breach",
            Self::MalformedRequest => "malformed-request",
            Self::DuplicateRequest => "duplicate-request",
            Self::UnknownRequest => "unknown-request",
            Self::Cancelled => "cancelled",
            Self::HostUnavailable => "host-unavailable",
            Self::HostCrashed => "host-crashed",
            Self::StaleGeneration => "stale-generation",
            Self::ReplayUnavailable => "replay-unavailable",
            Self::GatewayUnavailable => "gateway-unavailable",
            Self::Internal => "internal",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinatorFailure {
    pub code: CoordinatorFailureCode,
    pub message: String,
}

impl CoordinatorFailure {
    pub fn new(code: CoordinatorFailureCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    pub fn protocol(message: impl Into<String>) -> Self {
        Self::new(CoordinatorFailureCode::ProtocolBreach, message)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinatorReply {
    pub request_id: String,
    pub result_json: Result<String, CoordinatorFailure>,
}

impl CoordinatorReply {
    pub fn ok(request_id: impl Into<String>, result_json: impl Into<String>) -> Self {
        Self { request_id: request_id.into(), result_json: Ok(result_json.into()) }
    }

    pub fn failed(request_id: impl Into<String>, failure: CoordinatorFailure) -> Self {
        Self { request_id: request_id.into(), result_json: Err(failure) }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinatorEvent {
    pub event_id: String,
    pub session_id: String,
    pub sequence: u64,
    pub family: String,
    pub payload_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResyncSnapshot {
    pub generation: u64,
    pub latest_sequence: u64,
    pub events: Vec<CoordinatorEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_protocol_mismatch() {
        let request = CoordinatorRequest {
            protocol_version: COORDINATOR_PROTOCOL_VERSION + 1,
            request_id: "r1".into(),
            session_id: "s1".into(),
            method: "send".into(),
            params_json: "{}".into(),
            deadline_ms: None,
        };
        assert_eq!(request.validate().unwrap_err().code, CoordinatorFailureCode::ProtocolMismatch);
    }
}
