use crate::remote_execution::{
    RemoteExecutionJournal, RemoteExecutionRecord, RemoteExecutionState,
};
use fabushi_android_box_exec_daemon::{
    AuthenticatedRemoteHttpTransport, RemoteCancelOutcome, RemoteDispatchOutcome,
    RemoteExecutionContext, RemoteExecutionTransport, RemoteReconcileOutcome, RemoteRunner,
    RemoteTransportPolicy,
};
use fabushi_android_shared::{ExecutionError, ExecutionRequest, ExecutionResult, ExecutionTarget};
use std::path::Path;

pub const DEFAULT_SAND_MODEL: &str = "default";
pub const SAND_SUMMARIZATION_MAX_PROMPT_CHARS: usize = 32_000;

pub trait HostRunnerSession {
    fn execute(&mut self, request: &ExecutionRequest) -> Result<ExecutionResult, ExecutionError>;
    fn cancel(&mut self, operation_id: &str) -> Result<(), ExecutionError>;
}

/// Legacy target router retained for local-only call sites. It never upgrades a local
/// request into a remote request. Shipping RemoteBox execution uses
/// AuthorizedHostRunnerComposition below because remote dispatch requires an account,
/// request, operation and one-time grant identity.
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

pub struct AuthenticatedRemoteHostRunner<T: RemoteExecutionTransport> {
    runner: RemoteRunner<T>,
    journal: RemoteExecutionJournal,
}

impl AuthenticatedRemoteHostRunner<AuthenticatedRemoteHttpTransport> {
    pub fn production(
        endpoint: &str,
        journal_path: impl AsRef<Path>,
        policy: RemoteTransportPolicy,
        now_ms: u64,
    ) -> Result<Self, ExecutionError> {
        let transport = AuthenticatedRemoteHttpTransport::new(endpoint, policy)?;
        let journal = RemoteExecutionJournal::open(journal_path.as_ref(), now_ms)
            .map_err(ExecutionError::Transport)?;
        Ok(Self::new(transport, journal))
    }
}

impl<T: RemoteExecutionTransport> AuthenticatedRemoteHostRunner<T> {
    pub fn new(transport: T, journal: RemoteExecutionJournal) -> Self {
        Self {
            runner: RemoteRunner::new(transport),
            journal,
        }
    }

    pub fn execute_authorized(
        &mut self,
        context: &RemoteExecutionContext,
        request: &ExecutionRequest,
        now_ms: u64,
    ) -> Result<ExecutionResult, ExecutionError> {
        request.validate()?;
        context.validate()?;
        if context.operation_id != request.operation_id {
            return Err(ExecutionError::InvalidRequest(
                "remote authorization operation does not match request".into(),
            ));
        }

        if let Some(existing) = self.journal.record(&request.operation_id) {
            if existing.request_id != context.request_id
                || existing.capability_id != request.capability_id
                || existing.account_fence != context.account_fence
                || existing.account_epoch != context.account_epoch
                || existing.permission_grant_id != context.permission_grant_id
                || existing.device_id != context.device_id
            {
                return Err(ExecutionError::InvalidRequest(
                    "remote operation identity conflicts with durable journal".into(),
                ));
            }
            return match existing.state {
                RemoteExecutionState::Completed => Ok(ExecutionResult {
                    operation_id: existing.operation_id.clone(),
                    output_json: existing.terminal_output_json.clone().unwrap_or_default(),
                }),
                RemoteExecutionState::OutcomeUnknown
                | RemoteExecutionState::Sent
                | RemoteExecutionState::Acked => Err(ExecutionError::Transport(
                    "remote_outcome_unknown_reconcile_required".into(),
                )),
                RemoteExecutionState::Rejected => Err(ExecutionError::Transport(
                    "remote_execution_rejected".into(),
                )),
                RemoteExecutionState::Cancelled => Err(ExecutionError::Transport(
                    "remote_execution_cancelled".into(),
                )),
                RemoteExecutionState::Pending => Err(ExecutionError::Transport(
                    "remote_pending_dispatch_requires_original_owner".into(),
                )),
            };
        }

        self.journal
            .begin(RemoteExecutionRecord {
                operation_id: request.operation_id.clone(),
                request_id: context.request_id.clone(),
                capability_id: request.capability_id.clone(),
                account_fence: context.account_fence.clone(),
                account_epoch: context.account_epoch,
                permission_grant_id: context.permission_grant_id.clone(),
                device_id: context.device_id.clone(),
                state: RemoteExecutionState::Pending,
                created_at_ms: now_ms,
                updated_at_ms: now_ms,
                remote_ack_id: None,
                terminal_output_json: None,
            })
            .map_err(ExecutionError::Transport)?;
        self.journal
            .mark_sent(
                &request.operation_id,
                &context.account_fence,
                context.account_epoch,
                &context.permission_grant_id,
                now_ms,
            )
            .map_err(ExecutionError::Transport)?;

        match self.runner.execute(context, request.clone())? {
            RemoteDispatchOutcome::Completed { ack_id, result } => {
                self.journal
                    .acknowledge(
                        &request.operation_id,
                        &context.request_id,
                        &context.account_fence,
                        context.account_epoch,
                        &ack_id,
                        now_ms,
                    )
                    .map_err(ExecutionError::Transport)?;
                self.journal
                    .complete(
                        &request.operation_id,
                        &context.request_id,
                        &context.account_fence,
                        context.account_epoch,
                        result.output_json.clone(),
                        now_ms,
                    )
                    .map_err(ExecutionError::Transport)?;
                Ok(result)
            }
            RemoteDispatchOutcome::Rejected { ack_id, reason } => {
                self.journal
                    .reject_after_dispatch(
                        &request.operation_id,
                        &context.request_id,
                        &context.account_fence,
                        context.account_epoch,
                        ack_id.as_deref(),
                        now_ms,
                    )
                    .map_err(ExecutionError::Transport)?;
                Err(ExecutionError::Transport(format!(
                    "remote_execution_rejected:{reason}"
                )))
            }
            RemoteDispatchOutcome::OutcomeUnknown { reason, .. } => {
                self.journal
                    .timeout(&request.operation_id, now_ms)
                    .map_err(ExecutionError::Transport)?;
                Err(ExecutionError::Transport(format!(
                    "remote_outcome_unknown:{reason}"
                )))
            }
        }
    }

    pub fn reconcile_authorized(
        &mut self,
        context: &RemoteExecutionContext,
        now_ms: u64,
    ) -> Result<ExecutionResult, ExecutionError> {
        let record = self
            .journal
            .record(&context.operation_id)
            .cloned()
            .ok_or_else(|| ExecutionError::InvalidRequest("remote operation is unknown".into()))?;
        if record.request_id != context.request_id
            || record.account_fence != context.account_fence
            || record.account_epoch != context.account_epoch
            || record.permission_grant_id != context.permission_grant_id
        {
            return Err(ExecutionError::InvalidRequest(
                "remote reconciliation identity conflicts with durable journal".into(),
            ));
        }
        if record.state != RemoteExecutionState::OutcomeUnknown {
            return Err(ExecutionError::InvalidRequest(
                "remote reconciliation requires outcome-unknown state".into(),
            ));
        }
        match self.runner.reconcile(
            context,
            &context.operation_id,
            &context.request_id,
        )? {
            RemoteReconcileOutcome::Completed { result, .. } => {
                self.journal
                    .reconcile(
                        &context.operation_id,
                        &context.account_fence,
                        context.account_epoch,
                        result.output_json.clone(),
                        now_ms,
                    )
                    .map_err(ExecutionError::Transport)?;
                Ok(result)
            }
            RemoteReconcileOutcome::Rejected { ack_id, reason } => {
                self.journal
                    .reconcile_rejected(
                        &context.operation_id,
                        &context.account_fence,
                        context.account_epoch,
                        ack_id.as_deref(),
                        now_ms,
                    )
                    .map_err(ExecutionError::Transport)?;
                Err(ExecutionError::Transport(format!(
                    "remote_execution_rejected:{reason}"
                )))
            }
            RemoteReconcileOutcome::Pending { .. } => Err(ExecutionError::Transport(
                "remote_outcome_unknown_reconcile_pending".into(),
            )),
        }
    }

    pub fn cancel_authorized(
        &mut self,
        context: &RemoteExecutionContext,
        now_ms: u64,
    ) -> Result<(), ExecutionError> {
        let state = self
            .journal
            .record(&context.operation_id)
            .map(|record| record.state.clone())
            .ok_or_else(|| ExecutionError::InvalidRequest("remote operation is unknown".into()))?;
        if state == RemoteExecutionState::Pending {
            return self
                .journal
                .cancel_before_dispatch(&context.operation_id, now_ms)
                .map_err(ExecutionError::Transport);
        }
        match self.runner.cancel(
            context,
            &context.operation_id,
            &context.request_id,
        )? {
            RemoteCancelOutcome::Confirmed { ack_id } => self
                .journal
                .confirm_cancel_after_dispatch(
                    &context.operation_id,
                    &context.account_fence,
                    context.account_epoch,
                    ack_id.as_deref(),
                    now_ms,
                )
                .map_err(ExecutionError::Transport),
            RemoteCancelOutcome::OutcomeUnknown { .. } => self
                .journal
                .cancel_after_dispatch(&context.operation_id, now_ms)
                .map_err(ExecutionError::Transport),
        }
    }

    pub fn journal(&self) -> &RemoteExecutionJournal {
        &self.journal
    }
}

/// Shipping execution composition. RemoteBox is impossible without a concrete
/// authenticated transport plus durable RemoteExecutionJournal. AndroidLocal has
/// no implicit fallback or upgrade to the remote runner.
pub struct AuthorizedHostRunnerComposition<L: HostRunnerSession, T: RemoteExecutionTransport> {
    local: L,
    remote: AuthenticatedRemoteHostRunner<T>,
}

impl<L: HostRunnerSession>
    AuthorizedHostRunnerComposition<L, AuthenticatedRemoteHttpTransport>
{
    pub fn production(
        local: L,
        remote_endpoint: &str,
        remote_journal_path: impl AsRef<Path>,
        policy: RemoteTransportPolicy,
        now_ms: u64,
    ) -> Result<Self, ExecutionError> {
        Ok(Self {
            local,
            remote: AuthenticatedRemoteHostRunner::production(
                remote_endpoint,
                remote_journal_path,
                policy,
                now_ms,
            )?,
        })
    }
}

impl<L: HostRunnerSession, T: RemoteExecutionTransport> AuthorizedHostRunnerComposition<L, T> {
    pub fn new(local: L, remote: AuthenticatedRemoteHostRunner<T>) -> Self {
        Self { local, remote }
    }

    pub fn run(
        &mut self,
        target: ExecutionTarget,
        request: &ExecutionRequest,
        remote_context: Option<&RemoteExecutionContext>,
        now_ms: u64,
    ) -> Result<ExecutionResult, ExecutionError> {
        request.validate()?;
        match target {
            ExecutionTarget::AndroidLocal => self.local.execute(request),
            ExecutionTarget::RemoteBox => {
                let context = remote_context.ok_or_else(|| {
                    ExecutionError::InvalidRequest(
                        "RemoteBox requires authenticated remote execution context".into(),
                    )
                })?;
                self.remote.execute_authorized(context, request, now_ms)
            }
        }
    }

    pub fn remote(&self) -> &AuthenticatedRemoteHostRunner<T> {
        &self.remote
    }

    pub fn into_parts(self) -> (L, AuthenticatedRemoteHostRunner<T>) {
        (self.local, self.remote)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fabushi_android_box_exec_daemon::{
        RemoteBearerCredential, RemoteCancelOutcome, RemoteDispatchOutcome,
        RemoteReconcileOutcome,
    };

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

    #[derive(Default)]
    struct RecordingRemoteTransport {
        executes: usize,
        result: Option<RemoteDispatchOutcome>,
    }

    impl RemoteExecutionTransport for RecordingRemoteTransport {
        fn execute(
            &mut self,
            _context: &RemoteExecutionContext,
            request: &ExecutionRequest,
        ) -> Result<RemoteDispatchOutcome, ExecutionError> {
            self.executes += 1;
            Ok(self.result.take().unwrap_or_else(|| RemoteDispatchOutcome::Completed {
                ack_id: "ack-1".into(),
                result: ExecutionResult {
                    operation_id: request.operation_id.clone(),
                    output_json: r#"{"ok":true}"#.into(),
                },
            }))
        }

        fn reconcile(
            &mut self,
            _context: &RemoteExecutionContext,
            operation_id: &str,
            _request_id: &str,
        ) -> Result<RemoteReconcileOutcome, ExecutionError> {
            Ok(RemoteReconcileOutcome::Completed {
                ack_id: "ack-reconcile".into(),
                result: ExecutionResult {
                    operation_id: operation_id.into(),
                    output_json: r#"{"reconciled":true}"#.into(),
                },
            })
        }

        fn cancel(
            &mut self,
            _context: &RemoteExecutionContext,
            _operation_id: &str,
            _request_id: &str,
        ) -> Result<RemoteCancelOutcome, ExecutionError> {
            Ok(RemoteCancelOutcome::Confirmed {
                ack_id: Some("ack-cancel".into()),
            })
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

    fn context(operation_id: &str) -> RemoteExecutionContext {
        RemoteExecutionContext {
            bearer: RemoteBearerCredential::new("remote-test-token-long-enough").unwrap(),
            account_fence: "session:account-a".into(),
            account_epoch: 7,
            operation_id: operation_id.into(),
            request_id: format!("request-{operation_id}"),
            permission_grant_id: "grant-1".into(),
            device_id: "device-1".into(),
        }
    }

    #[test]
    fn remote_operation_identity_is_fenced_by_device() {
        let root = tempfile::tempdir().unwrap();
        let journal =
            RemoteExecutionJournal::open(root.path().join("remote.json"), 1).unwrap();
        let remote = AuthenticatedRemoteHostRunner::new(
            RecordingRemoteTransport::default(),
            journal,
        );
        let mut composition =
            AuthorizedHostRunnerComposition::new(RecordingRunner::default(), remote);
        composition
            .run(
                ExecutionTarget::RemoteBox,
                &request("remote-device-op"),
                Some(&context("remote-device-op")),
                2,
            )
            .unwrap();

        let mut wrong_device = context("remote-device-op");
        wrong_device.device_id = "device-2".into();
        let error = composition
            .run(
                ExecutionTarget::RemoteBox,
                &request("remote-device-op"),
                Some(&wrong_device),
                3,
            )
            .unwrap_err();
        assert!(matches!(error, ExecutionError::InvalidRequest(_)));
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
        let root = tempfile::tempdir().unwrap();
        let journal =
            RemoteExecutionJournal::open(root.path().join("remote.json"), 1).unwrap();
        let remote = AuthenticatedRemoteHostRunner::new(
            RecordingRemoteTransport::default(),
            journal,
        );
        let mut composition =
            AuthorizedHostRunnerComposition::new(RecordingRunner::default(), remote);
        composition
            .run(ExecutionTarget::AndroidLocal, &request("local-op"), None, 2)
            .unwrap();
        let (local, remote) = composition.into_parts();
        assert_eq!(local.executes, vec!["local-op"]);
        assert!(remote.journal().record("local-op").is_none());
    }

    #[test]
    fn remote_box_requires_auth_context_and_persists_terminal_result() {
        let root = tempfile::tempdir().unwrap();
        let journal =
            RemoteExecutionJournal::open(root.path().join("remote.json"), 1).unwrap();
        let remote = AuthenticatedRemoteHostRunner::new(
            RecordingRemoteTransport::default(),
            journal,
        );
        let mut composition =
            AuthorizedHostRunnerComposition::new(RecordingRunner::default(), remote);
        assert!(composition
            .run(ExecutionTarget::RemoteBox, &request("remote-op"), None, 2)
            .is_err());
        let result = composition
            .run(
                ExecutionTarget::RemoteBox,
                &request("remote-op"),
                Some(&context("remote-op")),
                3,
            )
            .unwrap();
        assert!(result.output_json.contains("ok"));
        assert_eq!(
            composition.remote().journal().record("remote-op").unwrap().state,
            RemoteExecutionState::Completed
        );
    }

    #[test]
    fn outcome_unknown_never_replays_and_reconciliation_settles_original_operation() {
        let root = tempfile::tempdir().unwrap();
        let journal =
            RemoteExecutionJournal::open(root.path().join("remote.json"), 1).unwrap();
        let remote = AuthenticatedRemoteHostRunner::new(
            RecordingRemoteTransport {
                executes: 0,
                result: Some(RemoteDispatchOutcome::OutcomeUnknown {
                    ack_id: Some("ack-unknown".into()),
                    reason: "connection-lost".into(),
                }),
            },
            journal,
        );
        let mut composition =
            AuthorizedHostRunnerComposition::new(RecordingRunner::default(), remote);
        let ctx = context("remote-unknown");
        assert!(composition
            .run(
                ExecutionTarget::RemoteBox,
                &request("remote-unknown"),
                Some(&ctx),
                2,
            )
            .is_err());
        assert_eq!(
            composition.remote().journal().record("remote-unknown").unwrap().state,
            RemoteExecutionState::OutcomeUnknown
        );
        assert!(composition
            .run(
                ExecutionTarget::RemoteBox,
                &request("remote-unknown"),
                Some(&ctx),
                3,
            )
            .is_err());
        let reconciled = composition
            .remote
            .reconcile_authorized(&ctx, 4)
            .unwrap();
        assert!(reconciled.output_json.contains("reconciled"));
        assert_eq!(
            composition.remote().journal().record("remote-unknown").unwrap().state,
            RemoteExecutionState::Completed
        );
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
