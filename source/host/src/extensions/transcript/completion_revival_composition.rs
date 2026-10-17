use std::sync::Arc;

use crate::runner::background_work::{
    BackgroundShellBackgroundCompletion, BackgroundShellSettledCallback,
    CloudAgentBackgroundCompletion, CloudAgentSettledCallback,
};

use super::{
    completion_revivals::{CompletionRevivals, ShellCompletion, SubagentCompletion},
    sand_pending_wake_store::coerce_quiet_origin,
};

pub fn cloud_agent_settled_callback(
    revivals: Arc<CompletionRevivals>,
) -> CloudAgentSettledCallback {
    Arc::new(move |completion: CloudAgentBackgroundCompletion| {
        revivals.handle_background_subagent_completion(SubagentCompletion {
            parent_agent_id: completion.parent_agent_id,
            subagent_agent_id: completion.work_id,
            title: completion.title,
            subagent_type: "cursor-agent".into(),
            status: completion.status,
            result: completion.result,
            quiet_origin: completion.quiet_origin.as_ref().and_then(coerce_quiet_origin),
        });
    })
}

pub fn background_shell_settled_callback(
    revivals: Arc<CompletionRevivals>,
) -> BackgroundShellSettledCallback {
    Arc::new(move |completion: BackgroundShellBackgroundCompletion| {
        revivals.handle_background_shell_completion(ShellCompletion {
            agent_id: completion.parent_agent_id,
            shell_id: completion.work_id,
            title: completion.title,
            status: completion.status,
            detail: completion.detail,
            output_path: completion.output_path,
            quiet_origin: completion.quiet_origin.as_ref().and_then(coerce_quiet_origin),
        });
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use serde_json::json;
    use super::*;
    use crate::extensions::transcript::{
        completion_revivals::{
            CompletionRevivalRuntimePort, RevivalExecution, RevivalReport,
        },
        sand_pending_wake_store::PendingWakeKind,
    };

    #[derive(Default)]
    struct Runtime {
        prompts: Mutex<Vec<(String, String)>>,
        cleared: Mutex<Vec<(String, PendingWakeKind, String)>>,
        reports: Mutex<Vec<RevivalReport>>,
    }

    impl CompletionRevivalRuntimePort for Runtime {
        fn can_execute(&self) -> bool { true }
        fn is_agent_deleted(&self, _agent_id: &str) -> bool { false }
        fn is_agent_gone(&self, _agent_id: &str) -> bool { false }
        fn clear_pending_wake(&self, agent_id: &str, kind: PendingWakeKind, work_id: &str) {
            self.cleared.lock().unwrap().push((
                agent_id.to_string(), kind, work_id.to_string(),
            ));
        }
        fn run_background_revival(
            &self,
            agent_id: &str,
            _source: &str,
            prompt: &str,
            _is_silence_allowed: bool,
            _auto_review_epoch: &str,
        ) -> Result<RevivalExecution, String> {
            self.prompts.lock().unwrap().push((agent_id.to_string(), prompt.to_string()));
            Ok(RevivalExecution {
                aborted: false,
                quiesced_for_upgrade: false,
                sent_message_count: 0,
            })
        }
        fn mark_resume_pending_for_quiesced_revival(&self, _agent_id: &str) {}
        fn report_revival(&self, report: RevivalReport) {
            self.reports.lock().unwrap().push(report);
        }
        fn report_revival_error(&self, _agent_id: &str, _title: &str, _error: &str) {}
    }

    #[test]
    fn cloud_settlement_reuses_single_completion_revival_owner() {
        let runtime = Arc::new(Runtime::default());
        let callback = cloud_agent_settled_callback(Arc::new(CompletionRevivals::new(runtime.clone())));
        callback(CloudAgentBackgroundCompletion {
            parent_agent_id: "parent".into(),
            work_id: "cloud-1".into(),
            title: "Research".into(),
            status: "completed".into(),
            result: "done".into(),
            quiet_origin: Some(json!({"automation":{"id":"routine-1","name":"Watch build"}})),
        });
        let prompts = runtime.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].1.contains("Research"));
        assert!(prompts[0].1.contains("nobody is waiting on it"));
        let cleared = runtime.cleared.lock().unwrap();
        assert_eq!(cleared.len(), 1);
        assert_eq!(cleared[0].1, PendingWakeKind::CloudAgent);
        assert_eq!(cleared[0].2, "cloud-1");
    }

    #[test]
    fn shell_settlement_preserves_detail_output_and_quiet_origin() {
        let runtime = Arc::new(Runtime::default());
        let callback = background_shell_settled_callback(Arc::new(CompletionRevivals::new(runtime.clone())));
        callback(BackgroundShellBackgroundCompletion {
            parent_agent_id: "parent".into(),
            work_id: "shell-1".into(),
            title: "Build".into(),
            status: "success".into(),
            detail: Some("all good".into()),
            output_path: Some("/private/output.log".into()),
            quiet_origin: Some(json!({"automation":{"id":"routine-2","name":"Nightly build"}})),
        });
        let prompts = runtime.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].1.contains("all good"));
        assert!(prompts[0].1.contains("/private/output.log"));
        assert!(prompts[0].1.contains("nobody is waiting on it"));
        let cleared = runtime.cleared.lock().unwrap();
        assert_eq!(cleared.len(), 1);
        assert_eq!(cleared[0].1, PendingWakeKind::Shell);
        assert_eq!(cleared[0].2, "shell-1");
    }
}
