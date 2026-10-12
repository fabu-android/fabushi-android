#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionTarget {
    AndroidLocal,
    RemoteBox,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionCapability {
    pub id: String,
    pub target: ExecutionTarget,
    pub cancellable: bool,
    pub supports_streaming: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionRequest {
    pub operation_id: String,
    pub capability_id: String,
    /// Typed adapters own interpretation. The shared layer preserves only the wire payload.
    pub input_json: String,
    pub timeout_ms: u64,
}

impl ExecutionRequest {
    pub fn validate(&self) -> Result<(), ExecutionError> {
        if self.operation_id.trim().is_empty() {
            return Err(ExecutionError::InvalidRequest(
                "operation_id must not be empty".into(),
            ));
        }
        if self.capability_id.trim().is_empty() {
            return Err(ExecutionError::InvalidRequest(
                "capability_id must not be empty".into(),
            ));
        }
        if self.timeout_ms == 0 {
            return Err(ExecutionError::InvalidRequest(
                "timeout_ms must be greater than zero".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionResult {
    pub operation_id: String,
    pub output_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionError {
    InvalidRequest(String),
    CapabilityUnavailable(String),
    Cancelled,
    TimedOut,
    Transport(String),
}
