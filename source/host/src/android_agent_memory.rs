use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::extensions::memory::memory_service::{
    FileMemoryStore, MemoryKind, MemoryRecord, MEMORY_NOTE_PREFIX, get_agent_memory_dir,
};
use crate::extensions::memory::project_membership::is_safe_project_slug;

/// Canonical Android Host owner for durable per-Agent memory.
pub struct AndroidAgentMemoryOwner {
    agents_root: PathBuf,
}

impl AndroidAgentMemoryOwner {
    pub fn open(app_data_dir: impl AsRef<Path>) -> Self {
        Self { agents_root: app_data_dir.as_ref().join("agent-memory").join("agents") }
    }

    pub fn list(&self, agent_id: &str, limit: usize) -> Result<Value, String> {
        let store = self.store(agent_id)?;
        Ok(Value::Array(store.list_memories(limit.clamp(1, 500)).into_iter().map(memory_json).collect()))
    }

    pub fn write(&self, agent_id: &str, content: &str, tier: &str, created_at_ms: i64) -> Result<Value, String> {
        let store = self.store(agent_id)?;
        let (payload, kind) = match tier.trim() {
            "profile" => (content.trim().to_string(), MemoryKind::Profile),
            "note" => (format!("{MEMORY_NOTE_PREFIX}{}", content.trim()), MemoryKind::Log),
            "log" => (content.trim().to_string(), MemoryKind::Log),
            _ => return Err("memory tier must be profile, note, or log".into()),
        };
        let record = store.add_memory(&payload, created_at_ms, kind)
            .map_err(|error| format!("failed to persist Agent memory: {error}"))?;
        Ok(match record {
            Some(record) => json!({"stored":true,"memory":memory_json(record)}),
            None => json!({"stored":false,"memory":Value::Null}),
        })
    }

    pub fn remove(&self, agent_id: &str, memory_id: &str) -> Result<Value, String> {
        let memory_id = memory_id.trim();
        if memory_id.is_empty() || memory_id.len() > 64 { return Err("memoryId is invalid".into()); }
        let removed = self.store(agent_id)?.remove_memory(memory_id)
            .map_err(|error| format!("failed to remove Agent memory: {error}"))?;
        Ok(json!({"removed":removed}))
    }

    pub fn clear(&self, agent_id: &str) -> Result<Value, String> {
        self.store(agent_id)?.clear_memories()
            .map_err(|error| format!("failed to clear Agent memory: {error}"))?;
        Ok(json!({"cleared":true}))
    }

    fn store(&self, agent_id: &str) -> Result<FileMemoryStore, String> {
        let agent_id = agent_id.trim();
        if !is_safe_project_slug(agent_id) { return Err("invalid Agent id for memory scope".into()); }
        Ok(FileMemoryStore::new(get_agent_memory_dir(self.agents_root.join(agent_id))))
    }
}

fn memory_json(record: MemoryRecord) -> Value {
    json!({
        "id": record.id,
        "content": record.content,
        "createdAt": record.created_at,
        "kind": match record.kind { MemoryKind::Profile => "profile", MemoryKind::Log => "log" },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_reopens_one_durable_memory_truth_without_duplicate_replay() {
        let root = tempfile::tempdir().unwrap();
        let owner = AndroidAgentMemoryOwner::open(root.path());
        assert_eq!(owner.write("agent-1", "  Remember   this ", "profile", 1_700_000_000_000).unwrap()["stored"], true);
        assert_eq!(owner.write("agent-1", "remember this", "log", 1_700_000_001_000).unwrap()["stored"], false);
        drop(owner);
        let reopened = AndroidAgentMemoryOwner::open(root.path());
        let rows = reopened.list("agent-1", 10).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["content"], "Remember this");
    }

    #[test]
    fn note_prefix_and_remove_are_durable() {
        let root = tempfile::tempdir().unwrap();
        let owner = AndroidAgentMemoryOwner::open(root.path());
        let written = owner.write("agent-1", "follow up tomorrow", "note", 1_700_000_000_000).unwrap();
        assert_eq!(written["memory"]["content"], "Note: follow up tomorrow");
        let id = written["memory"]["id"].as_str().unwrap().to_string();
        assert_eq!(owner.remove("agent-1", &id).unwrap()["removed"], true);
        drop(owner);
        assert!(AndroidAgentMemoryOwner::open(root.path()).list("agent-1", 10).unwrap().as_array().unwrap().is_empty());
    }

    #[test]
    fn unsafe_agent_id_and_unbounded_tier_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let owner = AndroidAgentMemoryOwner::open(root.path());
        assert!(owner.list("../other-account", 10).is_err());
        assert!(owner.write("agent-1", "fact", "unknown", 1_700_000_000_000).is_err());
    }
}
