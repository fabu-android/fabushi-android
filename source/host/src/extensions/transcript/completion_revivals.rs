use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use super::sand_pending_wake_store::{PendingWakeKind, QuietWakeOrigin};

pub const QUIET_REVIVAL_INSTRUCTION: &str = "Pick the work back up. Everything above came out of your own quiet standing order(s) — the user did not ask to hear about it, so the saved instruction's delivery rule governs. If the outcome is a genuine change, a new actionable result, or a real blocker the user must know about, tell them once with a single useful SendMessage. If it amounts to no change, nothing new, or still waiting, end the turn with no SendMessage at all — no \"still waiting\" or progress notes; if the standing order says to keep watching, just keep the watch going quietly. Keep your status current, and clear it once everything is done and you're idle.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentCompletion {
    pub parent_agent_id: String,
    pub subagent_agent_id: String,
    pub title: String,
    pub subagent_type: String,
    pub status: String,
    pub result: String,
    pub quiet_origin: Option<QuietWakeOrigin>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCompletion {
    pub agent_id: String,
    pub shell_id: String,
    pub title: String,
    pub status: String,
    pub detail: Option<String>,
    pub output_path: Option<String>,
    pub quiet_origin: Option<QuietWakeOrigin>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevivalExecution {
    pub aborted: bool,
    pub quiesced_for_upgrade: bool,
    pub sent_message_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevivalReport {
    pub agent_id: String,
    pub kind: &'static str,
    pub outcome: String,
    pub completion_count: usize,
    pub subagent_type: Option<String>,
    pub subagent_agent_id: Option<String>,
    pub is_quiet_origin: bool,
    pub reason: Option<String>,
    pub sent_message_count: Option<usize>,
}

pub trait CompletionRevivalRuntimePort: Send + Sync {
    fn can_execute(&self) -> bool;
    fn is_agent_deleted(&self, agent_id: &str) -> bool;
    fn is_agent_gone(&self, agent_id: &str) -> bool;
    fn clear_pending_wake(&self, agent_id: &str, kind: PendingWakeKind, work_id: &str);
    fn run_background_revival(
        &self,
        agent_id: &str,
        source: &str,
        prompt: &str,
        is_silence_allowed: bool,
        auto_review_epoch: &str,
    ) -> Result<RevivalExecution, String>;
    fn mark_resume_pending_for_quiesced_revival(&self, agent_id: &str);
    fn report_revival(&self, report: RevivalReport);
    fn report_revival_error(&self, agent_id: &str, title: &str, error: &str);
}

#[derive(Default)]
struct CompletionRevivalState {
    pending_subagent: HashMap<String, Vec<SubagentCompletion>>,
    reviving_subagent: HashSet<String>,
    pending_shell: HashMap<String, Vec<ShellCompletion>>,
    reviving_shell: HashSet<String>,
}

pub struct CompletionRevivals {
    runtime: Arc<dyn CompletionRevivalRuntimePort>,
    state: Mutex<CompletionRevivalState>,
}

impl CompletionRevivals {
    pub fn new(runtime: Arc<dyn CompletionRevivalRuntimePort>) -> Self {
        Self { runtime, state: Mutex::new(CompletionRevivalState::default()) }
    }

    pub fn pending_subagent_completions(&self, agent_id: &str) -> Vec<SubagentCompletion> {
        self.state.lock().unwrap().pending_subagent.get(agent_id).cloned().unwrap_or_default()
    }

    pub fn pending_shell_completions(&self, agent_id: &str) -> Vec<ShellCompletion> {
        self.state.lock().unwrap().pending_shell.get(agent_id).cloned().unwrap_or_default()
    }

    pub fn clear_agent_pending_completions(&self, agent_id: &str) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.pending_subagent.remove(agent_id);
        state.pending_shell.remove(agent_id);
    }

    /// Frozen SandHost health treats a completion revival that is actively
    /// draining as background work even after its durable marker has been
    /// removed. Read the single CompletionRevivals owner instead of inventing
    /// a parallel Host-side busy flag.
    pub fn has_mid_drain_revival(&self) -> bool {
        !self.mid_drain_revival_agent_ids().is_empty()
    }

    pub fn mid_drain_revival_agent_ids(&self) -> Vec<String> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut agent_ids = state
            .reviving_subagent
            .iter()
            .chain(state.reviving_shell.iter())
            .cloned()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        agent_ids.sort();
        agent_ids
    }

    pub fn handle_background_subagent_completion(&self, completion: SubagentCompletion) {
        if self.runtime.is_agent_deleted(&completion.parent_agent_id) {
            self.runtime.report_revival(RevivalReport {
                agent_id: completion.parent_agent_id.clone(),
                kind: "subagent",
                outcome: "dropped".into(),
                completion_count: 1,
                subagent_type: Some(completion.subagent_type.clone()),
                subagent_agent_id: Some(completion.subagent_agent_id.clone()),
                is_quiet_origin: completion.quiet_origin.is_some(),
                reason: Some("agent_deleted".into()),
                sent_message_count: None,
            });
            return;
        }

        let agent_id = completion.parent_agent_id.clone();
        {
            let mut state = self.state.lock().unwrap();
            let queue = state.pending_subagent.entry(agent_id.clone()).or_default();
            if queue.iter().any(|item| item.subagent_agent_id == completion.subagent_agent_id) {
                return;
            }
            queue.push(completion);
        }
        self.revive_for_subagent_completions(&agent_id);
    }

    pub fn revive_for_subagent_completions(&self, agent_id: &str) {
        if !self.runtime.can_execute() {
            return;
        }
        {
            let mut state = self.state.lock().unwrap();
            if !state.reviving_subagent.insert(agent_id.to_string()) {
                return;
            }
        }

        loop {
            let completions = {
                let mut state = self.state.lock().unwrap();
                state.pending_subagent.remove(agent_id).unwrap_or_default()
            };
            if completions.is_empty() {
                break;
            }
            for completion in &completions {
                self.runtime.clear_pending_wake(
                    agent_id,
                    if completion.subagent_type == "cursor-agent" {
                        PendingWakeKind::CloudAgent
                    } else {
                        PendingWakeKind::Subagent
                    },
                    &completion.subagent_agent_id,
                );
            }
            let report = self.run_subagent_revival(agent_id, &completions);
            self.runtime.report_revival(RevivalReport {
                agent_id: agent_id.to_string(),
                kind: "subagent",
                completion_count: completions.len(),
                subagent_type: completions.first().map(|item| item.subagent_type.clone()),
                subagent_agent_id: None,
                is_quiet_origin: is_all_quiet_subagent(&completions),
                ..report
            });
        }

        self.state.lock().unwrap().reviving_subagent.remove(agent_id);
    }

    fn run_subagent_revival(
        &self,
        agent_id: &str,
        completions: &[SubagentCompletion],
    ) -> RevivalReport {
        if completions.is_empty() {
            return base_report(agent_id, "subagent", "delivered");
        }
        if !self.runtime.can_execute() {
            return dropped_report(agent_id, "subagent", "no_runner");
        }
        if self.runtime.is_agent_gone(agent_id) {
            return dropped_report(agent_id, "subagent", "session_unavailable");
        }
        let prompt = build_subagent_revival_prompt(completions);
        match self.runtime.run_background_revival(
            agent_id,
            "subagent-revival",
            &prompt,
            true,
            "continue",
        ) {
            Ok(run) if run.aborted => base_report(agent_id, "subagent", "superseded"),
            Ok(run) if run.quiesced_for_upgrade => {
                self.runtime.mark_resume_pending_for_quiesced_revival(agent_id);
                dropped_report(agent_id, "subagent", "quiesced")
            }
            Ok(run) => RevivalReport {
                sent_message_count: Some(run.sent_message_count),
                ..base_report(agent_id, "subagent", "delivered")
            },
            Err(error) => {
                self.runtime.report_revival_error(
                    agent_id,
                    "Background task follow-up failed",
                    &error,
                );
                dropped_report(agent_id, "subagent", "error")
            }
        }
    }

    pub fn handle_background_shell_completion(&self, completion: ShellCompletion) {
        if self.runtime.is_agent_gone(&completion.agent_id) {
            self.runtime.report_revival(RevivalReport {
                agent_id: completion.agent_id,
                kind: "shell",
                outcome: "dropped".into(),
                completion_count: 1,
                subagent_type: None,
                subagent_agent_id: None,
                is_quiet_origin: completion.quiet_origin.is_some(),
                reason: Some("agent_gone".into()),
                sent_message_count: None,
            });
            return;
        }
        let agent_id = completion.agent_id.clone();
        self.state.lock().unwrap().pending_shell.entry(agent_id.clone()).or_default().push(completion);
        self.revive_for_shell_completions(&agent_id);
    }

    pub fn revive_for_shell_completions(&self, agent_id: &str) {
        if !self.runtime.can_execute() {
            return;
        }
        {
            let mut state = self.state.lock().unwrap();
            if !state.reviving_shell.insert(agent_id.to_string()) {
                return;
            }
        }
        loop {
            let completions = {
                let mut state = self.state.lock().unwrap();
                state.pending_shell.remove(agent_id).unwrap_or_default()
            };
            if completions.is_empty() {
                break;
            }
            for completion in &completions {
                self.runtime.clear_pending_wake(
                    agent_id,
                    PendingWakeKind::Shell,
                    &completion.shell_id,
                );
            }
            let report = self.run_shell_revival(agent_id, &completions);
            self.runtime.report_revival(report);
        }
        self.state.lock().unwrap().reviving_shell.remove(agent_id);
    }

    fn run_shell_revival(&self, agent_id: &str, completions: &[ShellCompletion]) -> RevivalReport {
        if completions.is_empty() {
            return base_report(agent_id, "shell", "delivered");
        }
        if !self.runtime.can_execute() {
            return shell_report(agent_id, completions, "dropped", Some("no_runner"), None);
        }
        if self.runtime.is_agent_gone(agent_id) {
            return shell_report(agent_id, completions, "dropped", Some("session_unavailable"), None);
        }
        let prompt = build_shell_revival_prompt(completions);
        match self.runtime.run_background_revival(
            agent_id,
            "shell-revival",
            &prompt,
            true,
            "continue",
        ) {
            Ok(run) if run.aborted => shell_report(agent_id, completions, "superseded", None, None),
            Ok(run) if run.quiesced_for_upgrade => {
                self.runtime.mark_resume_pending_for_quiesced_revival(agent_id);
                shell_report(agent_id, completions, "dropped", Some("quiesced"), None)
            }
            Ok(run) => shell_report(
                agent_id,
                completions,
                "delivered",
                None,
                Some(run.sent_message_count),
            ),
            Err(error) => {
                self.runtime.report_revival_error(
                    agent_id,
                    "Background command follow-up failed",
                    &error,
                );
                shell_report(agent_id, completions, "dropped", Some("error"), None)
            }
        }
    }
}

pub fn describe_quiet_origin_note(origin: &QuietWakeOrigin) -> String {
    let source = origin
        .automation
        .as_ref()
        .map(|automation| {
            format!(
                "your routine \"{}\" (folder {})",
                automation.name, automation.id
            )
        })
        .unwrap_or_else(|| "one of your own quiet self-initiated runs".into());
    format!("(You started this during {source} — nobody is waiting on it.)")
}

pub fn is_all_quiet_subagent(completions: &[SubagentCompletion]) -> bool {
    completions.iter().all(|completion| completion.quiet_origin.is_some())
}

pub fn is_all_quiet_shell(completions: &[ShellCompletion]) -> bool {
    completions.iter().all(|completion| completion.quiet_origin.is_some())
}

pub fn build_subagent_revival_prompt(completions: &[SubagentCompletion]) -> String {
    let blocks = completions
        .iter()
        .map(|completion| {
            let label = if completion.status == "error" {
                format!(
                    "Background task \"{}\" ({}) failed:",
                    completion.title, completion.subagent_type
                )
            } else {
                format!(
                    "Background task \"{}\" ({}) finished:",
                    completion.title, completion.subagent_type
                )
            };
            let mut value = format!("{label}\n{}", completion.result);
            if let Some(origin) = &completion.quiet_origin {
                value.push('\n');
                value.push_str(&describe_quiet_origin_note(origin));
            }
            value
        })
        .collect::<Vec<_>>();
    let intro = if completions.len() == 1 {
        "A background task you started has finished.".to_string()
    } else {
        format!("{} background tasks you started have finished.", completions.len())
    };
    let instruction = "Pick the work back up: review the result(s), then either keep going or wrap up. If this result is genuinely new and relevant to the user, or the user asked to be told when this finished, tell them with a SendMessage. Lead with the concrete thing that finished, not a bare pronoun like \"That\" (they cannot see the background task). If it is stale, irrelevant, already handled, or a duplicate, and the user was not waiting on it, just stay silent and end the turn with no SendMessage rather than narrating it. Keep your status current, and clear it once everything is done and you're idle.";
    [
        format!("[A background task just completed] {intro}"),
        String::new(),
        blocks.join("\n\n"),
        String::new(),
        if is_all_quiet_subagent(completions) {
            QUIET_REVIVAL_INSTRUCTION.to_string()
        } else {
            instruction.to_string()
        },
    ]
    .join("\n")
}

pub fn describe_shell_outcome(status: &str) -> &'static str {
    match status {
        "success" => "finished",
        "aborted" => "was stopped",
        _ => "failed",
    }
}

pub fn build_shell_revival_prompt(completions: &[ShellCompletion]) -> String {
    let blocks = completions
        .iter()
        .map(|completion| {
            let mut lines = vec![format!(
                "Background command \"{}\" {}.",
                completion.title,
                describe_shell_outcome(&completion.status)
            )];
            if let Some(detail) = &completion.detail {
                lines.push(detail.clone());
            }
            if let Some(path) = &completion.output_path {
                lines.push(format!("Full output: {path}"));
            }
            if let Some(origin) = &completion.quiet_origin {
                lines.push(describe_quiet_origin_note(origin));
            }
            lines.join("\n")
        })
        .collect::<Vec<_>>();
    let intro = if completions.len() == 1 {
        "A command you started in the background has finished.".to_string()
    } else {
        format!(
            "{} commands you started in the background have finished.",
            completions.len()
        )
    };
    let instruction = "Pick the work back up: check the result (read the output file if you need the full logs), then either keep going or wrap up. If this result is genuinely new and relevant to the user, or the user asked to be told when this finished, tell them with a SendMessage. Lead with the concrete thing that finished, not a bare pronoun like \"That\" (they cannot see the background task). If it is stale, irrelevant, already handled, or a duplicate, and the user was not waiting on it, just stay silent and end the turn with no SendMessage rather than narrating it. Keep your status current, and clear it once everything is done and you're idle.";
    [
        format!("[A background command just completed] {intro}"),
        String::new(),
        blocks.join("\n\n"),
        String::new(),
        if is_all_quiet_shell(completions) {
            QUIET_REVIVAL_INSTRUCTION.to_string()
        } else {
            instruction.to_string()
        },
    ]
    .join("\n")
}

fn base_report(agent_id: &str, kind: &'static str, outcome: &str) -> RevivalReport {
    RevivalReport {
        agent_id: agent_id.into(),
        kind,
        outcome: outcome.into(),
        completion_count: 0,
        subagent_type: None,
        subagent_agent_id: None,
        is_quiet_origin: false,
        reason: None,
        sent_message_count: None,
    }
}

fn dropped_report(agent_id: &str, kind: &'static str, reason: &str) -> RevivalReport {
    RevivalReport {
        reason: Some(reason.into()),
        ..base_report(agent_id, kind, "dropped")
    }
}

fn shell_report(
    agent_id: &str,
    completions: &[ShellCompletion],
    outcome: &str,
    reason: Option<&str>,
    sent_message_count: Option<usize>,
) -> RevivalReport {
    RevivalReport {
        agent_id: agent_id.into(),
        kind: "shell",
        outcome: outcome.into(),
        completion_count: completions.len(),
        subagent_type: None,
        subagent_agent_id: None,
        is_quiet_origin: is_all_quiet_shell(completions),
        reason: reason.map(str::to_string),
        sent_message_count,
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use super::super::sand_pending_wake_store::{AutomationQuietOrigin, QuietWakeOrigin};

    fn quiet() -> QuietWakeOrigin {
        QuietWakeOrigin {
            automation: Some(AutomationQuietOrigin {
                id: "routine-1".into(),
                name: "Watch build".into(),
            }),
        }
    }

    #[test]
    fn quiet_subagent_prompt_suppresses_progress_chatter() {
        let prompt = build_subagent_revival_prompt(&[SubagentCompletion {
            parent_agent_id: "parent".into(),
            subagent_agent_id: "child".into(),
            title: "Check CI".into(),
            subagent_type: "subagent".into(),
            status: "success".into(),
            result: "No change".into(),
            quiet_origin: Some(quiet()),
        }]);
        assert!(prompt.contains(QUIET_REVIVAL_INSTRUCTION));
        assert!(prompt.contains("nobody is waiting on it"));
        assert!(is_all_quiet_subagent(&[SubagentCompletion {
            parent_agent_id: "parent".into(),
            subagent_agent_id: "child".into(),
            title: "Check CI".into(),
            subagent_type: "subagent".into(),
            status: "success".into(),
            result: "No change".into(),
            quiet_origin: Some(quiet()),
        }]));
    }

    #[test]
    fn shell_prompt_preserves_outcome_and_output_path() {
        let prompt = build_shell_revival_prompt(&[ShellCompletion {
            agent_id: "agent".into(),
            shell_id: "shell-1".into(),
            title: "Build".into(),
            status: "aborted".into(),
            detail: Some("Stopped by user".into()),
            output_path: Some("/private/output.log".into()),
            quiet_origin: None,
        }]);
        assert!(prompt.contains("Background command \"Build\" was stopped."));
        assert!(prompt.contains("Stopped by user"));
        assert!(prompt.contains("Full output: /private/output.log"));
        assert_eq!(describe_shell_outcome("success"), "finished");
        assert_eq!(describe_shell_outcome("aborted"), "was stopped");
        assert_eq!(describe_shell_outcome("error"), "failed");
    }
}
