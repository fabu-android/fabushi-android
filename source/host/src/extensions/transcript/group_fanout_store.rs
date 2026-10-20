use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

const GROUP_FANOUT_FILE_NAME: &str = "group-fanout-v1.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupFanoutFailure {
    pub member_id: String,
    pub message: String,
    pub observed_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupFanoutChild {
    pub member_id: String,
    pub request_id: String,
    pub operation_id: String,
    pub round: u32,
    pub baseline_group_entry_count: usize,
    #[serde(default)]
    pub outcome_unknown: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupFanoutRun {
    pub parent_request_id: String,
    pub parent_operation_id: String,
    pub account_fence: String,
    pub group_id: String,
    pub model_id: String,
    pub user_text: String,
    pub member_ids: Vec<String>,
    pub round: u32,
    pub cursor: usize,
    pub total_member_turns: u32,
    pub posted_this_round: u32,
    pub total_posted: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_ack_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_child: Option<GroupFanoutChild>,
    #[serde(default)]
    pub failures: Vec<GroupFanoutFailure>,
    #[serde(default)]
    pub completed: bool,
    pub updated_at_ms: u64,
}

impl GroupFanoutRun {
    pub fn new(
        parent_request_id: impl Into<String>,
        parent_operation_id: impl Into<String>,
        account_fence: impl Into<String>,
        group_id: impl Into<String>,
        model_id: impl Into<String>,
        user_text: impl Into<String>,
        member_ids: Vec<String>,
        parent_ack_token: Option<String>,
        now_ms: u64,
    ) -> Result<Self, String> {
        let parent_request_id = parent_request_id.into();
        let parent_operation_id = parent_operation_id.into();
        let account_fence = account_fence.into();
        let group_id = group_id.into();
        if parent_request_id.trim().is_empty()
            || parent_operation_id.trim().is_empty()
            || account_fence.trim().is_empty()
            || group_id.trim().is_empty()
        {
            return Err("group fanout identity fields must be non-empty".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        let member_ids = member_ids
            .into_iter()
            .filter(|member| {
                !member.trim().is_empty()
                    && member != &group_id
                    && seen.insert(member.clone())
            })
            .collect::<Vec<_>>();
        if member_ids.is_empty() {
            return Err("group fanout requires at least one distinct member".into());
        }
        Ok(Self {
            parent_request_id,
            parent_operation_id,
            account_fence,
            group_id,
            model_id: model_id.into(),
            user_text: user_text.into(),
            member_ids,
            round: 0,
            cursor: 0,
            total_member_turns: 0,
            posted_this_round: 0,
            total_posted: 0,
            parent_ack_token,
            active_child: None,
            failures: Vec::new(),
            completed: false,
            updated_at_ms: now_ms,
        })
    }

    pub fn is_current_account(&self, account_fence: &str) -> bool {
        !self.completed && self.account_fence == account_fence
    }

    pub fn current_member(&self) -> Option<&str> {
        if self.completed || self.active_child.is_some() {
            return None;
        }
        self.member_ids.get(self.cursor).map(String::as_str)
    }

    pub fn start_child(
        &mut self,
        request_id: String,
        operation_id: String,
        baseline_group_entry_count: usize,
        now_ms: u64,
    ) -> Result<GroupFanoutChild, String> {
        let member_id = self
            .current_member()
            .ok_or_else(|| "group fanout has no dispatchable member".to_string())?
            .to_string();
        let child = GroupFanoutChild {
            member_id,
            request_id,
            operation_id,
            round: self.round,
            baseline_group_entry_count,
            outcome_unknown: false,
        };
        self.active_child = Some(child.clone());
        self.total_member_turns = self.total_member_turns.saturating_add(1);
        self.updated_at_ms = now_ms;
        Ok(child)
    }

    pub fn settle_child(
        &mut self,
        posted_messages: u32,
        failure: Option<String>,
        now_ms: u64,
    ) -> Result<(), String> {
        let child = self
            .active_child
            .take()
            .ok_or_else(|| "group fanout child is not active".to_string())?;
        if let Some(message) = failure {
            self.failures.push(GroupFanoutFailure {
                member_id: child.member_id,
                message,
                observed_at_ms: now_ms,
            });
        }
        self.posted_this_round = self.posted_this_round.saturating_add(posted_messages);
        self.total_posted = self.total_posted.saturating_add(posted_messages);
        self.cursor = self.cursor.saturating_add(1);
        self.updated_at_ms = now_ms;
        Ok(())
    }

    pub fn mark_child_outcome_unknown(&mut self, now_ms: u64) -> Result<(), String> {
        let child = self
            .active_child
            .as_mut()
            .ok_or_else(|| "group fanout child is not active".to_string())?;
        child.outcome_unknown = true;
        self.updated_at_ms = now_ms;
        Ok(())
    }

    pub fn advance_round_or_complete(
        &mut self,
        max_rounds: u32,
        max_member_turns: u32,
        now_ms: u64,
    ) -> bool {
        if self.active_child.is_some() || self.cursor < self.member_ids.len() {
            return false;
        }
        if self.posted_this_round == 0
            || self.round.saturating_add(1) >= max_rounds
            || self.total_member_turns >= max_member_turns
        {
            self.completed = true;
            self.updated_at_ms = now_ms;
            return true;
        }
        self.round = self.round.saturating_add(1);
        self.cursor = 0;
        self.posted_this_round = 0;
        self.updated_at_ms = now_ms;
        false
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupFanoutFile {
    version: u8,
    #[serde(default)]
    runs: Vec<GroupFanoutRun>,
}

#[derive(Clone)]
pub struct GroupFanoutStore {
    file_path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl GroupFanoutStore {
    pub fn open(root: impl AsRef<Path>) -> Self {
        Self {
            file_path: root.as_ref().join(GROUP_FANOUT_FILE_NAME),
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn list(&self) -> Vec<GroupFanoutRun> {
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.read_unlocked()
    }

    pub fn get(&self, parent_request_id: &str) -> Option<GroupFanoutRun> {
        self.list()
            .into_iter()
            .find(|run| run.parent_request_id == parent_request_id)
    }

    pub fn upsert(&self, run: GroupFanoutRun) -> Result<(), String> {
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut runs = self.read_unlocked();
        runs.retain(|existing| existing.parent_request_id != run.parent_request_id);
        runs.push(run);
        self.write_unlocked(&runs)
            .map_err(|error| format!("failed to persist group fanout store: {error}"))
    }

    pub fn remove(&self, parent_request_id: &str) -> Result<(), String> {
        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut runs = self.read_unlocked();
        runs.retain(|run| run.parent_request_id != parent_request_id);
        if runs.is_empty() {
            match fs::remove_file(&self.file_path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(format!("failed to remove group fanout store: {error}")),
            }
        } else {
            self.write_unlocked(&runs)
                .map_err(|error| format!("failed to persist group fanout store: {error}"))
        }
    }

    fn read_unlocked(&self) -> Vec<GroupFanoutRun> {
        let Ok(raw) = fs::read_to_string(&self.file_path) else {
            return Vec::new();
        };
        let Ok(file) = serde_json::from_str::<GroupFanoutFile>(&raw) else {
            return Vec::new();
        };
        if file.version != 1 {
            return Vec::new();
        }
        file.runs
    }

    fn write_unlocked(&self, runs: &[GroupFanoutRun]) -> std::io::Result<()> {
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let part = PathBuf::from(format!("{}.part", self.file_path.display()));
        let encoded = serde_json::to_vec(&GroupFanoutFile {
            version: 1,
            runs: runs.to_vec(),
        })
        .map_err(std::io::Error::other)?;
        let mut file = fs::File::create(&part)?;
        file.write_all(&encoded)?;
        file.sync_all()?;
        replace_file(&part, &self.file_path)
    }
}

fn replace_file(part: &Path, target: &Path) -> std::io::Result<()> {
    match fs::rename(part, target) {
        Ok(()) => Ok(()),
        Err(first) if target.exists() => {
            fs::remove_file(target)?;
            fs::rename(part, target).map_err(|_| first)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(now: u64) -> GroupFanoutRun {
        GroupFanoutRun::new(
            "parent-request",
            "parent-operation",
            "account:1",
            "group-a",
            "gpt",
            "@member-a hello",
            vec!["member-a".into(), "member-b".into(), "member-a".into()],
            Some("ack-token".into()),
            now,
        )
        .unwrap()
    }

    #[test]
    fn fanout_identity_dedupes_members_and_rejects_nested_self() {
        let run = run(1);
        assert_eq!(run.member_ids, vec!["member-a", "member-b"]);
        assert_eq!(run.current_member(), Some("member-a"));
        assert!(GroupFanoutRun::new(
            "r",
            "o",
            "a",
            "group",
            "m",
            "text",
            vec!["group".into()],
            None,
            1,
        )
        .is_err());
    }

    #[test]
    fn child_failure_advances_without_aborting_other_members() {
        let mut run = run(1);
        run.start_child("child-r-1".into(), "child-o-1".into(), 4, 2)
            .unwrap();
        run.settle_child(0, Some("member failed".into()), 3)
            .unwrap();
        assert_eq!(run.current_member(), Some("member-b"));
        assert_eq!(run.failures.len(), 1);
        run.start_child("child-r-2".into(), "child-o-2".into(), 4, 4)
            .unwrap();
        run.settle_child(1, None, 5).unwrap();
        assert!(!run.advance_round_or_complete(3, 10, 6));
        assert_eq!(run.round, 1);
        assert_eq!(run.current_member(), Some("member-a"));
    }

    #[test]
    fn zero_visible_messages_finishes_round_without_inventing_reply() {
        let mut run = run(1);
        for index in 0..2 {
            run.start_child(
                format!("child-r-{index}"),
                format!("child-o-{index}"),
                0,
                2 + index,
            )
            .unwrap();
            run.settle_child(0, None, 3 + index).unwrap();
        }
        assert!(run.advance_round_or_complete(3, 10, 10));
        assert!(run.completed);
        assert_eq!(run.total_posted, 0);
    }

    #[test]
    fn outcome_unknown_survives_reopen_without_advancing_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let store = GroupFanoutStore::open(dir.path());
        let mut run = run(1);
        run.start_child("child-r".into(), "child-o".into(), 3, 2)
            .unwrap();
        run.mark_child_outcome_unknown(3).unwrap();
        store.upsert(run).unwrap();

        let reopened = GroupFanoutStore::open(dir.path());
        let restored = reopened.get("parent-request").unwrap();
        assert_eq!(restored.cursor, 0);
        assert!(restored.active_child.unwrap().outcome_unknown);
    }

    #[test]
    fn account_fence_is_part_of_durable_run_identity() {
        let run = run(1);
        assert!(run.is_current_account("account:1"));
        assert!(!run.is_current_account("account:2"));
    }

    #[test]
    fn remove_is_idempotent_and_does_not_leave_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = GroupFanoutStore::open(dir.path());
        store.upsert(run(1)).unwrap();
        store.remove("parent-request").unwrap();
        store.remove("parent-request").unwrap();
        assert!(store.list().is_empty());
        assert!(!PathBuf::from(format!("{}.part", store.file_path.display())).exists());
    }
}
