use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::durable_file_policy::SAND_ACK_OBLIGATIONS_FILE_NAME;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AckObligation {
    pub account_fence: String,
    pub agent_id: String,
    pub created_at_ms: f64,
    pub last_send_at_ms: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_interrupt_at_ms: Option<f64>,
    pub coalesced_count: f64,
    pub redrive_attempts: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordSendOutcome {
    pub obligation: AckObligation,
    pub created: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AckObligationFile {
    version: u8,
    pending: Vec<AckObligation>,
}

pub fn finite_number(value: Option<&Value>, fallback: f64) -> f64 {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
}

pub fn coerce_obligation(value: &Value) -> Option<AckObligation> {
    let entry = value.as_object()?;
    // Android's Host data root survives account switches. Legacy/global rows
    // without an account fence must never be claimed by a later account.
    let account_fence = entry
        .get("accountFence")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let agent_id = entry
        .get("agentId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let created_at_ms = finite_number(entry.get("createdAtMs"), 0.0);
    Some(AckObligation {
        account_fence,
        agent_id,
        created_at_ms,
        last_send_at_ms: finite_number(entry.get("lastSendAtMs"), created_at_ms),
        last_interrupt_at_ms: entry
            .get("lastInterruptAtMs")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite()),
        coalesced_count: finite_number(entry.get("coalescedCount"), 1.0).max(1.0),
        redrive_attempts: finite_number(entry.get("redriveAttempts"), 0.0).max(0.0),
    })
}

pub fn parse_ack_obligations_file(raw: Option<&str>) -> Vec<AckObligation> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    value
        .get("pending")
        .and_then(Value::as_array)
        .map(|pending| pending.iter().filter_map(coerce_obligation).collect())
        .unwrap_or_default()
}

fn same_key(entry: &AckObligation, account_fence: &str, agent_id: &str) -> bool {
    entry.account_fence == account_fence && entry.agent_id == agent_id
}

pub struct SandAckObligationStore {
    file_path: PathBuf,
    cache: Mutex<Option<Vec<AckObligation>>>,
}

impl SandAckObligationStore {
    pub fn new(root_dir: impl AsRef<Path>) -> Self {
        Self {
            file_path: root_dir.as_ref().join(SAND_ACK_OBLIGATIONS_FILE_NAME),
            cache: Mutex::new(None),
        }
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    pub fn get(&self, account_fence: &str, agent_id: &str) -> Option<AckObligation> {
        if account_fence.trim().is_empty() || agent_id.trim().is_empty() {
            return None;
        }
        self.list()
            .into_iter()
            .find(|entry| same_key(entry, account_fence, agent_id))
    }

    pub fn list(&self) -> Vec<AckObligation> {
        if let Ok(cache) = self.cache.lock() {
            if let Some(pending) = cache.as_ref() {
                return pending.clone();
            }
        }
        let raw = fs::read_to_string(&self.file_path).ok();
        let parsed = parse_ack_obligations_file(raw.as_deref());
        if let Ok(mut cache) = self.cache.lock() {
            *cache = Some(parsed.clone());
        }
        parsed
    }

    pub fn list_for_account(&self, account_fence: &str) -> Vec<AckObligation> {
        if account_fence.trim().is_empty() {
            return Vec::new();
        }
        self.list()
            .into_iter()
            .filter(|entry| entry.account_fence == account_fence)
            .collect()
    }

    pub fn record_send(
        &self,
        account_fence: &str,
        agent_id: &str,
        at_ms: f64,
    ) -> io::Result<RecordSendOutcome> {
        validate_key(account_fence, agent_id)?;
        if !at_ms.is_finite() || at_ms < 0.0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid ack send timestamp"));
        }
        let existing = self.get(account_fence, agent_id);
        let obligation = match existing.as_ref() {
            Some(existing) => AckObligation {
                last_send_at_ms: at_ms,
                coalesced_count: existing.coalesced_count + 1.0,
                ..existing.clone()
            },
            None => AckObligation {
                account_fence: account_fence.to_string(),
                agent_id: agent_id.to_string(),
                created_at_ms: at_ms,
                last_send_at_ms: at_ms,
                last_interrupt_at_ms: None,
                coalesced_count: 1.0,
                redrive_attempts: 0.0,
            },
        };
        self.upsert(obligation.clone())?;
        Ok(RecordSendOutcome {
            obligation,
            created: existing.is_none(),
        })
    }

    pub fn record_interrupt(
        &self,
        account_fence: &str,
        agent_id: &str,
        at_ms: f64,
    ) -> io::Result<bool> {
        if !at_ms.is_finite() || at_ms < 0.0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid ack interrupt timestamp"));
        }
        let Some(mut existing) = self.get(account_fence, agent_id) else {
            return Ok(false);
        };
        existing.last_interrupt_at_ms = Some(at_ms);
        self.upsert(existing)?;
        Ok(true)
    }

    pub fn record_redrive_attempt(
        &self,
        account_fence: &str,
        agent_id: &str,
    ) -> io::Result<Option<AckObligation>> {
        let Some(mut existing) = self.get(account_fence, agent_id) else {
            return Ok(None);
        };
        existing.redrive_attempts += 1.0;
        self.upsert(existing.clone())?;
        Ok(Some(existing))
    }

    pub fn clear(&self, account_fence: &str, agent_id: &str) -> io::Result<bool> {
        let current = self.list();
        let remaining = current
            .iter()
            .filter(|entry| !same_key(entry, account_fence, agent_id))
            .cloned()
            .collect::<Vec<_>>();
        if remaining.len() == current.len() {
            return Ok(false);
        }
        self.write(&remaining)?;
        Ok(true)
    }

    pub fn clear_account(&self, account_fence: &str) -> io::Result<usize> {
        if account_fence.trim().is_empty() {
            return Ok(0);
        }
        let current = self.list();
        let remaining = current
            .iter()
            .filter(|entry| entry.account_fence != account_fence)
            .cloned()
            .collect::<Vec<_>>();
        let removed = current.len().saturating_sub(remaining.len());
        if removed > 0 {
            self.write(&remaining)?;
        }
        Ok(removed)
    }

    pub fn upsert(&self, obligation: AckObligation) -> io::Result<()> {
        validate_key(&obligation.account_fence, &obligation.agent_id)?;
        let mut pending = self
            .list()
            .into_iter()
            .filter(|entry| {
                !same_key(entry, &obligation.account_fence, &obligation.agent_id)
            })
            .collect::<Vec<_>>();
        pending.push(obligation);
        self.write(&pending)
    }

    pub fn restore(
        &self,
        previous: Option<AckObligation>,
        account_fence: &str,
        agent_id: &str,
    ) -> io::Result<()> {
        match previous {
            Some(previous)
                if previous.account_fence == account_fence && previous.agent_id == agent_id =>
            {
                self.upsert(previous)
            }
            Some(_) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "ack obligation restore identity mismatch",
            )),
            None => {
                let _ = self.clear(account_fence, agent_id)?;
                Ok(())
            }
        }
    }

    fn write(&self, pending: &[AckObligation]) -> io::Result<()> {
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let encoded = serde_json::to_vec(&AckObligationFile {
            version: 2,
            pending: pending.to_vec(),
        })
        .map_err(io::Error::other)?;
        atomic_replace(&self.file_path, &encoded)?;
        if let Ok(mut cache) = self.cache.lock() {
            *cache = Some(pending.to_vec());
        }
        Ok(())
    }
}

fn validate_key(account_fence: &str, agent_id: &str) -> io::Result<()> {
    if account_fence.trim().is_empty() || agent_id.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ack obligation requires account fence and agent id",
        ));
    }
    Ok(())
}

fn atomic_replace(path: &Path, encoded: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "ack path has no parent"))?;
    fs::create_dir_all(parent)?;
    let part = path.with_extension(format!("part-{}", std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&part)?;
        file.write_all(encoded)?;
        file.sync_all()?;
        fs::rename(&part, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-ack-obligation-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn legacy_unfenced_rows_fail_closed() {
        let raw = r#"{"version":1,"pending":[{"agentId":"agent-a","createdAtMs":1,"lastSendAtMs":1,"coalescedCount":1,"redriveAttempts":0}]}"#;
        assert!(parse_ack_obligations_file(Some(raw)).is_empty());
    }

    #[test]
    fn reopen_coalesces_without_crossing_account_fence() {
        let root = root("account");
        {
            let store = SandAckObligationStore::new(&root);
            let first = store.record_send("acct-a", "agent-a", 10.0).unwrap();
            assert!(first.created);
            let second = store.record_send("acct-a", "agent-a", 20.0).unwrap();
            assert!(!second.created);
            assert_eq!(second.obligation.coalesced_count, 2.0);
            store.record_send("acct-b", "agent-a", 30.0).unwrap();
        }
        let reopened = SandAckObligationStore::new(&root);
        assert_eq!(reopened.list_for_account("acct-a").len(), 1);
        assert_eq!(reopened.list_for_account("acct-b").len(), 1);
        assert_eq!(
            reopened.get("acct-a", "agent-a").unwrap().last_send_at_ms,
            20.0
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn interrupt_redrive_restore_and_clear_are_identity_fenced() {
        let root = root("lifecycle");
        let store = SandAckObligationStore::new(&root);
        store.record_send("acct-a", "agent-a", 10.0).unwrap();
        let before = store.get("acct-a", "agent-a").unwrap();
        assert!(store
            .record_interrupt("acct-a", "agent-a", 15.0)
            .unwrap());
        let redriven = store
            .record_redrive_attempt("acct-a", "agent-a")
            .unwrap()
            .unwrap();
        assert_eq!(redriven.redrive_attempts, 1.0);
        assert!(store
            .restore(Some(before.clone()), "acct-b", "agent-a")
            .is_err());
        store
            .restore(Some(before), "acct-a", "agent-a")
            .unwrap();
        assert_eq!(
            store.get("acct-a", "agent-a").unwrap().redrive_attempts,
            0.0
        );
        assert!(store.clear("acct-a", "agent-a").unwrap());
        assert!(store.get("acct-a", "agent-a").is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn clear_account_never_deletes_another_accounts_obligation() {
        let root = root("clear-account");
        let store = SandAckObligationStore::new(&root);
        store.record_send("acct-a", "agent-a", 1.0).unwrap();
        store.record_send("acct-b", "agent-a", 1.0).unwrap();
        assert_eq!(store.clear_account("acct-a").unwrap(), 1);
        assert!(store.get("acct-a", "agent-a").is_none());
        assert!(store.get("acct-b", "agent-a").is_some());
        let _ = fs::remove_dir_all(root);
    }
}
