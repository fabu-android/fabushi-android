use std::collections::BTreeMap;
use fabushi_android_shared::{ExecutionError, ExecutionRequest, ExecutionResult};

pub trait ProductionAction: Send {
    fn execute(&mut self, input_json: &str) -> Result<String, ExecutionError>;
}

#[derive(Default)]
pub struct ProductionExecutor { actions: BTreeMap<String, Box<dyn ProductionAction>> }

impl ProductionExecutor {
    pub fn register(&mut self, capability_id: impl Into<String>, action: Box<dyn ProductionAction>) -> Result<(), ExecutionError> {
        let id=capability_id.into();
        if id.trim().is_empty() { return Err(ExecutionError::InvalidRequest("capability id is required".into())); }
        if self.actions.insert(id.clone(), action).is_some() {
            return Err(ExecutionError::InvalidRequest(format!("duplicate production capability: {id}")));
        }
        Ok(())
    }

    pub fn execute(&mut self, request: &ExecutionRequest) -> Result<ExecutionResult, ExecutionError> {
        request.validate()?;
        let action=self.actions.get_mut(&request.capability_id)
            .ok_or_else(|| ExecutionError::CapabilityUnavailable(request.capability_id.clone()))?;
        let output_json=action.execute(&request.input_json)?;
        Ok(ExecutionResult { operation_id: request.operation_id.clone(), output_json })
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use fabushi_android_shared::{ExecutionRequest, ExecutionTarget, ExecutionCapability};

    struct EchoAction;
    impl ProductionAction for EchoAction {
        fn execute(&mut self, input_json: &str) -> Result<String, ExecutionError> {
            Ok(format!(r#"{{"echo":{input_json}}}"#))
        }
    }

    #[test]
    fn production_executor_uses_canonical_input_and_output_fields() {
        let mut executor = ProductionExecutor::default();
        executor.register("android.echo", Box::new(EchoAction)).unwrap();
        let request = ExecutionRequest {
            operation_id: "op-1".into(),
            capability_id: "android.echo".into(),
            input_json: r#"{"value":1}"#.into(),
            timeout_ms: 1_000,
        };
        let result = executor.execute(&request).unwrap();
        assert_eq!(result.operation_id, "op-1");
        assert_eq!(result.output_json, r#"{"echo":{"value":1}}"#);
    }

    #[test]
    fn duplicate_and_unknown_capabilities_fail_closed() {
        let mut executor = ProductionExecutor::default();
        executor.register("android.echo", Box::new(EchoAction)).unwrap();
        assert!(executor.register("android.echo", Box::new(EchoAction)).is_err());
        let request = ExecutionRequest {
            operation_id: "op-2".into(),
            capability_id: "missing".into(),
            input_json: "{}".into(),
            timeout_ms: 1_000,
        };
        assert!(matches!(
            executor.execute(&request),
            Err(ExecutionError::CapabilityUnavailable(_))
        ));
        let _ = ExecutionCapability {
            id: "android.echo".into(),
            target: ExecutionTarget::AndroidLocal,
            cancellable: false,
            supports_streaming: false,
        };
    }
}
