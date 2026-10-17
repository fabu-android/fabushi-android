use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const XUSER_TURN_DEDUPE_FILE_NAME: &str = "host-xuser-turn-nonces.json";
pub const XUSER_TURN_DEDUPE_TTL_MS: u64 = 3 * 24 * 60 * 60 * 1_000;

pub trait XuserTurnDedupe: Send + Sync {
    fn mark_seen_if_new(&self, nonce: &str) -> io::Result<bool>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedNonce {
    nonce: String,
    expires_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedState {
    version: u32,
    #[serde(default)]
    seen: Vec<PersistedNonce>,
}

fn parse_dedupe_file(raw: Option<&str>) -> BTreeMap<String, u64> {
    let Some(raw) = raw else { return BTreeMap::new() };
    let Ok(state) = serde_json::from_str::<PersistedState>(raw) else {
        return BTreeMap::new();
    };
    if state.version != 1 {
        return BTreeMap::new();
    }
    state
        .seen
        .into_iter()
        .filter(|item| !item.nonce.is_empty())
        .map(|item| (item.nonce, item.expires_at_ms))
        .collect()
}

fn persist_atomic(path: &Path, map: &BTreeMap<String, u64>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("json.tmp-{}", std::process::id()));
    let state = PersistedState {
        version: 1,
        seen: map
            .iter()
            .map(|(nonce, expires_at_ms)| PersistedNonce {
                nonce: nonce.clone(),
                expires_at_ms: *expires_at_ms,
            })
            .collect(),
    };
    let bytes = serde_json::to_vec(&state)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub struct XuserTurnDedupeStore {
    file_path: PathBuf,
    ttl_ms: u64,
    now_ms: Box<dyn Fn() -> u64 + Send + Sync>,
    cache: Mutex<Option<BTreeMap<String, u64>>>,
}

impl XuserTurnDedupeStore {
    pub fn new(root: &Path, now_ms: Box<dyn Fn() -> u64 + Send + Sync>) -> Self {
        Self::with_ttl(root, XUSER_TURN_DEDUPE_TTL_MS, now_ms)
    }

    pub fn with_ttl(
        root: &Path,
        ttl_ms: u64,
        now_ms: Box<dyn Fn() -> u64 + Send + Sync>,
    ) -> Self {
        Self {
            file_path: root.join(XUSER_TURN_DEDUPE_FILE_NAME),
            ttl_ms,
            now_ms,
            cache: Mutex::new(None),
        }
    }

    fn load(&self) -> BTreeMap<String, u64> {
        parse_dedupe_file(fs::read_to_string(&self.file_path).ok().as_deref())
    }
}

impl XuserTurnDedupe for XuserTurnDedupeStore {
    fn mark_seen_if_new(&self, nonce: &str) -> io::Result<bool> {
        let nonce = nonce.trim();
        if nonce.is_empty() {
            return Ok(false);
        }
        if nonce.len() > 512 || nonce.chars().any(char::is_control) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid xuser turn nonce"));
        }

        let mut guard = self.cache.lock().map_err(|_| io::Error::other("xuser dedupe lock poisoned"))?;
        if guard.is_none() {
            *guard = Some(self.load());
        }
        let map = guard.as_mut().expect("dedupe cache initialized");
        let now = (self.now_ms)();
        let before = map.len();
        map.retain(|_, expires_at_ms| *expires_at_ms > now);
        if map.contains_key(nonce) {
            if map.len() != before {
                persist_atomic(&self.file_path, map)?;
            }
            return Ok(false);
        }
        map.insert(nonce.to_string(), now.saturating_add(self.ttl_ms));
        persist_atomic(&self.file_path, map)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-xuser-dedupe-{label}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos(),
        ))
    }

    #[test]
    fn duplicate_remote_turn_is_rejected_across_reopen_until_ttl_expires() {
        let root = root("reopen");
        let clock = Arc::new(AtomicU64::new(1_000));
        let first_clock = Arc::clone(&clock);
        let first = XuserTurnDedupeStore::with_ttl(
            &root,
            100,
            Box::new(move || first_clock.load(Ordering::SeqCst)),
        );
        assert!(first.mark_seen_if_new("nonce-a").unwrap());
        assert!(!first.mark_seen_if_new("nonce-a").unwrap());
        drop(first);

        let second_clock = Arc::clone(&clock);
        let reopened = XuserTurnDedupeStore::with_ttl(
            &root,
            100,
            Box::new(move || second_clock.load(Ordering::SeqCst)),
        );
        assert!(!reopened.mark_seen_if_new("nonce-a").unwrap());
        clock.store(1_101, Ordering::SeqCst);
        assert!(reopened.mark_seen_if_new("nonce-a").unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_nonce_fails_closed_and_atomic_write_leaves_no_part_file() {
        let root = root("atomic");
        let store = XuserTurnDedupeStore::new(&root, Box::new(|| 42));
        assert!(!store.mark_seen_if_new("   ").unwrap());
        assert!(store.mark_seen_if_new("bad\nnonce").is_err());
        assert!(store.mark_seen_if_new("nonce-ok").unwrap());
        assert!(
            fs::read_dir(&root)
                .unwrap()
                .all(|entry| !entry.unwrap().file_name().to_string_lossy().contains("tmp-")),
        );
        let _ = fs::remove_dir_all(root);
    }
}
