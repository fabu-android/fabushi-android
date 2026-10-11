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
