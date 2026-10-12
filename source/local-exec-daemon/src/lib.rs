//! Android local Runner contract implementation.
//!
//! This layer advertises and validates capabilities. It intentionally does not expose an
//! unrestricted shell; concrete Android actions must be individually registered.

pub mod invariant_violation_log;
#[allow(special_module_name)]
pub mod main;
pub mod production_executor;

use std::collections::{BTreeMap, BTreeSet};

use fabushi_android_shared::{
    ExecutionCapability, ExecutionError, ExecutionRequest, ExecutionResult, ExecutionTarget,
};

pub trait LocalCapabilityHandler: Send {
    fn execute(&mut self, request: &ExecutionRequest) -> Result<ExecutionResult, ExecutionError>;
    fn cancel(&mut self, operation_id: &str) -> Result<(), ExecutionError>;
}

pub struct AndroidLocalRunner {
    capabilities: BTreeMap<String, (ExecutionCapability, Box<dyn LocalCapabilityHandler>)>,
    active: BTreeSet<String>,
}

impl AndroidLocalRunner {
    pub fn new() -> Self {
        Self { capabilities: BTreeMap::new(), active: BTreeSet::new() }
    }

    pub fn register(&mut self, capability: ExecutionCapability, handler: Box<dyn LocalCapabilityHandler>) -> Result<(), ExecutionError> {
        if capability.target != ExecutionTarget::AndroidLocal {
            return Err(ExecutionError::InvalidRequest("local runner accepts only AndroidLocal capabilities".into()));
        }
        if capability.id.trim().is_empty() {
            return Err(ExecutionError::InvalidRequest("capability id must not be empty".into()));
        }
        if self.capabilities.contains_key(&capability.id) {
            return Err(ExecutionError::InvalidRequest(format!(
                "duplicate local capability registration: {}",
                capability.id
            )));
        }
        self.capabilities.insert(capability.id.clone(), (capability, handler));
        Ok(())
    }

    pub fn execute(&mut self, request: ExecutionRequest) -> Result<ExecutionResult, ExecutionError> {
        request.validate()?;
        if !self.active.insert(request.operation_id.clone()) {
            return Err(ExecutionError::InvalidRequest("operation id is already active".into()));
        }
        let result = match self.capabilities.get_mut(&request.capability_id) {
            Some((_, handler)) => handler.execute(&request),
            None => Err(ExecutionError::CapabilityUnavailable(request.capability_id.clone())),
        };
        self.active.remove(&request.operation_id);
        result
    }

    pub fn advertised_capabilities(&self) -> Vec<ExecutionCapability> {
        self.capabilities.values().map(|(capability, _)| capability.clone()).collect()
    }
}

impl Default for AndroidLocalRunner { fn default() -> Self { Self::new() } }


#[cfg(test)]
mod tests {
    use super::*;

    struct EchoHandler(&'static str);

    impl LocalCapabilityHandler for EchoHandler {
        fn execute(&mut self, request: &ExecutionRequest) -> Result<ExecutionResult, ExecutionError> {
            Ok(ExecutionResult {
                operation_id: request.operation_id.clone(),
                output_json: format!(r#"{{"owner":"{}"}}"#, self.0),
            })
        }

        fn cancel(&mut self, _operation_id: &str) -> Result<(), ExecutionError> {
            Ok(())
        }
    }

    fn capability(id: &str) -> ExecutionCapability {
        ExecutionCapability {
            id: id.into(),
            target: ExecutionTarget::AndroidLocal,
            cancellable: false,
            supports_streaming: false,
        }
    }

    #[test]
    fn duplicate_capability_registration_fails_closed_without_replacing_owner() {
        let mut runner = AndroidLocalRunner::new();
        runner
            .register(capability("android.echo"), Box::new(EchoHandler("first")))
            .unwrap();

        assert!(matches!(
            runner.register(capability("android.echo"), Box::new(EchoHandler("replacement"))),
            Err(ExecutionError::InvalidRequest(message))
                if message.contains("duplicate local capability registration")
        ));

        let result = runner
            .execute(ExecutionRequest {
                operation_id: "op-1".into(),
                capability_id: "android.echo".into(),
                input_json: "{}".into(),
                timeout_ms: 1_000,
            })
            .unwrap();
        assert_eq!(result.output_json, r#"{"owner":"first"}"#);
        assert_eq!(runner.advertised_capabilities().len(), 1);
    }

    #[test]
    fn non_android_local_capability_registration_fails_closed() {
        let mut runner = AndroidLocalRunner::new();
        let mut wrong = capability("remote.echo");
        wrong.target = ExecutionTarget::RemoteBox;
        assert!(runner
            .register(wrong, Box::new(EchoHandler("remote")))
            .is_err());
        assert!(runner.advertised_capabilities().is_empty());
    }
}
