use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

pub type SharedTurnRunShell = Arc<Mutex<TurnRunShell>>;

pub fn shared_turn_run_shell() -> SharedTurnRunShell {
    Arc::new(Mutex::new(TurnRunShell::default()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnCancellation {
    pub intentional: bool,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveRun {
    pub operation_id: String,
    pub request_id: String,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnRunShellError {
    EmptyPrompt,
    InterruptedBeforeDispatch,
    AlreadyActive,
    QuiescingForUpgrade,
    UnknownRun,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnRunLease {
    pub operation_id: String,
    pub request_id: String,
    pub generation: u64,
}

#[derive(Default)]
pub struct TurnRunShell {
    generation: u64,
    active: BTreeMap<String, ActiveRun>,
    awaiting_user: BTreeSet<String>,
    cancellations: BTreeMap<String, TurnCancellation>,
    quiescing_for_upgrade: bool,
}

impl TurnRunShell {
    pub fn begin(
        &mut self,
        operation_id: &str,
        request_id: &str,
        prompt: &str,
    ) -> Result<TurnRunLease, TurnRunShellError> {
        if prompt.trim().is_empty() {
            return Err(TurnRunShellError::EmptyPrompt);
        }
        if self.quiescing_for_upgrade {
            return Err(TurnRunShellError::QuiescingForUpgrade);
        }
        if operation_id.trim().is_empty() || request_id.trim().is_empty() {
            return Err(TurnRunShellError::InterruptedBeforeDispatch);
        }
        if self.active.contains_key(operation_id) {
            return Err(TurnRunShellError::AlreadyActive);
        }

        self.generation = self.generation.saturating_add(1).max(1);
        let active = ActiveRun {
            operation_id: operation_id.to_string(),
            request_id: request_id.to_string(),
            generation: self.generation,
        };
        self.active.insert(operation_id.to_string(), active.clone());
        self.cancellations.remove(operation_id);
        self.awaiting_user.remove(operation_id);
        Ok(TurnRunLease {
            operation_id: active.operation_id,
            request_id: active.request_id,
            generation: active.generation,
        })
    }

    pub fn cancel(
        &mut self,
        operation_id: &str,
        cancellation: TurnCancellation,
    ) -> Result<ActiveRun, TurnRunShellError> {
        let run = self
            .active
            .remove(operation_id)
            .ok_or(TurnRunShellError::UnknownRun)?;
        self.awaiting_user.remove(operation_id);
        self.cancellations
            .insert(operation_id.to_string(), cancellation);
        Ok(run)
    }

    pub fn finish(&mut self, operation_id: &str) -> Result<ActiveRun, TurnRunShellError> {
        let run = self
            .active
            .remove(operation_id)
            .ok_or(TurnRunShellError::UnknownRun)?;
        self.awaiting_user.remove(operation_id);
        Ok(run)
    }

    pub fn mark_awaiting_user(
        &mut self,
        operation_id: &str,
    ) -> Result<(), TurnRunShellError> {
        if !self.active.contains_key(operation_id) {
            return Err(TurnRunShellError::UnknownRun);
        }
        self.awaiting_user.insert(operation_id.to_string());
        Ok(())
    }

    pub fn resume_awaiting_user(
        &mut self,
        operation_id: &str,
    ) -> Result<(), TurnRunShellError> {
        if !self.active.contains_key(operation_id) || !self.awaiting_user.remove(operation_id) {
            return Err(TurnRunShellError::UnknownRun);
        }
        Ok(())
    }

    pub fn end_awaiting_user(
        &mut self,
        operation_id: &str,
        reason: impl Into<String>,
    ) -> Result<ActiveRun, TurnRunShellError> {
        if !self.awaiting_user.contains(operation_id) {
            return Err(TurnRunShellError::UnknownRun);
        }
        self.cancel(
            operation_id,
            TurnCancellation {
                intentional: true,
                reason: reason.into(),
            },
        )
    }

    pub fn interrupt_all(&mut self, reason: impl Into<String>) -> Vec<ActiveRun> {
        let reason = reason.into();
        let operation_ids = self.active.keys().cloned().collect::<Vec<_>>();
        operation_ids
            .into_iter()
            .filter_map(|operation_id| {
                self.cancel(
                    &operation_id,
                    TurnCancellation {
                        intentional: true,
                        reason: reason.clone(),
                    },
                )
                .ok()
            })
            .collect()
    }

    pub fn request_quiesce_for_upgrade(&mut self) {
        self.quiescing_for_upgrade = true;
    }

    pub fn cancel_quiesce_for_upgrade(&mut self) {
        self.quiescing_for_upgrade = false;
    }

    pub fn is_quiescing_for_upgrade(&self) -> bool {
        self.quiescing_for_upgrade
    }

    pub fn is_active(&self, operation_id: &str) -> bool {
        self.active.contains_key(operation_id)
    }

    pub fn is_awaiting_user(&self, operation_id: &str) -> bool {
        self.awaiting_user.contains(operation_id)
    }

    pub fn cancellation(&self, operation_id: &str) -> Option<&TurnCancellation> {
        self.cancellations.get(operation_id)
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_owns_generation_active_and_terminal_lifecycle() {
        let mut shell = TurnRunShell::default();
        let first = shell.begin("op-1", "req-1", "hello").unwrap();
        assert_eq!(first.generation, 1);
        assert!(shell.is_active("op-1"));
        assert_eq!(
            shell.begin("op-1", "req-2", "again"),
            Err(TurnRunShellError::AlreadyActive)
        );
        shell.finish("op-1").unwrap();
        assert!(!shell.is_active("op-1"));

        let second = shell.begin("op-2", "req-2", "hello").unwrap();
        assert_eq!(second.generation, 2);
    }

    #[test]
    fn quiesce_blocks_new_runs_without_abandoning_existing_run() {
        let mut shell = TurnRunShell::default();
        shell.begin("op-1", "req-1", "hello").unwrap();
        shell.request_quiesce_for_upgrade();
        assert!(shell.is_quiescing_for_upgrade());
        assert_eq!(
            shell.begin("op-2", "req-2", "hello"),
            Err(TurnRunShellError::QuiescingForUpgrade)
        );
        assert!(shell.is_active("op-1"));
        shell.cancel_quiesce_for_upgrade();
        assert!(shell.begin("op-2", "req-2", "hello").is_ok());
    }

    #[test]
    fn awaiting_user_and_interrupt_all_settle_owned_runs() {
        let mut shell = TurnRunShell::default();
        shell.begin("op-1", "req-1", "hello").unwrap();
        shell.begin("op-2", "req-2", "hello").unwrap();
        shell.mark_awaiting_user("op-1").unwrap();
        assert!(shell.is_awaiting_user("op-1"));
        shell.resume_awaiting_user("op-1").unwrap();
        assert!(shell.is_active("op-1"));
        assert!(!shell.is_awaiting_user("op-1"));

        shell.mark_awaiting_user("op-1").unwrap();
        shell.end_awaiting_user("op-1", "answer received").unwrap();
        assert!(!shell.is_active("op-1"));
        assert_eq!(
            shell.cancellation("op-1").map(|value| value.reason.as_str()),
            Some("answer received")
        );

        let interrupted = shell.interrupt_all("shutdown");
        assert_eq!(interrupted.len(), 1);
        assert_eq!(shell.active_count(), 0);
        assert_eq!(
            shell.cancellation("op-2").map(|value| value.reason.as_str()),
            Some("shutdown")
        );
    }

    #[test]
    fn empty_prompt_and_unknown_cancel_fail_closed() {
        let mut shell = TurnRunShell::default();
        assert_eq!(
            shell.begin("op", "req", "   "),
            Err(TurnRunShellError::EmptyPrompt)
        );
        assert_eq!(
            shell.cancel(
                "missing",
                TurnCancellation {
                    intentional: true,
                    reason: "user".into(),
                },
            ),
            Err(TurnRunShellError::UnknownRun)
        );
    }
}
