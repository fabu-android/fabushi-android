use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const STATE_VERSION: u32 = 1;
const MAX_INSTRUCTION_CHARS: usize = 16_000;
const MAX_SNAPSHOT_CHARS: usize = 12 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidPendingHandoff {
    pub request_id: String,
    pub account_fence: String,
    pub agent_id: String,
    pub instruction: String,
    pub snapshot_data_url: Option<String>,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AndroidHandoffStart {
    Started(AndroidPendingHandoff),
    AlreadyPending(AndroidPendingHandoff),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidHandoffEnded {
    pub request_id: String,
    pub account_fence: String,
    pub agent_id: String,
    pub resolution: String,
    pub trigger: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedState {
    version: u32,
    pending_by_agent: BTreeMap<String, AndroidPendingHandoff>,
    #[serde(default)]
    resume_by_request: BTreeMap<String, AndroidHandoffEnded>,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            pending_by_agent: BTreeMap::new(),
            resume_by_request: BTreeMap::new(),
        }
    }
}

/// Durable Android replacement for Desktop BoxHandoffService.
///
/// This owner persists only handoff lifecycle state. It never captures a screen or executes a
/// remote operation by itself. Authorized Remote Runner / viewport code may attach a snapshot only
/// when both account fence and request identity still match the live handoff.
#[derive(Debug)]
pub struct AndroidBoxHandoffOwner {
    path: PathBuf,
    state: PersistedState,
}

impl AndroidBoxHandoffOwner {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        let state = match fs::read(&path) {
            Ok(bytes) => {
                let decoded: PersistedState = serde_json::from_slice(&bytes)
                    .map_err(|error| format!("failed to decode durable box handoff state: {error}"))?;
                if decoded.version != STATE_VERSION {
                    return Err(format!(
                        "unsupported durable box handoff state version {}",
                        decoded.version
                    ));
                }
                decoded
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => PersistedState::default(),
            Err(error) => {
                return Err(format!("failed to read durable box handoff state: {error}"))
            }
        };
        Ok(Self { path, state })
    }

    pub fn pending_for(
        &self,
        account_fence: &str,
        agent_id: &str,
    ) -> Result<Option<AndroidPendingHandoff>, String> {
        let (account_fence, agent_id) = validate_scope(account_fence, agent_id)?;
        Ok(self
            .state
            .pending_by_agent
            .get(agent_id)
            .filter(|pending| pending.account_fence == account_fence)
            .cloned())
    }

    pub fn start(
        &mut self,
        account_fence: &str,
        agent_id: &str,
        instruction: &str,
        now_ms: u64,
    ) -> Result<AndroidHandoffStart, String> {
        let (account_fence, agent_id) = validate_scope(account_fence, agent_id)?;
        let instruction = instruction.trim();
        if instruction.is_empty() {
            return Err("box handoff instruction is required".into());
        }
        if instruction.chars().count() > MAX_INSTRUCTION_CHARS {
            return Err("box handoff instruction exceeds the supported limit".into());
        }

        if let Some(live) = self.state.pending_by_agent.get(agent_id) {
            if live.account_fence == account_fence {
                return Ok(AndroidHandoffStart::AlreadyPending(live.clone()));
            }
            return Err("box handoff target is fenced to a different account session".into());
        }

        let pending = AndroidPendingHandoff {
            request_id: Uuid::new_v4().to_string(),
            account_fence: account_fence.to_string(),
            agent_id: agent_id.to_string(),
            instruction: instruction.to_string(),
            snapshot_data_url: None,
            created_at_ms: now_ms,
        };
        self.state
            .pending_by_agent
            .insert(agent_id.to_string(), pending.clone());
        self.persist()?;
        Ok(AndroidHandoffStart::Started(pending))
    }

    pub fn attach_snapshot_if_current(
        &mut self,
        account_fence: &str,
        agent_id: &str,
        request_id: &str,
        snapshot_data_url: &str,
    ) -> Result<bool, String> {
        let (account_fence, agent_id) = validate_scope(account_fence, agent_id)?;
        let request_id = request_id.trim();
        if request_id.is_empty() {
            return Err("box handoff request id is required".into());
        }
        let snapshot_data_url = snapshot_data_url.trim();
        if snapshot_data_url.is_empty() {
            return Ok(false);
        }
        if !snapshot_data_url.starts_with("data:image/") {
            return Err("box handoff snapshot must be an image data URL".into());
        }
        if snapshot_data_url.len() > MAX_SNAPSHOT_CHARS {
            return Err("box handoff snapshot exceeds the supported limit".into());
        }

        let Some(live) = self.state.pending_by_agent.get_mut(agent_id) else {
            return Ok(false);
        };
        if live.account_fence != account_fence || live.request_id != request_id {
            return Ok(false);
        }
        live.snapshot_data_url = Some(snapshot_data_url.to_string());
        self.persist()?;
        Ok(true)
    }

    pub fn end(
        &mut self,
        account_fence: &str,
        agent_id: &str,
        request_id: &str,
        resolution: Option<&str>,
        trigger: Option<&str>,
    ) -> Result<Option<AndroidHandoffEnded>, String> {
        let (account_fence, agent_id) = validate_scope(account_fence, agent_id)?;
        let request_id = request_id.trim();
        if request_id.is_empty() {
            return Err("box handoff request id is required".into());
        }
        let Some(live) = self.state.pending_by_agent.get(agent_id) else {
            return Ok(self
                .state
                .resume_by_request
                .get(request_id)
                .filter(|ended| {
                    ended.account_fence == account_fence && ended.agent_id == agent_id
                })
                .cloned());
        };
        if live.account_fence != account_fence || live.request_id != request_id {
            return Ok(None);
        }
        let live = self.state.pending_by_agent.remove(agent_id).expect("checked pending handoff");
        let ended = AndroidHandoffEnded {
            request_id: live.request_id,
            account_fence: live.account_fence,
            agent_id: live.agent_id,
            resolution: normalized_end_value(resolution, "completed"),
            trigger: normalized_end_value(trigger, "unknown"),
        };
        self.state
            .resume_by_request
            .insert(ended.request_id.clone(), ended.clone());
        self.persist()?;
        Ok(Some(ended))
    }

    pub fn pending_resumes_for_account(
        &self,
        account_fence: &str,
    ) -> Result<Vec<AndroidHandoffEnded>, String> {
        let account_fence = account_fence.trim();
        if account_fence.is_empty() {
            return Err("box handoff account fence is required".into());
        }
        Ok(self
            .state
            .resume_by_request
            .values()
            .filter(|ended| ended.account_fence == account_fence)
            .cloned()
            .collect())
    }

    pub fn acknowledge_resume(
        &mut self,
        account_fence: &str,
        agent_id: &str,
        request_id: &str,
    ) -> Result<bool, String> {
        let (account_fence, agent_id) = validate_scope(account_fence, agent_id)?;
        let request_id = request_id.trim();
        if request_id.is_empty() {
            return Err("box handoff request id is required".into());
        }
        let Some(ended) = self.state.resume_by_request.get(request_id) else {
            return Ok(false);
        };
        if ended.account_fence != account_fence || ended.agent_id != agent_id {
            return Ok(false);
        }
        self.state.resume_by_request.remove(request_id);
        self.persist()?;
        Ok(true)
    }

    pub fn forget_account(&mut self, account_fence: &str) -> Result<usize, String> {
        let account_fence = account_fence.trim();
        if account_fence.is_empty() {
            return Err("box handoff account fence is required".into());
        }
        let pending_before = self.state.pending_by_agent.len();
        let resume_before = self.state.resume_by_request.len();
        self.state
            .pending_by_agent
            .retain(|_, pending| pending.account_fence != account_fence);
        self.state
            .resume_by_request
            .retain(|_, ended| ended.account_fence != account_fence);
        let removed = pending_before
            .saturating_sub(self.state.pending_by_agent.len())
            .saturating_add(resume_before.saturating_sub(self.state.resume_by_request.len()));
        if removed > 0 {
            self.persist()?;
        }
        Ok(removed)
    }

    pub fn pending_count(&self) -> usize {
        self.state.pending_by_agent.len()
    }

    pub fn projection(&self, account_fence: &str, agent_id: &str) -> Result<Value, String> {
        Ok(match self.pending_for(account_fence, agent_id)? {
            Some(pending) => json!({
                "requestId": pending.request_id,
                "instruction": pending.instruction,
                "snapshotDataUrl": pending.snapshot_data_url,
                "createdAtMs": pending.created_at_ms,
            }),
            None => Value::Null,
        })
    }

    fn persist(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec(&self.state)
            .map_err(|error| format!("failed to encode durable box handoff state: {error}"))?;
        write_atomic(&self.path, &bytes)
            .map_err(|error| format!("failed to persist durable box handoff state: {error}"))
    }
}

fn validate_scope<'a>(
    account_fence: &'a str,
    agent_id: &'a str,
) -> Result<(&'a str, &'a str), String> {
    let account_fence = account_fence.trim();
    let agent_id = agent_id.trim();
    if account_fence.is_empty() {
        return Err("box handoff account fence is required".into());
    }
    if agent_id.is_empty() || agent_id.len() > 200 || agent_id.chars().any(char::is_control) {
        return Err("box handoff agent id is invalid".into());
    }
    Ok((account_fence, agent_id))
}

fn normalized_end_value(value: Option<&str>, fallback: &str) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .chars()
        .take(128)
        .collect()
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "handoff path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_start_is_idempotent_and_survives_process_reopen() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("box-handoff.json");
        let request_id;
        {
            let mut owner = AndroidBoxHandoffOwner::open(&path).unwrap();
            let AndroidHandoffStart::Started(first) = owner
                .start("session:a", "agent-1", "Complete login", 1_700_000_000_000)
                .unwrap()
            else {
                panic!("first handoff must start");
            };
            request_id = first.request_id.clone();
            let AndroidHandoffStart::AlreadyPending(second) = owner
                .start("session:a", "agent-1", "Different text", 1_700_000_000_100)
                .unwrap()
            else {
                panic!("duplicate handoff must reuse pending request");
            };
            assert_eq!(second.request_id, request_id);
            assert_eq!(second.instruction, "Complete login");
        }
        let reopened = AndroidBoxHandoffOwner::open(&path).unwrap();
        assert_eq!(
            reopened.pending_for("session:a", "agent-1").unwrap().unwrap().request_id,
            request_id
        );
    }

    #[test]
    fn late_snapshot_cannot_resurrect_or_overwrite_new_request() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("box-handoff.json");
        let mut owner = AndroidBoxHandoffOwner::open(&path).unwrap();
        let AndroidHandoffStart::Started(first) = owner
            .start("session:a", "agent-1", "First", 1)
            .unwrap()
        else {
            panic!();
        };
        assert!(owner
            .end(
                "session:a",
                "agent-1",
                &first.request_id,
                Some("completed"),
                Some("user"),
            )
            .unwrap()
            .is_some());
        let AndroidHandoffStart::Started(second) = owner
            .start("session:a", "agent-1", "Second", 2)
            .unwrap()
        else {
            panic!();
        };
        assert!(!owner
            .attach_snapshot_if_current(
                "session:a",
                "agent-1",
                &first.request_id,
                "data:image/webp;base64,old",
            )
            .unwrap());
        assert!(owner
            .attach_snapshot_if_current(
                "session:a",
                "agent-1",
                &second.request_id,
                "data:image/webp;base64,new",
            )
            .unwrap());
        assert!(owner
            .projection("session:a", "agent-1")
            .unwrap()["snapshotDataUrl"]
            .as_str()
            .unwrap()
            .ends_with("new"));
    }

    #[test]
    fn account_switch_is_fail_closed_and_cleanup_is_scoped() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("box-handoff.json");
        let mut owner = AndroidBoxHandoffOwner::open(&path).unwrap();
        owner.start("session:a", "agent-1", "A", 1).unwrap();
        owner.start("session:b", "agent-2", "B", 2).unwrap();
        assert!(owner
            .start("session:b", "agent-1", "wrong account", 3)
            .unwrap_err()
            .contains("different account"));
        assert_eq!(owner.forget_account("session:a").unwrap(), 1);
        assert!(owner.pending_for("session:a", "agent-1").unwrap().is_none());
        assert!(owner.pending_for("session:b", "agent-2").unwrap().is_some());
    }

    #[test]
    fn stale_end_is_ignored_and_valid_end_is_durable() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("box-handoff.json");
        let mut owner = AndroidBoxHandoffOwner::open(&path).unwrap();
        let AndroidHandoffStart::Started(started) = owner
            .start("session:a", "agent-1", "Take over", 1)
            .unwrap()
        else {
            panic!();
        };
        assert!(owner
            .end("session:a", "agent-1", "stale", None, None)
            .unwrap()
            .is_none());
        let ended = owner
            .end(
                "session:a",
                "agent-1",
                &started.request_id,
                Some("cancelled"),
                Some("cancel"),
            )
            .unwrap()
            .unwrap();
        assert_eq!(ended.resolution, "cancelled");
        assert_eq!(
            owner
                .end(
                    "session:a",
                    "agent-1",
                    &started.request_id,
                    Some("ignored"),
                    Some("ignored"),
                )
                .unwrap(),
            Some(ended.clone()),
            "a retried end must replay the durable resume obligation until it is acknowledged",
        );
        drop(owner);
        let mut reopened = AndroidBoxHandoffOwner::open(&path).unwrap();
        assert_eq!(reopened.pending_count(), 0);
        assert_eq!(
            reopened.pending_resumes_for_account("session:a").unwrap(),
            vec![ended.clone()],
        );
        assert!(reopened
            .acknowledge_resume("session:a", "agent-1", &started.request_id)
            .unwrap());
        drop(reopened);
        assert!(AndroidBoxHandoffOwner::open(&path)
            .unwrap()
            .pending_resumes_for_account("session:a")
            .unwrap()
            .is_empty());
    }
    #[test]
    fn account_cleanup_removes_pending_and_terminal_resume_obligations_only_for_that_account() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("box-handoff.json");
        let mut owner = AndroidBoxHandoffOwner::open(&path).unwrap();
        let AndroidHandoffStart::Started(a) = owner.start("session:a", "agent-a", "A", 1).unwrap() else { panic!() };
        let AndroidHandoffStart::Started(b) = owner.start("session:b", "agent-b", "B", 2).unwrap() else { panic!() };
        owner.end("session:a", "agent-a", &a.request_id, Some("done"), Some("user")).unwrap();
        assert_eq!(owner.forget_account("session:a").unwrap(), 1);
        assert!(owner.pending_resumes_for_account("session:a").unwrap().is_empty());
        assert!(owner.pending_for("session:a", "agent-a").unwrap().is_none());
        assert!(owner.pending_for("session:b", "agent-b").unwrap().is_some());
        assert_eq!(owner.pending_for("session:b", "agent-b").unwrap().unwrap().request_id, b.request_id);
    }

}
