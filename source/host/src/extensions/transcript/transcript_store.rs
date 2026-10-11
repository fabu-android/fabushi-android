use serde_json::Value;
use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct TranscriptStore {
    path: PathBuf,
    entries: Vec<Value>,
}

impl TranscriptStore {
    pub fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let entries = match fs::read_to_string(&path) {
            Ok(raw) => {
                let value: Value = serde_json::from_str(&raw)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                let entries = value
                    .as_array()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "transcript root must be an array"))?
                    .clone();
                validate_entries(&entries)?;
                entries
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };
        Ok(Self { path, entries })
    }

    pub fn get_transcript(&self) -> Vec<Value> {
        self.entries.clone()
    }

    pub fn page_for_agent(
        &self,
        agent_id: &str,
        before_seq: Option<usize>,
        limit: usize,
    ) -> io::Result<(Vec<Value>, Option<usize>)> {
        let agent_id = agent_id.trim();
        if agent_id.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "transcript page agent id is required",
            ));
        }
        if limit == 0 || limit > 500 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "transcript page limit must be between 1 and 500",
            ));
        }

        let scoped = self
            .entries
            .iter()
            .filter(|entry| entry.get("agentId").and_then(Value::as_str) == Some(agent_id))
            .collect::<Vec<_>>();
        let end = before_seq.unwrap_or(scoped.len()).min(scoped.len());
        let start = end.saturating_sub(limit);
        let page = scoped[start..end]
            .iter()
            .map(|entry| (*entry).clone())
            .collect::<Vec<_>>();
        let next_before_seq = (start > 0).then_some(start);
        Ok((page, next_before_seq))
    }

    pub fn contains_id(&self, id: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
    }

    pub fn entry(&self, id: &str) -> Option<Value> {
        self.entries
            .iter()
            .find(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
            .cloned()
    }

    pub fn set_transcript(&mut self, entries: &[Value]) -> io::Result<()> {
        validate_entries(entries)?;
        let previous = std::mem::replace(&mut self.entries, entries.to_vec());
        if let Err(error) = self.persist() {
            self.entries = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn append_entry(&mut self, entry: Value) -> io::Result<()> {
        entry_id(&entry)?;
        self.entries.push(entry);
        if let Err(error) = self.persist() {
            self.entries.pop();
            return Err(error);
        }
        Ok(())
    }

    pub fn append_entry_if_absent(&mut self, entry: Value) -> io::Result<bool> {
        let id = entry_id(&entry)?.to_string();
        if self.contains_id(&id) {
            return Ok(false);
        }
        self.append_entry(entry)?;
        Ok(true)
    }

    pub fn update_entry(
        &mut self,
        id: &str,
        mut update: impl FnMut(&Value) -> Value,
    ) -> io::Result<Option<Value>> {
        let previous = self.entries.clone();
        let mut updated = None;
        for entry in &mut self.entries {
            if entry.get("id").and_then(Value::as_str) != Some(id) {
                continue;
            }
            let next = update(entry);
            entry_id(&next)?;
            updated = Some(next.clone());
            *entry = next;
        }
        if updated.is_none() {
            return Ok(None);
        }
        if let Err(error) = self.persist() {
            self.entries = previous;
            return Err(error);
        }
        Ok(updated)
    }

    pub fn remove_entry(&mut self, id: &str) -> io::Result<bool> {
        let previous = self.entries.clone();
        self.entries
            .retain(|entry| entry.get("id").and_then(Value::as_str) != Some(id));
        if self.entries.len() == previous.len() {
            return Ok(false);
        }
        if let Err(error) = self.persist() {
            self.entries = previous;
            return Err(error);
        }
        Ok(true)
    }

    pub fn clear_transcript(&mut self) -> io::Result<()> {
        let previous = std::mem::take(&mut self.entries);
        if let Err(error) = self.persist() {
            self.entries = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn persist(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.path.with_extension("json.tmp");
        let serialized = serde_json::to_string_pretty(&self.entries)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        fs::write(&temporary, format!("{serialized}\n"))?;
        fs::rename(temporary, &self.path)
    }
}

fn entry_id(entry: &Value) -> io::Result<&str> {
    entry
        .as_object()
        .and_then(|object| object.get("id"))
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "transcript entry id is required"))
}

fn validate_entries(entries: &[Value]) -> io::Result<()> {
    for entry in entries {
        entry_id(entry)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("fabushi-transcript-{}-{stamp}", std::process::id()))
            .join("transcript.json")
    }

    #[test]
    fn agent_scoped_pages_use_exclusive_cursor_and_survive_reopen() {
        let path = temp_path();
        let mut store = TranscriptStore::open(&path).unwrap();
        for (id, agent_id) in [
            ("a-1", "agent-a"),
            ("b-1", "agent-b"),
            ("a-2", "agent-a"),
            ("a-3", "agent-a"),
            ("b-2", "agent-b"),
            ("a-4", "agent-a"),
            ("a-5", "agent-a"),
        ] {
            store
                .append_entry(json!({"id":id,"agentId":agent_id,"kind":"message","content":id}))
                .unwrap();
        }

        let (latest, cursor) = store.page_for_agent("agent-a", None, 2).unwrap();
        assert_eq!(
            latest.iter().map(|entry| entry["id"].as_str().unwrap()).collect::<Vec<_>>(),
            vec!["a-4", "a-5"],
        );
        assert_eq!(cursor, Some(3));

        store
            .append_entry(json!({"id":"a-6","agentId":"agent-a","kind":"message","content":"a-6"}))
            .unwrap();
        let (older, next) = store.page_for_agent("agent-a", cursor, 2).unwrap();
        assert_eq!(
            older.iter().map(|entry| entry["id"].as_str().unwrap()).collect::<Vec<_>>(),
            vec!["a-2", "a-3"],
            "exclusive cursor must not be shifted by newer appends",
        );
        assert_eq!(next, Some(1));

        drop(store);
        let reopened = TranscriptStore::open(&path).unwrap();
        let (oldest, exhausted) = reopened.page_for_agent("agent-a", next, 2).unwrap();
        assert_eq!(
            oldest.iter().map(|entry| entry["id"].as_str().unwrap()).collect::<Vec<_>>(),
            vec!["a-1"],
        );
        assert_eq!(exhausted, None);
        assert!(reopened.page_for_agent("agent-b", None, 0).is_err());
        assert!(reopened.page_for_agent("", None, 2).is_err());

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn append_update_remove_clear_follow_in_memory_contract_and_survive_reopen() {
        let path = temp_path();
        let mut store = TranscriptStore::open(&path).unwrap();
        assert!(store.get_transcript().is_empty());

        assert!(store
            .append_entry_if_absent(json!({"id":"u1","kind":"message","role":"user","content":"hi"}))
            .unwrap());
        assert!(!store
            .append_entry_if_absent(json!({"id":"u1","kind":"message","role":"user","content":"duplicate"}))
            .unwrap());
        store
            .append_entry(json!({"id":"a1","kind":"message","role":"assistant","content":"hello"}))
            .unwrap();

        drop(store);
        let mut reopened = TranscriptStore::open(&path).unwrap();
        assert_eq!(reopened.get_transcript().len(), 2);
        let updated = reopened
            .update_entry("a1", |entry| {
                let mut entry = entry.clone();
                entry["content"] = Value::String("updated".into());
                entry
            })
            .unwrap()
            .unwrap();
        assert_eq!(updated["content"], "updated");
        assert!(reopened.remove_entry("u1").unwrap());
        assert!(!reopened.remove_entry("missing").unwrap());
        reopened.clear_transcript().unwrap();
        assert!(TranscriptStore::open(&path).unwrap().get_transcript().is_empty());

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
