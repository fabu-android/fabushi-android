use std::{fs, path::{Path, PathBuf}, sync::{atomic::AtomicBool, Arc}};

use serde_json::{json, Value};

use crate::{
    extensions::memory::{
        memory_service::{
            get_project_dir, get_project_memory_shard_dir, get_user_memory_shard_dir,
            project_dir_exists, FileMemoryStore, MemoryKind, MemoryRecord, MEMORY_NOTE_PREFIX,
        },
        memory_synthesis_service::{
            parse_json_object, synthesis_request_json, synthesis_system_prompt,
            verification_request_json, verification_system_prompt, MemorySynthesisOptions,
            MemorySynthesisService, SynthesisCancelSignal, SynthesisProposalRequest,
            SynthesisReport, SynthesisTarget, SynthesisVerificationRequest,
        },
        project_membership::{is_safe_project_slug, AgentProjectMembership},
    },
    runner::{AndroidHostInferenceProvider, AndroidInferenceMode},
    sha256::sha256_hex,
};

const DEFAULT_LIST_LIMIT: usize = 200;
const MAX_LIST_LIMIT: usize = 500;
const DEFAULT_RECENT_LIMIT: usize = 32;
const MAX_RECENT_LIMIT: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndroidMemoryScope<'a> {
    Agent,
    User,
    Project(&'a str),
}


#[derive(Debug, Clone)]
pub struct AndroidMemoryOwner {
    root: PathBuf,
}

impl AndroidMemoryOwner {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn list(
        &self,
        account_fence: &str,
        agent_id: &str,
        limit: Option<u64>,
    ) -> Result<Value, String> {
        self.list_scoped(account_fence, agent_id, AndroidMemoryScope::Agent, limit)
    }

    pub fn list_scoped(
        &self,
        account_fence: &str,
        agent_id: &str,
        scope: AndroidMemoryScope<'_>,
        limit: Option<u64>,
    ) -> Result<Value, String> {
        let store = self.scoped_store(account_fence, agent_id, scope)?;
        let limit = bounded_limit(limit, DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT)?;
        Ok(Value::Array(
            store
                .list_memories(limit)
                .into_iter()
                .map(memory_record_json)
                .collect(),
        ))
    }

    pub fn recall(
        &self,
        account_fence: &str,
        agent_id: &str,
        recent_limit: Option<u64>,
    ) -> Result<Value, String> {
        self.recall_scoped(account_fence, agent_id, AndroidMemoryScope::Agent, recent_limit)
    }

    pub fn recall_scoped(
        &self,
        account_fence: &str,
        agent_id: &str,
        scope: AndroidMemoryScope<'_>,
        recent_limit: Option<u64>,
    ) -> Result<Value, String> {
        let store = self.scoped_store(account_fence, agent_id, scope)?;
        let recent_limit = bounded_limit(recent_limit, DEFAULT_RECENT_LIMIT, MAX_RECENT_LIMIT)?;
        let recall = store.recall(recent_limit);
        Ok(json!({
            "profile": recall.profile.into_iter().map(memory_record_json).collect::<Vec<_>>(),
            "recent": recall.recent.into_iter().map(memory_record_json).collect::<Vec<_>>(),
        }))
    }

    pub fn add(
        &self,
        account_fence: &str,
        agent_id: &str,
        content: &str,
        kind: &str,
        created_at_ms: i64,
    ) -> Result<Value, String> {
        self.add_scoped(
            account_fence,
            agent_id,
            AndroidMemoryScope::Agent,
            content,
            kind,
            created_at_ms,
        )
    }

    pub fn add_scoped(
        &self,
        account_fence: &str,
        agent_id: &str,
        scope: AndroidMemoryScope<'_>,
        content: &str,
        tier: &str,
        created_at_ms: i64,
    ) -> Result<Value, String> {
        let content = content.trim();
        if content.is_empty() {
            return Err("memory content is required".into());
        }
        let (payload, kind) = match tier {
            "profile" => (content.to_string(), MemoryKind::Profile),
            "log" => (content.to_string(), MemoryKind::Log),
            "note" => (format!("{MEMORY_NOTE_PREFIX}{content}"), MemoryKind::Log),
            _ => return Err("memory tier must be profile, note, or log".into()),
        };
        let store = self.scoped_store(account_fence, agent_id, scope)?;
        match store
            .add_memory(&payload, created_at_ms, kind)
            .map_err(|error| format!("failed to persist memory: {error}"))?
        {
            Some(record) => Ok(json!({
                "created": true,
                "memory": memory_record_json(record),
            })),
            None => Ok(json!({"created": false})),
        }
    }

    pub fn remove(
        &self,
        account_fence: &str,
        agent_id: &str,
        memory_id: &str,
    ) -> Result<Value, String> {
        self.remove_scoped(account_fence, agent_id, AndroidMemoryScope::Agent, memory_id)
    }

    pub fn remove_scoped(
        &self,
        account_fence: &str,
        agent_id: &str,
        scope: AndroidMemoryScope<'_>,
        memory_id: &str,
    ) -> Result<Value, String> {
        let memory_id = memory_id.trim();
        if memory_id.is_empty() || memory_id.len() > 128 {
            return Err("memory id is invalid".into());
        }
        let store = self.scoped_store(account_fence, agent_id, scope)?;
        let removed = store
            .remove_memory(memory_id)
            .map_err(|error| format!("failed to remove memory: {error}"))?;
        Ok(json!({"removed": removed}))
    }

    pub fn clear(&self, account_fence: &str, agent_id: &str) -> Result<Value, String> {
        self.clear_scoped(account_fence, agent_id, AndroidMemoryScope::Agent)
    }

    pub fn clear_scoped(
        &self,
        account_fence: &str,
        agent_id: &str,
        scope: AndroidMemoryScope<'_>,
    ) -> Result<Value, String> {
        let store = self.scoped_store(account_fence, agent_id, scope)?;
        store
            .clear_memories()
            .map_err(|error| format!("failed to clear memory: {error}"))?;
        Ok(json!({"cleared": true}))
    }

    fn store(&self, account_fence: &str, agent_id: &str) -> Result<FileMemoryStore, String> {
        self.scoped_store(account_fence, agent_id, AndroidMemoryScope::Agent)
    }

    pub fn synthesize_settled_turn(
        &self,
        account_fence: &str,
        agent_id: &str,
        evidence_id: Option<String>,
        user: &str,
        assistant: &str,
        occurred_at_ms: i64,
        mode: AndroidInferenceMode,
        bearer_token: Option<String>,
        cancelled: Arc<AtomicBool>,
        report: Option<Arc<dyn Fn(SynthesisReport) + Send + Sync>>,
    ) -> Result<(), String> {
        let target = Arc::new(self.store(account_fence, agent_id)?);
        let list_target = Arc::clone(&target);
        let list_agent = agent_id.to_string();
        let list_targets = Arc::new(move || {
            vec![(
                list_agent.clone(),
                Arc::clone(&list_target) as Arc<dyn SynthesisTarget>,
            )]
        });
        let get_target = Arc::clone(&target);
        let get_agent = agent_id.to_string();
        let target_lookup = Arc::new(move |candidate: &str| {
            (candidate == get_agent)
                .then(|| Arc::clone(&get_target) as Arc<dyn SynthesisTarget>)
        });

        let propose_token = bearer_token.clone();
        let propose_cancelled = Arc::clone(&cancelled);
        let propose = Arc::new(
            move |request: SynthesisProposalRequest, signal: SynthesisCancelSignal| {
                let output = AndroidHostInferenceProvider::run_summarization_prompt(
                    mode,
                    propose_token.clone(),
                    Arc::clone(&propose_cancelled),
                    synthesis_system_prompt(),
                    &synthesis_request_json(&request),
                    &|| signal.is_cancelled(),
                )
                .map_err(|error| error.message)?;
                parse_json_object(&output)
                    .ok_or_else(|| "memory synthesis inference returned invalid JSON".to_string())
            },
        );

        let verify_token = bearer_token;
        let verify_cancelled = cancelled;
        let verify = Arc::new(
            move |request: SynthesisVerificationRequest, signal: SynthesisCancelSignal| {
                let output = AndroidHostInferenceProvider::run_summarization_prompt(
                    mode,
                    verify_token.clone(),
                    Arc::clone(&verify_cancelled),
                    verification_system_prompt(),
                    &verification_request_json(&request),
                    &|| signal.is_cancelled(),
                )
                .map_err(|error| error.message)?;
                let verdict = parse_json_object(&output)
                    .ok_or_else(|| "memory synthesis verification returned invalid JSON".to_string())?;
                Ok(verdict.get("approved").and_then(Value::as_bool) == Some(true))
            },
        );

        let mut options = MemorySynthesisOptions::new(list_targets, target_lookup, propose);
        options.verify = Some(verify);
        options.report = report;
        let service = MemorySynthesisService::new(options);
        service.start();
        service.record_turn(agent_id, evidence_id, user, assistant, occurred_at_ms);
        let _ = service.run_now();
        Ok(())
    }

    fn scoped_store(
        &self,
        account_fence: &str,
        agent_id: &str,
        scope: AndroidMemoryScope<'_>,
    ) -> Result<FileMemoryStore, String> {
        let (account_root, agent_key) = self.account_agent_root(account_fence, agent_id)?;
        let path = match scope {
            AndroidMemoryScope::Agent => account_root.join(format!("agent-{agent_key}")),
            AndroidMemoryScope::User => get_user_memory_shard_dir(&account_root, &agent_key),
            AndroidMemoryScope::Project(slug) => {
                let slug = slug.trim();
                if !is_safe_project_slug(slug) {
                    return Err("memory project slug is invalid".into());
                }
                if !project_dir_exists(&account_root, slug) {
                    return Err(format!("memory project {slug:?} does not exist"));
                }
                let membership = self.membership(&account_root, &agent_key);
                if !membership.read().contains(slug) {
                    return Err(format!("Agent has not joined memory project {slug:?}"));
                }
                get_project_memory_shard_dir(&account_root, slug, &agent_key)
            }
        };
        Ok(FileMemoryStore::with_synthesis_metadata(path, true))
    }

    pub fn create_project(
        &self,
        account_fence: &str,
        agent_id: &str,
        slug: &str,
        name: &str,
        description: Option<&str>,
    ) -> Result<Value, String> {
        let (account_root, agent_key) = self.account_agent_root(account_fence, agent_id)?;
        let slug = slug.trim();
        let name = name.trim();
        if !is_safe_project_slug(slug) {
            return Err("memory project slug is invalid".into());
        }
        if name.is_empty() {
            return Err("memory project name is required".into());
        }
        let project_dir = get_project_dir(&account_root, slug);
        let existed = project_dir_exists(&account_root, slug);
        if !existed {
            fs::create_dir_all(&project_dir)
                .map_err(|error| format!("failed to create memory project: {error}"))?;
            let body = format!(
                "---\nname: {}\ndescription: {}\n---\n",
                name,
                description.unwrap_or_default().trim()
            );
            write_atomic(&project_dir.join("project.md"), body.as_bytes())?;
        }
        let joined = self.membership(&account_root, &agent_key)
            .join(slug)
            .map_err(|error| format!("failed to persist memory project membership: {error}"))?;
        Ok(json!({"project":slug,"created":!existed,"joined":joined || existed}))
    }

    pub fn join_project(
        &self,
        account_fence: &str,
        agent_id: &str,
        slug: &str,
    ) -> Result<Value, String> {
        let (account_root, agent_key) = self.account_agent_root(account_fence, agent_id)?;
        let slug = slug.trim();
        if !is_safe_project_slug(slug) {
            return Err("memory project slug is invalid".into());
        }
        if !project_dir_exists(&account_root, slug) {
            return Err(format!("memory project {slug:?} does not exist"));
        }
        let joined = self.membership(&account_root, &agent_key)
            .join(slug)
            .map_err(|error| format!("failed to persist memory project membership: {error}"))?;
        Ok(json!({"project":slug,"joined":joined}))
    }

    pub fn leave_project(
        &self,
        account_fence: &str,
        agent_id: &str,
        slug: &str,
    ) -> Result<Value, String> {
        let (account_root, agent_key) = self.account_agent_root(account_fence, agent_id)?;
        let slug = slug.trim();
        if !is_safe_project_slug(slug) {
            return Err("memory project slug is invalid".into());
        }
        let left = self.membership(&account_root, &agent_key)
            .leave(slug)
            .map_err(|error| format!("failed to persist memory project membership: {error}"))?;
        Ok(json!({"project":slug,"left":left}))
    }

    fn account_agent_root(
        &self,
        account_fence: &str,
        agent_id: &str,
    ) -> Result<(PathBuf, String), String> {
        let account_fence = account_fence.trim();
        let agent_id = agent_id.trim();
        if account_fence.is_empty() {
            return Err("memory requires an authenticated account fence".into());
        }
        if agent_id.is_empty() || agent_id.len() > 200 || agent_id.chars().any(char::is_control) {
            return Err("memory agentId is invalid".into());
        }
        Ok((
            self.root.join(format!("account-{}", sha256_hex(account_fence.as_bytes()))),
            sha256_hex(agent_id.as_bytes()),
        ))
    }

    fn membership(&self, account_root: &Path, agent_key: &str) -> AgentProjectMembership {
        AgentProjectMembership::new(
            account_root.join("agent-membership").join(format!("agent-{agent_key}")),
        )
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("memory project path has no parent")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to prepare memory project: {error}"))?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        use std::io::Write;
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| format!("failed to persist memory project: {error}"))
}

fn bounded_limit(value: Option<u64>, default: usize, max: usize) -> Result<usize, String> {
    let value = value.unwrap_or(default as u64);
    if value == 0 || value > max as u64 {
        return Err(format!("memory limit must be between 1 and {max}"));
    }
    usize::try_from(value).map_err(|_| "memory limit is invalid".into())
}

fn memory_record_json(record: MemoryRecord) -> Value {
    json!({
        "id": record.id,
        "content": record.content,
        "createdAtMs": record.created_at,
        "kind": match record.kind {
            MemoryKind::Profile => "profile",
            MemoryKind::Log => "log",
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::memory::memory_service::MemoryOrigin;

    #[test]
    fn explicit_memory_is_account_fenced_and_durable() {
        let root = tempfile::tempdir().unwrap();
        let owner = AndroidMemoryOwner::new(root.path());

        let created = owner
            .add(
                "session:account-a",
                "agent-a",
                "Likes green tea",
                "profile",
                1_700_000_000_000,
            )
            .unwrap();
        assert_eq!(created["created"], true);
        assert_eq!(
            owner.list("session:account-a", "agent-a", None).unwrap()[0]["content"],
            "Likes green tea"
        );
        assert!(owner
            .list("session:account-b", "agent-a", None)
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());

        let reopened = AndroidMemoryOwner::new(root.path());
        assert_eq!(
            reopened
                .recall("session:account-a", "agent-a", None)
                .unwrap()["profile"][0]["content"],
            "Likes green tea"
        );
    }

    #[test]
    fn explicit_memory_is_marked_explicit_for_future_synthesis() {
        let root = tempfile::tempdir().unwrap();
        let owner = AndroidMemoryOwner::new(root.path());
        owner
            .add(
                "session:account-a",
                "agent-a",
                "Never auto-send transcripts",
                "profile",
                1_700_000_000_000,
            )
            .unwrap();

        let store = owner.store("session:account-a", "agent-a").unwrap();
        let snapshot = store.prepare_synthesis();
        assert_eq!(snapshot.memories.len(), 1);
        assert_eq!(snapshot.memories[0].origin, MemoryOrigin::Explicit);
    }

    #[test]
    fn user_scope_is_account_fenced_and_separate_from_agent_scope() {
        let root = tempfile::tempdir().unwrap();
        let owner = AndroidMemoryOwner::new(root.path());
        owner
            .add_scoped(
                "session:account-a",
                "agent-a",
                AndroidMemoryScope::User,
                "User-scoped fact",
                "profile",
                1_700_000_000_000,
            )
            .unwrap();
        assert!(owner
            .list("session:account-a", "agent-a", None)
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(
            owner
                .list_scoped(
                    "session:account-a",
                    "agent-a",
                    AndroidMemoryScope::User,
                    None,
                )
                .unwrap()[0]["content"],
            "User-scoped fact"
        );
        assert!(owner
            .list_scoped(
                "session:account-b",
                "agent-a",
                AndroidMemoryScope::User,
                None,
            )
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn project_scope_requires_membership_and_recovers_after_rejoin() {
        let root = tempfile::tempdir().unwrap();
        let owner = AndroidMemoryOwner::new(root.path());
        owner
            .create_project(
                "session:account-a",
                "agent-a",
                "alpha",
                "Alpha",
                Some("Project memory"),
            )
            .unwrap();
        owner
            .add_scoped(
                "session:account-a",
                "agent-a",
                AndroidMemoryScope::Project("alpha"),
                "Project fact",
                "note",
                1_700_000_000_000,
            )
            .unwrap();
        assert_eq!(
            owner
                .list_scoped(
                    "session:account-a",
                    "agent-a",
                    AndroidMemoryScope::Project("alpha"),
                    None,
                )
                .unwrap()[0]["content"],
            "Note: Project fact"
        );
        owner
            .leave_project("session:account-a", "agent-a", "alpha")
            .unwrap();
        assert!(owner
            .list_scoped(
                "session:account-a",
                "agent-a",
                AndroidMemoryScope::Project("alpha"),
                None,
            )
            .unwrap_err()
            .contains("has not joined"));
        owner
            .join_project("session:account-a", "agent-a", "alpha")
            .unwrap();
        assert_eq!(
            owner
                .list_scoped(
                    "session:account-a",
                    "agent-a",
                    AndroidMemoryScope::Project("alpha"),
                    None,
                )
                .unwrap()[0]["content"],
            "Note: Project fact"
        );
        assert!(owner
            .join_project("session:account-a", "agent-b", "../escape")
            .is_err());
        assert!(owner
            .join_project("session:account-b", "agent-a", "alpha")
            .is_err());
    }

    #[test]
    fn failed_synthesis_never_mutates_explicit_memory() {
        let root = tempfile::tempdir().unwrap();
        let owner = AndroidMemoryOwner::new(root.path());
        owner
            .add(
                "session:account-a",
                "agent-a",
                "Keep this explicit fact",
                "profile",
                1_700_000_000_000,
            )
            .unwrap();

        owner
            .synthesize_settled_turn(
                "session:account-a",
                "agent-a",
                Some("turn:r1".into()),
                "Please remember a possible fact",
                "Acknowledged",
                1_700_000_001_000,
                AndroidInferenceMode::Test,
                None,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();

        let memories = owner.list("session:account-a", "agent-a", None).unwrap();
        assert_eq!(memories.as_array().unwrap().len(), 1);
        assert_eq!(memories[0]["content"], "Keep this explicit fact");
    }

    #[test]
    fn duplicate_remove_clear_and_limits_are_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let owner = AndroidMemoryOwner::new(root.path());
        let first = owner
            .add(
                "session:account-a",
                "agent-a",
                "Remember this",
                "log",
                1_700_000_000_000,
            )
            .unwrap();
        assert_eq!(first["created"], true);
        assert_eq!(
            owner
                .add(
                    "session:account-a",
                    "agent-a",
                    " remember   this ",
                    "log",
                    1_700_000_001_000,
                )
                .unwrap()["created"],
            false
        );
        assert!(owner.list("session:account-a", "agent-a", Some(0)).is_err());
        assert!(owner
            .list("session:account-a", "agent-a", Some((MAX_LIST_LIMIT + 1) as u64))
            .is_err());

        let id = owner.list("session:account-a", "agent-a", None).unwrap()[0]["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(
            owner
                .remove("session:account-a", "agent-a", &id)
                .unwrap()["removed"],
            true
        );
        owner
            .add(
                "session:account-a",
                "agent-a",
                "Another fact",
                "log",
                1_700_000_002_000,
            )
            .unwrap();
        owner.clear("session:account-a", "agent-a").unwrap();
        assert!(owner
            .list("session:account-a", "agent-a", None)
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
    }
}
