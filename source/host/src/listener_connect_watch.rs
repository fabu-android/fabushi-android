use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

pub const LISTENER_CONNECT_WATCH_TIMEOUT_MS: u64 = 15 * 60 * 1_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingListenerConnectWatch {
    pub account_fence: String,
    pub agent_id: String,
    pub platform: String,
    pub generation: u64,
    pub expires_at_ms: u64,
    pub armed: bool,
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DurableState {
    next_generation: u64,
    watches: BTreeMap<String, PendingListenerConnectWatch>,
}

#[derive(Debug)]
pub struct DurableListenerConnectWatchStore {
    path: PathBuf,
    state: DurableState,
}

impl DurableListenerConnectWatchStore {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let path = path.into();
        let state = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| format!("failed to parse listener connect watch journal: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => DurableState::default(),
            Err(error) => return Err(format!("failed to read listener connect watch journal: {error}")),
        };
        let mut store = Self { path, state };
        if store.remove_expired(now_ms) {
            store.persist()?;
        }
        Ok(store)
    }

    pub fn watch(
        &mut self,
        account_fence: &str,
        agent_id: &str,
        platform: &str,
        now_ms: u64,
    ) -> Result<u64, String> {
        let key = watch_key(account_fence, agent_id, platform)?;
        if let Some(existing) = self.state.watches.get(&key) {
            return Ok(existing.generation);
        }
        self.state.next_generation = self.state.next_generation.saturating_add(1).max(1);
        let generation = self.state.next_generation;
        self.state.watches.insert(key, PendingListenerConnectWatch {
            account_fence: account_fence.to_string(),
            agent_id: agent_id.to_string(),
            platform: platform.to_string(),
            generation,
            expires_at_ms: now_ms.saturating_add(LISTENER_CONNECT_WATCH_TIMEOUT_MS),
            armed: false,
            ready: false,
        });
        self.persist()?;
        Ok(generation)
    }

    pub fn pending_for_account(&mut self, account_fence: &str, now_ms: u64) -> Result<Vec<PendingListenerConnectWatch>, String> {
        if self.remove_expired(now_ms) {
            self.persist()?;
        }
        Ok(self.state.watches.values().filter(|watch| watch.account_fence == account_fence).cloned().collect())
    }

    pub fn observe(
        &mut self,
        account_fence: &str,
        agent_id: &str,
        platform: &str,
        generation: u64,
        connected: bool,
        now_ms: u64,
    ) -> Result<bool, String> {
        let key = watch_key(account_fence, agent_id, platform)?;
        let Some(snapshot) = self.state.watches.get(&key).cloned() else { return Ok(false); };
        if snapshot.generation != generation || snapshot.expires_at_ms <= now_ms {
            if snapshot.expires_at_ms <= now_ms {
                self.state.watches.remove(&key);
                self.persist()?;
            }
            return Ok(false);
        }
        let Some(watch) = self.state.watches.get_mut(&key) else { return Ok(false); };
        if watch.ready {
            return Ok(true);
        }
        if !connected {
            if !watch.armed {
                watch.armed = true;
                self.persist()?;
            }
            return Ok(false);
        }
        if !watch.armed {
            return Ok(false);
        }
        watch.ready = true;
        self.persist()?;
        Ok(true)
    }

    pub fn ready_for_account(&self, account_fence: &str) -> Vec<PendingListenerConnectWatch> {
        self.state.watches.values().filter(|watch| watch.account_fence == account_fence && watch.ready).cloned().collect()
    }

    pub fn acknowledge(&mut self, watch: &PendingListenerConnectWatch) -> Result<bool, String> {
        let key = watch_key(&watch.account_fence, &watch.agent_id, &watch.platform)?;
        let remove = self.state.watches.get(&key).is_some_and(|current| current.generation == watch.generation && current.ready);
        if remove {
            self.state.watches.remove(&key);
            self.persist()?;
        }
        Ok(remove)
    }

    fn remove_expired(&mut self, now_ms: u64) -> bool {
        let before = self.state.watches.len();
        self.state.watches.retain(|_, watch| watch.expires_at_ms > now_ms);
        before != self.state.watches.len()
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("failed to create listener watch directory: {error}"))?;
        }
        let bytes = serde_json::to_vec_pretty(&self.state)
            .map_err(|error| format!("failed to encode listener connect watch journal: {error}"))?;
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, bytes).map_err(|error| format!("failed to write listener connect watch journal: {error}"))?;
        fs::rename(&tmp, &self.path).map_err(|error| format!("failed to replace listener connect watch journal: {error}"))
    }
}

fn watch_key(account_fence: &str, agent_id: &str, platform: &str) -> Result<String, String> {
    let account_fence = account_fence.trim();
    let agent_id = agent_id.trim();
    let platform = platform.trim();
    if account_fence.is_empty() || agent_id.is_empty() || platform.is_empty() {
        return Err("listener connect watch scope is incomplete".into());
    }
    Ok(format!("{}\n{}\n{}", crate::sha256::sha256_hex(account_fence.as_bytes()), agent_id, platform))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_watch_requires_disconnect_edge_dedupes_and_recovers_after_reopen() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("listener-watches.json");
        let mut store = DurableListenerConnectWatchStore::open(&path, 100).unwrap();
        let generation = store.watch("acct-a", "agent-1", "slack", 100).unwrap();
        assert_eq!(generation, store.watch("acct-a", "agent-1", "slack", 101).unwrap());
        assert!(!store.observe("acct-a", "agent-1", "slack", generation, true, 102).unwrap());
        assert!(!store.observe("acct-a", "agent-1", "slack", generation, false, 103).unwrap());
        drop(store);
        let mut reopened = DurableListenerConnectWatchStore::open(&path, 104).unwrap();
        assert!(reopened.observe("acct-a", "agent-1", "slack", generation, true, 104).unwrap());
        assert_eq!(reopened.ready_for_account("acct-a").len(), 1);
        assert!(reopened.ready_for_account("acct-b").is_empty(), "account switch must fence old ready callbacks");
        let ready = reopened.ready_for_account("acct-a").remove(0);
        assert!(!reopened.observe("acct-a", "agent-1", "slack", generation + 1, true, 105).unwrap(), "stale generation callback must be fenced");
        assert!(reopened.acknowledge(&ready).unwrap());
        assert!(!reopened.acknowledge(&ready).unwrap(), "duplicate completion ACK must be idempotent");
    }

    #[test]
    fn lookup_failure_can_retry_without_mutating_watch() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("listener-watches.json");
        let mut store = DurableListenerConnectWatchStore::open(&path, 100).unwrap();
        let generation = store.watch("acct-a", "agent-1", "github", 100).unwrap();
        let before = store.pending_for_account("acct-a", 101).unwrap();
        assert_eq!(before.len(), 1);
        let after = store.pending_for_account("acct-a", 102).unwrap();
        assert_eq!(after[0].generation, generation);
        assert!(!after[0].armed);
        assert!(!after[0].ready);
    }
}
