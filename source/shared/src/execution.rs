const MAX_EXECUTION_ID_BYTES: usize = 256;
const MAX_EXECUTION_INPUT_JSON_BYTES: usize = 64 * 1024;

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
        validate_execution_identity("operation_id", &self.operation_id)?;
        validate_execution_identity("capability_id", &self.capability_id)?;
        if self.timeout_ms == 0 {
            return Err(ExecutionError::InvalidRequest(
                "timeout_ms must be greater than zero".into(),
            ));
        }
        if self.input_json.len() > MAX_EXECUTION_INPUT_JSON_BYTES {
            return Err(ExecutionError::InvalidRequest(
                "input_json exceeds the bounded execution payload size".into(),
            ));
        }
        serde_json::from_str::<serde_json::Value>(&self.input_json)
            .map_err(|_| ExecutionError::InvalidRequest("input_json must be valid JSON".into()))?;
        Ok(())
    }
}

fn validate_execution_identity(label: &str, value: &str) -> Result<(), ExecutionError> {
    if value.trim().is_empty()
        || value.len() > MAX_EXECUTION_ID_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(ExecutionError::InvalidRequest(format!(
            "{label} identity is invalid"
        )));
    }
    Ok(())
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


#[cfg(test)]
mod tests {
    use super::*;

    fn request(input_json: String) -> ExecutionRequest {
        ExecutionRequest {
            operation_id: "operation-1".into(),
            capability_id: "android.local.test".into(),
            input_json,
            timeout_ms: 1_000,
        }
    }

    #[test]
    fn execution_request_rejects_unbounded_or_invalid_wire_inputs() {
        let mut invalid = request("{".into());
        assert!(matches!(
            invalid.validate(),
            Err(ExecutionError::InvalidRequest(message))
                if message.contains("valid JSON")
        ));

        invalid.input_json = format!("{{\"value\":\"{}\"}}", "x".repeat(MAX_EXECUTION_INPUT_JSON_BYTES));
        assert!(matches!(
            invalid.validate(),
            Err(ExecutionError::InvalidRequest(message))
                if message.contains("bounded execution payload")
        ));

        invalid = request("{}".into());
        invalid.operation_id = "bad\noperation".into();
        assert!(invalid.validate().is_err());
        invalid = request("{}".into());
        invalid.capability_id = "x".repeat(MAX_EXECUTION_ID_BYTES + 1);
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn execution_request_accepts_bounded_valid_json() {
        assert!(request(r#"{"path":"notes.txt"}"#.into()).validate().is_ok());
    }
}
