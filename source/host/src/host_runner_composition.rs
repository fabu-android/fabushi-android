use fabushi_android_shared::{ExecutionError, ExecutionRequest, ExecutionResult, ExecutionTarget};

pub const DEFAULT_SAND_MODEL: &str = "default";
pub const SAND_SUMMARIZATION_MAX_PROMPT_CHARS: usize = 32_000;

pub trait HostRunnerSession {
    fn execute(&mut self, request: &ExecutionRequest) -> Result<ExecutionResult, ExecutionError>;
    fn cancel(&mut self, operation_id: &str) -> Result<(), ExecutionError>;
}

/// Host-owned execution router. The caller must supply the capability-broker-selected target;
/// the router never upgrades a local request into a remote request on its own.
pub struct HostRunnerComposition<L: HostRunnerSession, R: HostRunnerSession> {
    local: L,
    remote: R,
}

impl<L: HostRunnerSession, R: HostRunnerSession> HostRunnerComposition<L, R> {
    pub fn new(local: L, remote: R) -> Self {
        Self { local, remote }
    }

    pub fn run(
        &mut self,
        target: ExecutionTarget,
        request: &ExecutionRequest,
    ) -> Result<ExecutionResult, ExecutionError> {
        request.validate()?;
        match target {
            ExecutionTarget::AndroidLocal => self.local.execute(request),
            ExecutionTarget::RemoteBox => self.remote.execute(request),
        }
    }

    pub fn cancel(
        &mut self,
        target: ExecutionTarget,
        operation_id: &str,
    ) -> Result<(), ExecutionError> {
        if operation_id.trim().is_empty() {
            return Err(ExecutionError::InvalidRequest(
                "operation_id must not be empty".into(),
            ));
        }
        match target {
            ExecutionTarget::AndroidLocal => self.local.cancel(operation_id),
            ExecutionTarget::RemoteBox => self.remote.cancel(operation_id),
        }
    }

    pub fn into_parts(self) -> (L, R) {
        (self.local, self.remote)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingRunner {
        executes: Vec<String>,
        cancels: Vec<String>,
    }

    impl HostRunnerSession for RecordingRunner {
        fn execute(&mut self, request: &ExecutionRequest) -> Result<ExecutionResult, ExecutionError> {
            self.executes.push(request.operation_id.clone());
            Ok(ExecutionResult {
                operation_id: request.operation_id.clone(),
                output_json: "{}".into(),
            })
        }

        fn cancel(&mut self, operation_id: &str) -> Result<(), ExecutionError> {
            self.cancels.push(operation_id.into());
            Ok(())
        }
    }

    fn request(operation_id: &str) -> ExecutionRequest {
        ExecutionRequest {
            operation_id: operation_id.into(),
            capability_id: "computer.use".into(),
            input_json: "{}".into(),
            timeout_ms: 30_000,
        }
    }

    #[test]
    fn remote_box_is_routed_only_to_remote_runner() {
        let mut composition = HostRunnerComposition::new(
            RecordingRunner::default(),
            RecordingRunner::default(),
        );
        composition
            .run(ExecutionTarget::RemoteBox, &request("remote-op"))
            .unwrap();
        let (local, remote) = composition.into_parts();
        assert!(local.executes.is_empty());
        assert_eq!(remote.executes, vec!["remote-op"]);
    }

    #[test]
    fn local_target_never_falls_through_to_remote_runner() {
        let mut composition = HostRunnerComposition::new(
            RecordingRunner::default(),
            RecordingRunner::default(),
        );
        composition
            .run(ExecutionTarget::AndroidLocal, &request("local-op"))
            .unwrap();
        let (local, remote) = composition.into_parts();
        assert_eq!(local.executes, vec!["local-op"]);
        assert!(remote.executes.is_empty());
    }

    #[test]
    fn cancellation_is_routed_to_the_original_execution_target() {
        let mut composition = HostRunnerComposition::new(
            RecordingRunner::default(),
            RecordingRunner::default(),
        );
        composition
            .cancel(ExecutionTarget::RemoteBox, "remote-op")
            .unwrap();
        let (local, remote) = composition.into_parts();
        assert!(local.cancels.is_empty());
        assert_eq!(remote.cancels, vec!["remote-op"]);
    }
}
