use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutomationRunState {
    Running,
    AwaitingApproval,
    Completed,
    Failed,
    Cancelled,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AutomationSpec {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub schedule: String,
    pub enabled: bool,
    pub account_fence: String,
    pub created_at_ms: u64,
    pub last_run_at_ms: Option<u64>,
    pub next_run_at_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AutomationRun {
    pub run_id: String,
    pub automation_id: String,
    pub request_id: String,
    pub account_fence: String,
    pub generation: u64,
    pub state: AutomationRunState,
    pub step_index: u32,
    pub step_mutation_id: Option<String>,
    pub approval_id: Option<String>,
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
    pub reason: Option<String>,
}

#[derive(Default, Deserialize, Serialize)]
struct DurableAutomationState {
    next_generation: u64,
    specs: BTreeMap<String, AutomationSpec>,
    runs: BTreeMap<String, AutomationRun>,
    request_to_run: BTreeMap<String, String>,
}

pub struct AutomationRuntime {
    path: PathBuf,
    state: DurableAutomationState,
}

impl AutomationRuntime {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let path = path.into();
        let mut state = if path.exists() {
            serde_json::from_slice::<DurableAutomationState>(
                &fs::read(&path).map_err(|e| e.to_string())?,
            )
            .map_err(|e| format!("invalid automation runtime store: {e}"))?
        } else {
            DurableAutomationState::default()
        };

        let mut changed = false;
        for run in state.runs.values_mut() {
            if run.state == AutomationRunState::Running {
                run.state = AutomationRunState::OutcomeUnknown;
                run.reason = Some(
                    "Host restarted while workflow step may have produced side effects; reconcile before retry"
                        .into(),
                );
                run.updated_at_ms = now_ms;
                changed = true;
            }
        }
        let runtime = Self { path, state };
        if changed {
            runtime.persist()?;
        }
        Ok(runtime)
    }

    pub fn upsert_spec(&mut self, spec: AutomationSpec) -> Result<(), String> {
        validate_spec(&spec)?;
        if let Some(existing) = self.state.specs.get(&spec.id) {
            if existing.account_fence != spec.account_fence {
                return Err("automation id belongs to another account fence".into());
            }
        }
        self.state.specs.insert(spec.id.clone(), spec);
        self.persist()
    }

    pub fn list_for_account(&self, account_fence: &str) -> Vec<Value> {
        self.state
            .specs
            .values()
            .filter(|spec| spec.account_fence == account_fence)
            .map(|spec| serde_json::to_value(spec).unwrap_or(Value::Null))
            .collect()
    }

    pub fn begin_run(
        &mut self,
        automation_id: &str,
        request_id: &str,
        run_id: &str,
        account_fence: &str,
        now_ms: u64,
    ) -> Result<AutomationRun, String> {
        let spec = self
            .state
            .specs
            .get(automation_id)
            .ok_or("automation not found")?;
        if !spec.enabled {
            return Err("automation is disabled".into());
        }
        if spec.account_fence != account_fence {
            return Err("automation account fence mismatch".into());
        }
        if let Some(existing_run_id) = self.state.request_to_run.get(request_id) {
            let existing = self
                .state
                .runs
                .get(existing_run_id)
                .ok_or("automation request index is corrupt")?;
            if existing.automation_id == automation_id
                && existing.account_fence == account_fence
                && matches!(
                    existing.state,
                    AutomationRunState::Completed
                        | AutomationRunState::Failed
                        | AutomationRunState::Cancelled
                )
            {
                return Ok(existing.clone());
            }
            return Err(match existing.state {
                AutomationRunState::OutcomeUnknown => {
                    "automation request outcome is unknown; reconcile before replay".into()
                }
                _ => "duplicate automation request is already active or bound".into(),
            });
        }
        if self.state.runs.contains_key(run_id) {
            return Err("duplicate automation run id".into());
        }

        self.state.next_generation = self.state.next_generation.saturating_add(1).max(1);
        let run = AutomationRun {
            run_id: run_id.into(),
            automation_id: automation_id.into(),
            request_id: request_id.into(),
            account_fence: account_fence.into(),
            generation: self.state.next_generation,
            state: AutomationRunState::Running,
            step_index: 0,
            step_mutation_id: Some(format!("{request_id}:step:0")),
            approval_id: None,
            started_at_ms: now_ms,
            updated_at_ms: now_ms,
            reason: None,
        };
        self.state
            .request_to_run
            .insert(request_id.into(), run_id.into());
        self.state.runs.insert(run_id.into(), run.clone());
        self.persist()?;
        Ok(run)
    }

    pub fn advance_step(
        &mut self,
        run_id: &str,
        account_fence: &str,
        generation: u64,
        now_ms: u64,
    ) -> Result<AutomationRun, String> {
        let run = self.current_mut(run_id, account_fence, generation)?;
        if run.state != AutomationRunState::Running {
            return Err("automation run is not running".into());
        }
        run.step_index = run.step_index.saturating_add(1);
        run.step_mutation_id =
            Some(format!("{}:step:{}", run.request_id, run.step_index));
        run.updated_at_ms = now_ms;
        let copy = run.clone();
        self.persist()?;
        Ok(copy)
    }

    pub fn await_approval(
        &mut self,
        run_id: &str,
        account_fence: &str,
        generation: u64,
        approval_id: &str,
        now_ms: u64,
    ) -> Result<AutomationRun, String> {
        if approval_id.trim().is_empty() {
            return Err("approval id is required".into());
        }
        let run = self.current_mut(run_id, account_fence, generation)?;
        if run.state != AutomationRunState::Running {
            return Err("automation run is not running".into());
        }
        run.state = AutomationRunState::AwaitingApproval;
        run.approval_id = Some(approval_id.into());
        run.updated_at_ms = now_ms;
        let copy = run.clone();
        self.persist()?;
        Ok(copy)
    }

    pub fn resolve_approval(
        &mut self,
        run_id: &str,
        account_fence: &str,
        generation: u64,
        approval_id: &str,
        allowed: bool,
        now_ms: u64,
    ) -> Result<AutomationRun, String> {
        let run = self.current_mut(run_id, account_fence, generation)?;
        if run.state != AutomationRunState::AwaitingApproval
            || run.approval_id.as_deref() != Some(approval_id)
        {
            return Err("stale or mismatched automation approval".into());
        }
        run.approval_id = None;
        run.state = if allowed {
            AutomationRunState::Running
        } else {
            AutomationRunState::Cancelled
        };
        run.reason = (!allowed).then(|| "approval denied".into());
        run.updated_at_ms = now_ms;
        let copy = run.clone();
        self.persist()?;
        Ok(copy)
    }

    pub fn cancel(
        &mut self,
        run_id: &str,
        account_fence: &str,
        generation: u64,
        reason: &str,
        now_ms: u64,
    ) -> Result<AutomationRun, String> {
        let run = self.current_mut(run_id, account_fence, generation)?;
        if matches!(
            run.state,
            AutomationRunState::Completed
                | AutomationRunState::Failed
                | AutomationRunState::Cancelled
                | AutomationRunState::OutcomeUnknown
        ) {
            return Ok(run.clone());
        }
        run.state = AutomationRunState::Cancelled;
        run.approval_id = None;
        run.reason = Some(reason.into());
        run.updated_at_ms = now_ms;
        let copy = run.clone();
        self.persist()?;
        Ok(copy)
    }

    pub fn settle(
        &mut self,
        run_id: &str,
        account_fence: &str,
        generation: u64,
        succeeded: bool,
        reason: Option<String>,
        now_ms: u64,
    ) -> Result<AutomationRun, String> {
        let run = self.current_mut(run_id, account_fence, generation)?;
        if run.state != AutomationRunState::Running {
            return Err("automation run cannot settle from current state".into());
        }
        run.state = if succeeded {
            AutomationRunState::Completed
        } else {
            AutomationRunState::Failed
        };
        run.reason = reason;
        run.updated_at_ms = now_ms;
        let automation_id = run.automation_id.clone();
        let copy = run.clone();
        if let Some(spec) = self.state.specs.get_mut(&automation_id) {
            spec.last_run_at_ms = Some(now_ms);
        }
        self.persist()?;
        Ok(copy)
    }

    pub fn snapshot(&self, run_id: &str) -> Option<Value> {
        self.state
            .runs
            .get(run_id)
            .and_then(|run| serde_json::to_value(run).ok())
    }

    fn current_mut(
        &mut self,
        run_id: &str,
        account_fence: &str,
        generation: u64,
    ) -> Result<&mut AutomationRun, String> {
        let run = self.state.runs.get_mut(run_id).ok_or("automation run not found")?;
        if run.account_fence != account_fence || run.generation != generation {
            return Err("stale automation callback fenced by account/generation".into());
        }
        Ok(run)
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temp = self.path.with_extension("json.tmp");
        fs::write(
            &temp,
            serde_json::to_vec_pretty(&self.state).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(temp, &self.path).map_err(|e| e.to_string())
    }
}

fn validate_spec(spec: &AutomationSpec) -> Result<(), String> {
    if spec.id.trim().is_empty() || spec.id.len() > 160 {
        return Err("automation id is invalid".into());
    }
    if spec.name.trim().is_empty() || spec.name.len() > 80 {
        return Err("automation name is invalid".into());
    }
    if spec.prompt.trim().is_empty() || spec.prompt.len() > 100_000 {
        return Err("automation prompt is invalid".into());
    }
    if spec.schedule.trim().is_empty() || spec.schedule.len() > 512 {
        return Err("automation schedule is invalid".into());
    }
    if spec.account_fence.trim().is_empty() {
        return Err("automation account fence is required".into());
    }
    Ok(())
}

pub fn run_json(run: &AutomationRun) -> Value {
    serde_json::to_value(run).unwrap_or_else(|_| json!({"error":"serialization_failed"}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(account: &str) -> AutomationSpec {
        AutomationSpec {
            id: "daily-brief".into(),
            name: "Daily brief".into(),
            prompt: "Summarize updates".into(),
            schedule: "0 8 * * *".into(),
            enabled: true,
            account_fence: account.into(),
            created_at_ms: 1,
            last_run_at_ms: None,
            next_run_at_ms: Some(100),
        }
    }

    #[test]
    fn duplicate_request_converges_only_after_known_terminal_state() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = AutomationRuntime::open(dir.path().join("automation.json"), 1).unwrap();
        runtime.upsert_spec(spec("acct-a")).unwrap();
        let run = runtime
            .begin_run("daily-brief", "request-1", "run-1", "acct-a", 2)
            .unwrap();
        assert!(runtime
            .begin_run("daily-brief", "request-1", "run-2", "acct-a", 3)
            .is_err());
        runtime
            .settle("run-1", "acct-a", run.generation, true, None, 4)
            .unwrap();
        let replay = runtime
            .begin_run("daily-brief", "request-1", "run-2", "acct-a", 5)
            .unwrap();
        assert_eq!(replay.run_id, "run-1");
    }

    #[test]
    fn restart_turns_running_side_effect_into_outcome_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("automation.json");
        let mut runtime = AutomationRuntime::open(&path, 1).unwrap();
        runtime.upsert_spec(spec("acct-a")).unwrap();
        runtime
            .begin_run("daily-brief", "request-1", "run-1", "acct-a", 2)
            .unwrap();
        drop(runtime);
        let mut recovered = AutomationRuntime::open(&path, 3).unwrap();
        assert!(recovered
            .begin_run("daily-brief", "request-1", "run-2", "acct-a", 4)
            .is_err());
        assert_eq!(
            recovered
                .snapshot("run-1")
                .unwrap()
                .get("state")
                .and_then(Value::as_str),
            Some("outcome_unknown")
        );
    }

    #[test]
    fn approval_and_account_generation_are_fenced() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = AutomationRuntime::open(dir.path().join("automation.json"), 1).unwrap();
        runtime.upsert_spec(spec("acct-a")).unwrap();
        let run = runtime
            .begin_run("daily-brief", "request-1", "run-1", "acct-a", 2)
            .unwrap();
        runtime
            .await_approval("run-1", "acct-a", run.generation, "approval-1", 3)
            .unwrap();
        assert!(runtime
            .resolve_approval(
                "run-1",
                "acct-b",
                run.generation,
                "approval-1",
                true,
                4
            )
            .is_err());
        assert!(runtime
            .resolve_approval(
                "run-1",
                "acct-a",
                run.generation + 1,
                "approval-1",
                true,
                4
            )
            .is_err());
        assert!(runtime
            .resolve_approval(
                "run-1",
                "acct-a",
                run.generation,
                "other",
                true,
                4
            )
            .is_err());
        let resumed = runtime
            .resolve_approval(
                "run-1",
                "acct-a",
                run.generation,
                "approval-1",
                true,
                5
            )
            .unwrap();
        assert_eq!(resumed.state, AutomationRunState::Running);
    }

    #[test]
    fn step_mutation_identity_is_stable_and_monotonic() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = AutomationRuntime::open(dir.path().join("automation.json"), 1).unwrap();
        runtime.upsert_spec(spec("acct-a")).unwrap();
        let run = runtime
            .begin_run("daily-brief", "request-1", "run-1", "acct-a", 2)
            .unwrap();
        assert_eq!(run.step_mutation_id.as_deref(), Some("request-1:step:0"));
        let next = runtime
            .advance_step("run-1", "acct-a", run.generation, 3)
            .unwrap();
        assert_eq!(next.step_mutation_id.as_deref(), Some("request-1:step:1"));
    }
}
