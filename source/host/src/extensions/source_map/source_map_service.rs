use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const BOX_STORE_SOURCE_KEY: &str = "box-store";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandSourceMode {
    Local,
    AgentStore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandSourceEntry {
    #[serde(rename = "sourceId")]
    pub source_id: String,
    pub mode: SandSourceMode,
}

fn parse_source_map(value: Value) -> BTreeMap<String, SandSourceEntry> {
    let Some(object) = value.as_object() else {
        return BTreeMap::new();
    };
    object
        .iter()
        .filter_map(|(key, value)| {
            let entry = serde_json::from_value::<SandSourceEntry>(value.clone()).ok()?;
            (!entry.source_id.trim().is_empty()).then(|| (key.clone(), entry))
        })
        .collect()
}

type SourceIdFactory = Arc<dyn Fn() -> String + Send + Sync>;

/// Canonical durable mapping from Agent/storage identity to backend source identity.
///
/// Android requires an explicit app-private path supplied by the Host. It intentionally does not
/// infer Desktop HOME/SAND roots.
pub struct SandSourceMap {
    path: PathBuf,
    create_id: SourceIdFactory,
    cache: Mutex<Option<BTreeMap<String, SandSourceEntry>>>,
}

impl std::fmt::Debug for SandSourceMap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SandSourceMap")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl SandSourceMap {
    pub fn new(path: PathBuf) -> Self {
        Self::with_create_id(path, || Uuid::new_v4().to_string())
    }

    pub fn with_create_id<F>(path: PathBuf, create_id: F) -> Self
    where
        F: Fn() -> String + Send + Sync + 'static,
    {
        Self {
            path,
            create_id: Arc::new(create_id),
            cache: Mutex::new(None),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn get_or_create(&self, agent_id: &str) -> io::Result<SandSourceEntry> {
        validate_key(agent_id)?;
        let mut map = self.load();
        if let Some(existing) = map.get(agent_id) {
            return Ok(existing.clone());
        }
        let entry = SandSourceEntry {
            source_id: validated_new_source_id((self.create_id)())?,
            mode: SandSourceMode::Local,
        };
        map.insert(agent_id.to_string(), entry.clone());
        self.save(&map)?;
        Ok(entry)
    }

    pub fn get_or_create_box_store(&self) -> io::Result<SandSourceEntry> {
        self.get_or_create(BOX_STORE_SOURCE_KEY)
    }

    pub fn get_box_store(&self) -> Option<SandSourceEntry> {
        self.load().get(BOX_STORE_SOURCE_KEY).cloned()
    }

    pub fn set_mode(&self, agent_id: &str, mode: SandSourceMode) -> io::Result<SandSourceEntry> {
        validate_key(agent_id)?;
        let mut map = self.load();
        let source_id = match map.get(agent_id) {
            Some(entry) => entry.source_id.clone(),
            None => validated_new_source_id((self.create_id)())?,
        };
        let entry = SandSourceEntry { source_id, mode };
        map.insert(agent_id.to_string(), entry.clone());
        self.save(&map)?;
        Ok(entry)
    }

    fn load(&self) -> BTreeMap<String, SandSourceEntry> {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(map) = cache.as_ref() {
            return map.clone();
        }
        let map = fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .map(parse_source_map)
            .unwrap_or_default();
        *cache = Some(map.clone());
        map
    }

    fn save(&self, map: &BTreeMap<String, SandSourceEntry>) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.path.with_extension(format!("tmp-{}", std::process::id()));
        let encoded = serde_json::to_vec_pretty(map)
            .map_err(|error| io::Error::other(error.to_string()))?;
        let result = (|| -> io::Result<()> {
            let mut file = fs::File::create(&temporary)?;
            file.write_all(&encoded)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
            return result;
        }
        *self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(map.clone());
        Ok(())
    }
}

fn validate_key(value: &str) -> io::Result<()> {
    if value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source-map key must be bounded, non-empty, and contain no control characters",
        ));
    }
    Ok(())
}

fn validated_new_source_id(value: String) -> io::Result<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "source id factory returned an invalid identifier",
        ));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_identity_is_stable_across_mode_change_and_process_reopen() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("source-map.json");
        let map = SandSourceMap::with_create_id(path.clone(), || "source-1".into());
        let created = map.get_or_create("agent-1").unwrap();
        assert_eq!(created.source_id, "source-1");
        assert_eq!(created.mode, SandSourceMode::Local);
        let changed = map.set_mode("agent-1", SandSourceMode::AgentStore).unwrap();
        assert_eq!(changed.source_id, "source-1");
        drop(map);

        let reopened = SandSourceMap::with_create_id(path, || "source-2".into());
        let restored = reopened.get_or_create("agent-1").unwrap();
        assert_eq!(restored.source_id, "source-1");
        assert_eq!(restored.mode, SandSourceMode::AgentStore);
    }

    #[test]
    fn box_store_identity_is_reserved_and_durable() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("source-map.json");
        let map = SandSourceMap::with_create_id(path.clone(), || "box-source".into());
        assert_eq!(
            map.get_or_create_box_store().unwrap().source_id,
            "box-source"
        );
        drop(map);
        let reopened = SandSourceMap::with_create_id(path, || "replacement".into());
        assert_eq!(reopened.get_box_store().unwrap().source_id, "box-source");
    }

    #[test]
    fn invalid_identity_inputs_fail_closed_without_persisting_partial_state() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("source-map.json");
        let map = SandSourceMap::with_create_id(path.clone(), || "   ".into());
        assert!(map.get_or_create("agent-1").is_err());
        assert!(!path.exists());
        assert!(map.get_or_create("   ").is_err());
    }
}
