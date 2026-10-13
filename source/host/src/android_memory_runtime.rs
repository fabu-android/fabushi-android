use std::path::PathBuf;

use serde_json::{json, Value};

use crate::{
    extensions::memory::memory_service::{FileMemoryStore, MemoryKind, MemoryRecord},
    sha256::sha256_hex,
};

const DEFAULT_LIST_LIMIT: usize = 200;
const MAX_LIST_LIMIT: usize = 500;
const DEFAULT_RECENT_LIMIT: usize = 32;
const MAX_RECENT_LIMIT: usize = 128;

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
        let store = self.store(account_fence, agent_id)?;
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
        let store = self.store(account_fence, agent_id)?;
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
        let content = content.trim();
        if content.is_empty() {
            return Err("memory content is required".into());
        }
        let kind = match kind {
            "profile" => MemoryKind::Profile,
            "log" => MemoryKind::Log,
            _ => return Err("memory kind must be profile or log".into()),
        };
        let store = self.store(account_fence, agent_id)?;
        match store
            .add_memory(content, created_at_ms, kind)
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
        let memory_id = memory_id.trim();
        if memory_id.is_empty() || memory_id.len() > 128 {
            return Err("memory id is invalid".into());
        }
        let store = self.store(account_fence, agent_id)?;
        let removed = store
            .remove_memory(memory_id)
            .map_err(|error| format!("failed to remove memory: {error}"))?;
        Ok(json!({"removed": removed}))
    }

    pub fn clear(&self, account_fence: &str, agent_id: &str) -> Result<Value, String> {
        let store = self.store(account_fence, agent_id)?;
        store
            .clear_memories()
            .map_err(|error| format!("failed to clear memory: {error}"))?;
        Ok(json!({"cleared": true}))
    }

    fn store(&self, account_fence: &str, agent_id: &str) -> Result<FileMemoryStore, String> {
        let account_fence = account_fence.trim();
        let agent_id = agent_id.trim();
        if account_fence.is_empty() {
            return Err("memory requires an authenticated account fence".into());
        }
        if agent_id.is_empty() || agent_id.len() > 200 || agent_id.chars().any(char::is_control) {
            return Err("memory agentId is invalid".into());
        }
        let account_key = sha256_hex(account_fence.as_bytes());
        let agent_key = sha256_hex(agent_id.as_bytes());
        Ok(FileMemoryStore::with_synthesis_metadata(
            self.root
                .join(format!("account-{account_key}"))
                .join(format!("agent-{agent_key}")),
            true,
        ))
    }
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
