use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub const SUBAGENT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentLineage {
    pub parent_request_id: Option<String>,
    pub root_parent_request_id: Option<String>,
    pub parent_agent_tool_call_id: Option<String>,
}

pub fn compute_subagent_request_id(tool_call_id: &str) -> String {
    let tool_call_id = tool_call_id.trim();
    if tool_call_id.is_empty() {
        "subagent".to_string()
    } else {
        format!("subagent:{tool_call_id}")
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerUseUsageSnapshot {
    pub model_id: Option<String>,
    pub turn_ended_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentSessionSnapshot {
    #[serde(default)]
    pub resolved_outline: Vec<Value>,
    #[serde(default)]
    pub observed_tool_call_count: usize,
    #[serde(default)]
    pub recent_activity: Vec<String>,
    pub transcript_path: Option<String>,
    pub computer_use_usage: Option<ComputerUseUsageSnapshot>,
    #[serde(default)]
    pub computer_use_action_counts: BTreeMap<String, u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubagentStatus {
    Running,
    Completed,
    Failed,
    Aborted,
    OutcomeUnknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DurableSubagentRecord {
    pub subagent_id: String,
    pub subagent_request_id: String,
    pub parent_agent_id: String,
    pub lineage: SubagentLineage,
    pub tool_call_id: String,
    pub subagent_type: String,
    pub title: String,
    pub prompt: String,
    pub account_fence: String,
    pub process_epoch: u64,
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
    pub status: SubagentStatus,
    pub pending_wake: bool,
    pub quiet_origin: Option<String>,
    pub box_id: String,
    pub pending_steer: Option<String>,
    pub completion_result: Option<String>,
    pub completion_error: Option<String>,
    #[serde(default)]
    pub snapshot: SubagentSessionSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubagentLaunch {
    pub record: DurableSubagentRecord,
    pub duplicate: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubagentRunOutcome {
    Completed(String),
    Failed(String),
    Aborted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubagentContinuation {
    pub prompt: String,
    pub subagent_request_id: String,
    pub lineage: SubagentLineage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerUseUsageEvent {
    pub parent_agent_id: String,
    pub subagent_agent_id: String,
    pub subagent_type: String,
    pub subagent_request_id: String,
    pub model_id: Option<String>,
    pub outcome: String,
    pub duration_ms: u64,
    pub tool_call_count: usize,
    pub turn_ended_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerUseAuditRecord {
    pub agent_id: String,
    pub turn_id: Option<String>,
    pub box_id: String,
    pub occurred_at_ms: u64,
    pub tool_call_id: String,
    pub action_count: u64,
    pub action_counts: BTreeMap<String, u64>,
    pub duration_ms: u64,
    pub screenshot_count: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubagentSettlement {
    pub continuation: Option<SubagentContinuation>,
    pub completion: Option<DurableSubagentRecord>,
    pub pending_wake_disarmed: Option<(String, String)>,
    pub computer_use_usage: Option<ComputerUseUsageEvent>,
    pub computer_use_audit: Option<ComputerUseAuditRecord>,
    pub ignored_stale_callback: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DurableSubagentDocument {
    schema_version: u32,
    process_epoch: u64,
    #[serde(default)]
    records: BTreeMap<String, DurableSubagentRecord>,
    #[serde(default)]
    request_index: BTreeMap<String, String>,
}

impl Default for DurableSubagentDocument {
    fn default() -> Self {
        Self {
            schema_version: SUBAGENT_SCHEMA_VERSION,
            process_epoch: 0,
            records: BTreeMap::new(),
            request_index: BTreeMap::new(),
        }
    }
}

pub struct DurableSubagentOwner {
    path: PathBuf,
    state: DurableSubagentDocument,
    controls: BTreeMap<String, Arc<AtomicBool>>,
    aborting: BTreeSet<String>,
}

impl DurableSubagentOwner {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let path = path.into();
        let mut state = match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str::<DurableSubagentDocument>(&raw)
                .map_err(|error| format!("invalid durable subagent state: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => DurableSubagentDocument::default(),
            Err(error) => return Err(format!("failed to read durable subagent state: {error}")),
        };
        if state.schema_version != SUBAGENT_SCHEMA_VERSION {
            return Err("unsupported durable subagent schema version".into());
        }
        state.process_epoch = state.process_epoch.saturating_add(1).max(1);
        let mut changed = false;
        for record in state.records.values_mut() {
            if record.status == SubagentStatus::Running {
                record.status = SubagentStatus::OutcomeUnknown;
                record.pending_wake = false;
                record.pending_steer = None;
                record.completion_error = Some("process-reopen-outcome-unknown".into());
                record.updated_at_ms = now_ms;
                changed = true;
            }
        }
        let owner = Self {
            path,
            state,
            controls: BTreeMap::new(),
            aborting: BTreeSet::new(),
        };
        if changed || !owner.path.exists() {
            owner.persist()?;
        }
        Ok(owner)
    }

    pub fn process_epoch(&self) -> u64 {
        self.state.process_epoch
    }

    pub fn launch(
        &mut self,
        parent_agent_id: &str,
        lineage: SubagentLineage,
        box_id: &str,
        subagent_type: &str,
        tool_call_id: &str,
        prompt: &str,
        account_fence: &str,
        quiet_origin: Option<&str>,
        now_ms: u64,
    ) -> Result<SubagentLaunch, String> {
        let parent_agent_id = required(parent_agent_id, "parentAgentId")?;
        let subagent_type = required(subagent_type, "subagentType")?;
        let tool_call_id = required(tool_call_id, "toolCallId")?;
        let prompt = required(prompt, "prompt")?;
        let account_fence = required(account_fence, "accountFence")?;
        let request_id = compute_subagent_request_id(tool_call_id);

        if let Some(existing_id) = self.state.request_index.get(&request_id).cloned() {
            let existing = self
                .state
                .records
                .get(&existing_id)
                .cloned()
                .ok_or("subagent request index is corrupt")?;
            let same_identity = existing.parent_agent_id == parent_agent_id
                && existing.tool_call_id == tool_call_id
                && existing.subagent_type == subagent_type
                && existing.prompt == prompt
                && existing.account_fence == account_fence
                && existing.lineage == lineage;
            if !same_identity {
                return Err("duplicate subagent request identity has mismatched frozen launch input".into());
            }
            return Ok(SubagentLaunch {
                record: existing,
                duplicate: true,
            });
        }

        let material = format!("{parent_agent_id}\n{request_id}");
        let subagent_id = format!(
            "generated:{}",
            crate::sha256::sha256_hex(material.as_bytes())
        );
        let title = derive_title(prompt);
        let record = DurableSubagentRecord {
            subagent_id: subagent_id.clone(),
            subagent_request_id: request_id.clone(),
            parent_agent_id: parent_agent_id.to_string(),
            lineage,
            tool_call_id: tool_call_id.to_string(),
            subagent_type: subagent_type.to_string(),
            title,
            prompt: prompt.to_string(),
            account_fence: account_fence.to_string(),
            process_epoch: self.state.process_epoch,
            started_at_ms: now_ms,
            updated_at_ms: now_ms,
            status: SubagentStatus::Running,
            pending_wake: true,
            quiet_origin: quiet_origin.map(ToOwned::to_owned),
            box_id: box_id.trim().to_string(),
            pending_steer: None,
            completion_result: None,
            completion_error: None,
            snapshot: SubagentSessionSnapshot::default(),
        };
        self.state
            .request_index
            .insert(request_id, subagent_id.clone());
        self.state.records.insert(subagent_id, record.clone());
        self.persist()?;
        Ok(SubagentLaunch {
            record,
            duplicate: false,
        })
    }

    pub fn attach_control(&mut self, subagent_id: &str, token: Arc<AtomicBool>) -> Result<(), String> {
        let record = self
            .state
            .records
            .get(subagent_id)
            .ok_or("subagent is unknown")?;
        if record.status != SubagentStatus::Running {
            return Err("subagent is not running".into());
        }
        self.controls.insert(subagent_id.to_string(), token);
        Ok(())
    }

    pub fn update_snapshot(
        &mut self,
        subagent_id: &str,
        account_fence: &str,
        callback_epoch: u64,
        snapshot: SubagentSessionSnapshot,
        now_ms: u64,
    ) -> Result<bool, String> {
        if !self.callback_is_current(subagent_id, account_fence, callback_epoch)? {
            return Ok(false);
        }
        let record = self.state.records.get_mut(subagent_id).unwrap();
        record.snapshot = snapshot;
        record.updated_at_ms = now_ms;
        self.persist()?;
        Ok(true)
    }

    pub fn steer(
        &mut self,
        subagent_id: &str,
        message: &str,
        account_fence: &str,
        callback_epoch: u64,
        now_ms: u64,
    ) -> Result<bool, String> {
        let message = required(message, "message")?.to_string();
        if !self.callback_is_current(subagent_id, account_fence, callback_epoch)? {
            return Ok(false);
        }
        if self.aborting.contains(subagent_id) {
            return Ok(false);
        }
        let record = self.state.records.get_mut(subagent_id).unwrap();
        if record.status != SubagentStatus::Running {
            return Ok(false);
        }
        record.pending_steer = Some(message);
        record.updated_at_ms = now_ms;
        if let Some(control) = self.controls.get(subagent_id) {
            control.store(true, Ordering::Release);
        }
        self.persist()?;
        Ok(true)
    }

    pub fn abort(
        &mut self,
        subagent_id: &str,
        account_fence: &str,
        callback_epoch: u64,
        now_ms: u64,
    ) -> Result<bool, String> {
        if !self.callback_is_current(subagent_id, account_fence, callback_epoch)? {
            return Ok(false);
        }
        let record = self.state.records.get_mut(subagent_id).unwrap();
        if record.status != SubagentStatus::Running {
            return Ok(false);
        }
        record.status = SubagentStatus::Aborted;
        record.pending_wake = false;
        record.pending_steer = None;
        record.updated_at_ms = now_ms;
        record.completion_error = Some("aborted".into());
        self.aborting.insert(subagent_id.to_string());
        if let Some(control) = self.controls.get(subagent_id) {
            control.store(true, Ordering::Release);
        }
        self.persist()?;
        Ok(true)
    }

    pub fn abort_for_parent_request(
        &mut self,
        parent_request_id: &str,
        reason: &str,
        now_ms: u64,
    ) -> Result<Vec<String>, String> {
        let ids = self
            .state
            .records
            .values()
            .filter(|record| {
                record.status == SubagentStatus::Running
                    && record.lineage.parent_request_id.as_deref() == Some(parent_request_id)
            })
            .map(|record| record.subagent_id.clone())
            .collect::<Vec<_>>();
        for id in &ids {
            if let Some(record) = self.state.records.get_mut(id) {
                record.status = SubagentStatus::Aborted;
                record.pending_wake = false;
                record.pending_steer = None;
                record.updated_at_ms = now_ms;
                record.completion_error = Some(reason.to_string());
            }
            self.aborting.insert(id.clone());
            if let Some(control) = self.controls.get(id) {
                control.store(true, Ordering::Release);
            }
        }
        if !ids.is_empty() {
            self.persist()?;
        }
        Ok(ids)
    }

    pub fn mark_account_outcome_unknown(
        &mut self,
        account_fence: &str,
        reason: &str,
        now_ms: u64,
    ) -> Result<Vec<String>, String> {
        let mut ids = Vec::new();
        for record in self.state.records.values_mut() {
            if record.account_fence == account_fence && record.status == SubagentStatus::Running {
                record.status = SubagentStatus::OutcomeUnknown;
                record.pending_wake = false;
                record.pending_steer = None;
                record.completion_error = Some(reason.to_string());
                record.updated_at_ms = now_ms;
                if let Some(control) = self.controls.get(&record.subagent_id) {
                    control.store(true, Ordering::Release);
                }
                ids.push(record.subagent_id.clone());
            }
        }
        if !ids.is_empty() {
            self.persist()?;
        }
        Ok(ids)
    }

    pub fn settle(
        &mut self,
        subagent_id: &str,
        account_fence: &str,
        callback_epoch: u64,
        outcome: SubagentRunOutcome,
        now_ms: u64,
    ) -> Result<SubagentSettlement, String> {
        if !self.callback_is_current(subagent_id, account_fence, callback_epoch)? {
            return Ok(SubagentSettlement {
                ignored_stale_callback: true,
                ..SubagentSettlement::default()
            });
        }

        if self.aborting.remove(subagent_id)
            || self
                .state
                .records
                .get(subagent_id)
                .is_some_and(|record| record.status == SubagentStatus::Aborted)
        {
            self.controls.remove(subagent_id);
            let record = self.state.records.get(subagent_id).cloned().unwrap();
            return Ok(SubagentSettlement {
                pending_wake_disarmed: Some((
                    record.parent_agent_id.clone(),
                    record.subagent_id.clone(),
                )),
                ..SubagentSettlement::default()
            });
        }

        let pending_steer = self
            .state
            .records
            .get(subagent_id)
            .and_then(|record| record.pending_steer.clone());
        if let Some(message) = pending_steer {
            let record = self.state.records.get_mut(subagent_id).unwrap();
            record.pending_steer = None;
            record.updated_at_ms = now_ms;
            self.controls.remove(subagent_id);
            let mut lineage = record.lineage.clone();
            lineage.parent_agent_tool_call_id = Some(record.tool_call_id.clone());
            let continuation = SubagentContinuation {
                prompt: format!(
                    "<system_reminder>\nThe parent agent sent new guidance while you were working:\n{message}\nContinue the same task with this guidance.\n</system_reminder>"
                ),
                subagent_request_id: record.subagent_request_id.clone(),
                lineage,
            };
            self.persist()?;
            return Ok(SubagentSettlement {
                continuation: Some(continuation),
                ..SubagentSettlement::default()
            });
        }

        let record = self.state.records.get_mut(subagent_id).unwrap();
        record.pending_wake = false;
        record.updated_at_ms = now_ms;
        match &outcome {
            SubagentRunOutcome::Completed(text) => {
                record.status = SubagentStatus::Completed;
                record.completion_result = Some(if text.trim().is_empty() {
                    "(the task finished without producing any text output)".into()
                } else {
                    text.trim().to_string()
                });
                record.completion_error = None;
            }
            SubagentRunOutcome::Failed(error) => {
                record.status = SubagentStatus::Failed;
                record.completion_result = None;
                record.completion_error = Some(error.clone());
            }
            SubagentRunOutcome::Aborted => {
                record.status = SubagentStatus::Aborted;
                record.completion_result = None;
                record.completion_error = Some("aborted".into());
            }
        }
        let completion = record.clone();
        self.controls.remove(subagent_id);

        let duration_ms = now_ms.saturating_sub(completion.started_at_ms);
        let is_computer = normalized_type(&completion.subagent_type) == "computeruse";
        let (computer_use_usage, computer_use_audit) = if is_computer {
            let usage = completion.snapshot.computer_use_usage.clone().unwrap_or_default();
            let action_count = completion
                .snapshot
                .computer_use_action_counts
                .values()
                .copied()
                .sum();
            (
                Some(ComputerUseUsageEvent {
                    parent_agent_id: completion.parent_agent_id.clone(),
                    subagent_agent_id: completion.subagent_id.clone(),
                    subagent_type: completion.subagent_type.clone(),
                    subagent_request_id: completion.subagent_request_id.clone(),
                    model_id: usage.model_id.clone(),
                    outcome: status_label(completion.status).into(),
                    duration_ms,
                    tool_call_count: completion.snapshot.observed_tool_call_count,
                    turn_ended_count: usage.turn_ended_count,
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                }),
                Some(ComputerUseAuditRecord {
                    agent_id: completion.parent_agent_id.clone(),
                    turn_id: Some(completion.subagent_request_id.clone()),
                    box_id: completion.box_id.clone(),
                    occurred_at_ms: now_ms,
                    tool_call_id: completion.tool_call_id.clone(),
                    action_count,
                    action_counts: completion.snapshot.computer_use_action_counts.clone(),
                    duration_ms,
                    screenshot_count: completion
                        .snapshot
                        .computer_use_action_counts
                        .get("screenshot")
                        .copied()
                        .unwrap_or_default(),
                }),
            )
        } else {
            (None, None)
        };
        self.persist()?;
        Ok(SubagentSettlement {
            completion: Some(completion),
            computer_use_usage,
            computer_use_audit,
            ..SubagentSettlement::default()
        })
    }

    pub fn reconcile_outcome_unknown(
        &mut self,
        subagent_id: &str,
        account_fence: &str,
        outcome: SubagentRunOutcome,
        now_ms: u64,
    ) -> Result<DurableSubagentRecord, String> {
        let record = self
            .state
            .records
            .get_mut(subagent_id)
            .ok_or("subagent is unknown")?;
        if record.account_fence != account_fence {
            return Err("subagent account fence is stale".into());
        }
        if record.status != SubagentStatus::OutcomeUnknown {
            return Err("subagent is not awaiting outcome reconciliation".into());
        }
        record.process_epoch = self.state.process_epoch;
        record.updated_at_ms = now_ms;
        record.pending_wake = false;
        match outcome {
            SubagentRunOutcome::Completed(text) => {
                record.status = SubagentStatus::Completed;
                record.completion_result = Some(if text.trim().is_empty() {
                    "(the task finished without producing any text output)".into()
                } else {
                    text.trim().to_string()
                });
                record.completion_error = None;
            }
            SubagentRunOutcome::Failed(error) => {
                record.status = SubagentStatus::Failed;
                record.completion_result = None;
                record.completion_error = Some(error);
            }
            SubagentRunOutcome::Aborted => {
                record.status = SubagentStatus::Aborted;
                record.completion_result = None;
                record.completion_error = Some("aborted".into());
            }
        }
        let result = record.clone();
        self.persist()?;
        Ok(result)
    }

    pub fn get(&self, subagent_id: &str) -> Option<&DurableSubagentRecord> {
        self.state.records.get(subagent_id)
    }

    pub fn list_running_for_parent(&self, parent_agent_id: &str) -> Vec<DurableSubagentRecord> {
        let mut records = self
            .state
            .records
            .values()
            .filter(|record| {
                record.parent_agent_id == parent_agent_id
                    && record.status == SubagentStatus::Running
                    && !self.aborting.contains(&record.subagent_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            left.started_at_ms
                .cmp(&right.started_at_ms)
                .then_with(|| left.subagent_id.cmp(&right.subagent_id))
        });
        records
    }

    pub fn all_records(&self) -> Vec<DurableSubagentRecord> {
        self.state.records.values().cloned().collect()
    }

    fn callback_is_current(
        &self,
        subagent_id: &str,
        account_fence: &str,
        callback_epoch: u64,
    ) -> Result<bool, String> {
        let record = self.state.records.get(subagent_id).ok_or("subagent is unknown")?;
        Ok(record.account_fence == account_fence
            && callback_epoch == self.state.process_epoch
            && record.process_epoch == callback_epoch)
    }

    fn persist(&self) -> Result<(), String> {
        persist_document(&self.path, &self.state)
    }
}

fn persist_document(path: &Path, state: &DurableSubagentDocument) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create durable subagent directory: {error}"))?;
    }
    let raw = serde_json::to_vec_pretty(state)
        .map_err(|error| format!("failed to serialize durable subagent state: {error}"))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, raw)
        .map_err(|error| format!("failed to write durable subagent state: {error}"))?;
    fs::rename(&tmp, path)
        .map_err(|error| format!("failed to commit durable subagent state: {error}"))
}

fn required<'a>(value: &'a str, label: &str) -> Result<&'a str, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(value)
    }
}

fn normalized_type(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !matches!(ch, '-' | '_' | ' '))
        .collect::<String>()
        .to_ascii_lowercase()
}

fn derive_title(prompt: &str) -> String {
    let compact = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= 80 {
        compact
    } else {
        format!("{}…", compact.chars().take(79).collect::<String>())
    }
}

pub fn status_label(status: SubagentStatus) -> &'static str {
    match status {
        SubagentStatus::Running => "running",
        SubagentStatus::Completed => "completed",
        SubagentStatus::Failed => "failed",
        SubagentStatus::Aborted => "aborted",
        SubagentStatus::OutcomeUnknown => "outcome-unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-subagent-{label}-{}-{}.json",
            std::process::id(),
            crate::android_json_runtime::now_ms()
        ))
    }

    fn lineage(parent: &str) -> SubagentLineage {
        SubagentLineage {
            parent_request_id: Some(parent.into()),
            root_parent_request_id: Some("root".into()),
            parent_agent_tool_call_id: None,
        }
    }

    #[test]
    fn stable_identity_duplicate_and_mismatch_fail_closed() {
        let path = root("duplicate");
        let mut owner = DurableSubagentOwner::open(&path, 10).unwrap();
        let first = owner
            .launch(
                "parent", lineage("req"), "box", "general-purpose", "call-1", "work",
                "acct", None, 11,
            )
            .unwrap();
        assert!(!first.duplicate);
        assert_eq!(first.record.subagent_request_id, "subagent:call-1");
        let duplicate = owner
            .launch(
                "parent", lineage("req"), "box", "general-purpose", "call-1", "work",
                "acct", None, 12,
            )
            .unwrap();
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.record.subagent_id, first.record.subagent_id);
        assert!(owner
            .launch(
                "parent", lineage("req"), "box", "general-purpose", "call-1", "different",
                "acct", None, 13,
            )
            .unwrap_err()
            .contains("mismatched"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn steer_continues_same_request_and_stop_wins_late_completion() {
        let path = root("steer");
        let mut owner = DurableSubagentOwner::open(&path, 10).unwrap();
        let launch = owner
            .launch(
                "parent", lineage("req"), "box", "general-purpose", "call-2", "work",
                "acct", None, 11,
            )
            .unwrap();
        let epoch = owner.process_epoch();
        let token = Arc::new(AtomicBool::new(false));
        owner.attach_control(&launch.record.subagent_id, Arc::clone(&token)).unwrap();
        assert!(owner
            .steer(&launch.record.subagent_id, "redirect", "acct", epoch, 12)
            .unwrap());
        assert!(token.load(Ordering::Acquire));
        let continuation = owner
            .settle(
                &launch.record.subagent_id,
                "acct",
                epoch,
                SubagentRunOutcome::Failed("cancelled".into()),
                13,
            )
            .unwrap()
            .continuation
            .unwrap();
        assert_eq!(continuation.subagent_request_id, "subagent:call-2");

        let token = Arc::new(AtomicBool::new(false));
        owner.attach_control(&launch.record.subagent_id, Arc::clone(&token)).unwrap();
        assert!(owner
            .abort(&launch.record.subagent_id, "acct", epoch, 14)
            .unwrap());
        let late = owner
            .settle(
                &launch.record.subagent_id,
                "acct",
                epoch,
                SubagentRunOutcome::Completed("late".into()),
                15,
            )
            .unwrap();
        assert!(late.completion.is_none());
        assert!(late.pending_wake_disarmed.is_some());
        assert_eq!(
            owner.get(&launch.record.subagent_id).unwrap().status,
            SubagentStatus::Aborted
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn process_reopen_account_fence_and_stale_epoch_require_reconciliation() {
        let path = root("reopen");
        let id;
        let old_epoch;
        {
            let mut owner = DurableSubagentOwner::open(&path, 10).unwrap();
            old_epoch = owner.process_epoch();
            id = owner
                .launch(
                    "parent", lineage("parent-run"), "box", "computer-use", "call-3", "inspect",
                    "acct-a", None, 11,
                )
                .unwrap()
                .record
                .subagent_id;
        }
        let mut reopened = DurableSubagentOwner::open(&path, 20).unwrap();
        assert!(reopened.process_epoch() > old_epoch);
        assert_eq!(reopened.get(&id).unwrap().status, SubagentStatus::OutcomeUnknown);
        assert!(reopened
            .settle(
                &id,
                "acct-a",
                old_epoch,
                SubagentRunOutcome::Completed("stale".into()),
                21,
            )
            .unwrap()
            .ignored_stale_callback);
        assert!(reopened
            .reconcile_outcome_unknown(
                &id,
                "acct-b",
                SubagentRunOutcome::Completed("wrong".into()),
                22,
            )
            .is_err());
        let reconciled = reopened
            .reconcile_outcome_unknown(
                &id,
                "acct-a",
                SubagentRunOutcome::Completed("confirmed".into()),
                23,
            )
            .unwrap();
        assert_eq!(reconciled.status, SubagentStatus::Completed);
        assert_eq!(reconciled.completion_result.as_deref(), Some("confirmed"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn parent_cancel_targets_only_matching_children_and_computer_audit_projects() {
        let path = root("parent");
        let mut owner = DurableSubagentOwner::open(&path, 10).unwrap();
        let a = owner
            .launch(
                "parent-a", lineage("req-a"), "box-a", "computeruse", "call-a", "inspect",
                "acct", None, 11,
            )
            .unwrap()
            .record
            .subagent_id;
        let b = owner
            .launch(
                "parent-b", lineage("req-b"), "box-b", "general-purpose", "call-b", "work",
                "acct", None, 12,
            )
            .unwrap()
            .record
            .subagent_id;
        let epoch = owner.process_epoch();
        let mut actions = BTreeMap::new();
        actions.insert("screenshot".into(), 2);
        actions.insert("click".into(), 3);
        owner
            .update_snapshot(
                &a,
                "acct",
                epoch,
                SubagentSessionSnapshot {
                    observed_tool_call_count: 5,
                    computer_use_usage: Some(ComputerUseUsageSnapshot {
                        model_id: Some("model-x".into()),
                        turn_ended_count: 2,
                        input_tokens: 10,
                        output_tokens: 4,
                        ..ComputerUseUsageSnapshot::default()
                    }),
                    computer_use_action_counts: actions,
                    ..SubagentSessionSnapshot::default()
                },
                13,
            )
            .unwrap();
        let settled = owner
            .settle(
                &a,
                "acct",
                epoch,
                SubagentRunOutcome::Completed("done".into()),
                20,
            )
            .unwrap();
        assert_eq!(settled.computer_use_usage.unwrap().tool_call_count, 5);
        assert_eq!(settled.computer_use_audit.unwrap().action_count, 5);
        assert!(owner.abort_for_parent_request("req-a", "parent-cancel", 21).unwrap().is_empty());
        assert_eq!(owner.get(&b).unwrap().status, SubagentStatus::Running);
        assert_eq!(
            owner.abort_for_parent_request("req-b", "parent-cancel", 22).unwrap(),
            vec![b.clone()]
        );
        assert_eq!(owner.get(&b).unwrap().status, SubagentStatus::Aborted);
        let _ = fs::remove_file(path);
    }
}
