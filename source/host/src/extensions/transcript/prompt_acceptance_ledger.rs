use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const NONCE_DIGEST_MISMATCH: &str = "NONCE_DIGEST_MISMATCH";
pub const SEND_ACCEPTANCE_FILE_NAME: &str = "send-acceptance.json";
pub const MAX_RECORDS: usize = 256;
pub const MAX_DEGRADED_WINDOWS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rich_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to_id: Option<String>,
    #[serde(default)]
    pub is_fork: bool,
    #[serde(default)]
    pub attachment_paths: Vec<String>,
    #[serde(default)]
    pub attachment_names: Vec<String>,
}

pub fn canonical_send_input(input: &SendInput) -> String {
    serde_json::to_string(&vec![
        input.agent_id.clone().map(Value::String).unwrap_or(Value::Null),
        Value::String(input.prompt.clone()),
        input.rich_text.clone().map(Value::String).unwrap_or(Value::Null),
        input.reply_to_id.clone().map(Value::String).unwrap_or(Value::Null),
        Value::Bool(input.is_fork),
        serde_json::to_value(&input.attachment_paths).unwrap_or_else(|_| Value::Array(Vec::new())),
        serde_json::to_value(&input.attachment_names).unwrap_or_else(|_| Value::Array(Vec::new())),
    ])
    .expect("send input canonical serialization is infallible")
}

pub fn send_input_digest(input: &SendInput) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical_send_input(input).as_bytes());
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceptanceStatus {
    Accepted,
    Rejected,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceRecord {
    pub account_slot: String,
    pub client_nonce: String,
    pub input_digest: String,
    pub status: AcceptanceStatus,
    pub accepted_at_ms: u64,
    pub agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub echo_entry_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection_code: Option<String>,
}

impl AcceptanceRecord {
    fn valid(&self) -> bool {
        !self.account_slot.trim().is_empty()
            && !self.client_nonce.trim().is_empty()
            && !self.input_digest.trim().is_empty()
            && !self.agent_id.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DegradedWindow {
    pub from_ms: u64,
    pub to_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceGaps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eviction_horizon_ms: Option<u64>,
    #[serde(default)]
    pub degraded_windows: Vec<DegradedWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corrupt_reset_at_ms: Option<u64>,
}

impl AcceptanceGaps {
    fn is_empty(&self) -> bool {
        self.eviction_horizon_ms.is_none()
            && self.degraded_windows.is_empty()
            && self.corrupt_reset_at_ms.is_none()
    }

    fn note_eviction(&mut self, accepted_at_ms: u64) {
        self.eviction_horizon_ms = Some(
            self.eviction_horizon_ms
                .unwrap_or(accepted_at_ms)
                .max(accepted_at_ms),
        );
    }

    fn note_corrupt_reset(&mut self, at_ms: u64) {
        self.corrupt_reset_at_ms = Some(
            self.corrupt_reset_at_ms
                .unwrap_or(at_ms)
                .max(at_ms),
        );
    }

    fn note_degraded_window(&mut self, window: DegradedWindow) {
        if self.degraded_windows.len() < MAX_DEGRADED_WINDOWS {
            self.degraded_windows.push(window);
            return;
        }
        if let Some(last) = self.degraded_windows.last_mut() {
            last.from_ms = last.from_ms.min(window.from_ms);
            last.to_ms = last.to_ms.max(window.to_ms);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptanceLookup {
    Found(AcceptanceRecord),
    UnknownDurability,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendAdmission {
    Dispatch,
    Duplicate(AcceptanceRecord),
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PromptAcceptanceError {
    #[error("{NONCE_DIGEST_MISMATCH}: client nonce {client_nonce} was previously recorded with another input digest")]
    DigestMismatch { client_nonce: String },
    #[error("send rejected: {code}")]
    Rejected { code: String },
    #[error("pending acceptance identity is incomplete")]
    InvalidPendingIdentity,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcceptanceFile {
    version: u32,
    records: Vec<AcceptanceRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    history_gaps: Option<AcceptanceGaps>,
}

struct LoadedLedger {
    records: HashMap<(String, String), AcceptanceRecord>,
    gaps: AcceptanceGaps,
}

pub struct PromptAcceptanceLedger {
    file_path: Option<PathBuf>,
    loaded: Option<LoadedLedger>,
    now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
    degraded_since_ms: Option<u64>,
    marker_may_exist: bool,
}

impl PromptAcceptanceLedger {
    pub fn new(root_dir: Option<impl AsRef<Path>>) -> Self {
        Self::with_clock(root_dir, Arc::new(system_now_ms))
    }

    pub fn with_clock(
        root_dir: Option<impl AsRef<Path>>,
        now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
    ) -> Self {
        Self {
            file_path: root_dir.map(|root| root.as_ref().join(SEND_ACCEPTANCE_FILE_NAME)),
            loaded: None,
            now_ms,
            degraded_since_ms: None,
            marker_may_exist: false,
        }
    }

    pub fn file_path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    pub fn marker_path(&self) -> Option<PathBuf> {
        self.file_path
            .as_ref()
            .map(|path| PathBuf::from(format!("{}.degraded", path.display())))
    }

    pub fn lookup(&mut self, account_slot: &str, client_nonce: &str) -> AcceptanceLookup {
        self.ensure_loaded();
        let loaded = self.loaded.as_ref().expect("ledger loaded");
        if let Some(record) = loaded
            .records
            .get(&(account_slot.to_string(), client_nonce.to_string()))
        {
            return AcceptanceLookup::Found(record.clone());
        }
        if !loaded.gaps.is_empty() || self.degraded_since_ms.is_some() {
            AcceptanceLookup::UnknownDurability
        } else {
            AcceptanceLookup::NotFound
        }
    }

    pub fn admit_send(
        &mut self,
        account_slot: &str,
        client_nonce: &str,
        input_digest: &str,
    ) -> Result<SendAdmission, PromptAcceptanceError> {
        match self.lookup(account_slot, client_nonce) {
            AcceptanceLookup::Found(record) => {
                if record.input_digest != input_digest {
                    return Err(PromptAcceptanceError::DigestMismatch {
                        client_nonce: client_nonce.to_string(),
                    });
                }
                if record.status == AcceptanceStatus::Rejected {
                    return Err(PromptAcceptanceError::Rejected {
                        code: record
                            .rejection_code
                            .unwrap_or_else(|| "unknown".to_string()),
                    });
                }
                Ok(SendAdmission::Duplicate(record))
            }
            AcceptanceLookup::UnknownDurability | AcceptanceLookup::NotFound => {
                Ok(SendAdmission::Dispatch)
            }
        }
    }

    pub fn record_pending(
        &mut self,
        account_slot: impl Into<String>,
        client_nonce: impl Into<String>,
        input_digest: impl Into<String>,
        agent_id: impl Into<String>,
        echo_entry_id: Option<String>,
    ) -> Result<AcceptanceRecord, PromptAcceptanceError> {
        let account_slot = account_slot.into();
        let client_nonce = client_nonce.into();
        let input_digest = input_digest.into();
        let agent_id = agent_id.into();
        if account_slot.trim().is_empty()
            || client_nonce.trim().is_empty()
            || input_digest.trim().is_empty()
            || agent_id.trim().is_empty()
        {
            return Err(PromptAcceptanceError::InvalidPendingIdentity);
        }
        self.ensure_loaded();
        let key = (account_slot.clone(), client_nonce.clone());
        if let Some(existing) = self
            .loaded
            .as_ref()
            .expect("loaded")
            .records
            .get(&key)
            .cloned()
        {
            if existing.input_digest != input_digest {
                return Err(PromptAcceptanceError::DigestMismatch { client_nonce });
            }
            return Ok(existing);
        }

        let record = AcceptanceRecord {
            account_slot,
            client_nonce,
            input_digest,
            status: AcceptanceStatus::Pending,
            accepted_at_ms: (self.now_ms)(),
            agent_id,
            echo_entry_id,
            rejection_code: None,
        };
        self.loaded
            .as_mut()
            .expect("loaded")
            .records
            .insert(key, record.clone());
        self.evict_past_cap();
        self.persist();
        Ok(record)
    }

    pub fn mark_accepted(&mut self, account_slot: &str, client_nonce: &str) {
        self.transition_pending(account_slot, client_nonce, AcceptanceStatus::Accepted, None);
    }

    pub fn mark_rejected(
        &mut self,
        account_slot: &str,
        client_nonce: &str,
        rejection_code: impl Into<String>,
    ) {
        self.transition_pending(
            account_slot,
            client_nonce,
            AcceptanceStatus::Rejected,
            Some(rejection_code.into()),
        );
    }

    pub fn clear(&mut self, account_slot: &str, client_nonce: &str) {
        self.ensure_loaded();
        if self
            .loaded
            .as_mut()
            .expect("loaded")
            .records
            .remove(&(account_slot.to_string(), client_nonce.to_string()))
            .is_some()
        {
            self.persist();
        }
    }

    pub fn clear_unless_accepted(&mut self, account_slot: &str, client_nonce: &str) {
        self.ensure_loaded();
        let key = (account_slot.to_string(), client_nonce.to_string());
        let accepted = self
            .loaded
            .as_ref()
            .expect("loaded")
            .records
            .get(&key)
            .is_some_and(|record| record.status == AcceptanceStatus::Accepted);
        if !accepted {
            self.clear(account_slot, client_nonce);
        }
    }

    pub fn record_count(&mut self) -> usize {
        self.ensure_loaded();
        self.loaded.as_ref().expect("loaded").records.len()
    }

    pub fn gaps(&mut self) -> AcceptanceGaps {
        self.ensure_loaded();
        self.loaded.as_ref().expect("loaded").gaps.clone()
    }

    pub fn durability_degraded(&self) -> bool {
        self.degraded_since_ms.is_some()
    }

    pub fn dispose(&mut self) {
        self.persist();
    }

    fn transition_pending(
        &mut self,
        account_slot: &str,
        client_nonce: &str,
        status: AcceptanceStatus,
        rejection_code: Option<String>,
    ) {
        self.ensure_loaded();
        let key = (account_slot.to_string(), client_nonce.to_string());
        let Some(existing) = self
            .loaded
            .as_mut()
            .expect("loaded")
            .records
            .get_mut(&key)
        else {
            return;
        };
        if existing.status != AcceptanceStatus::Pending {
            return;
        }
        existing.status = status;
        existing.rejection_code = rejection_code;
        self.persist();
    }

    fn evict_past_cap(&mut self) {
        self.ensure_loaded();
        let loaded = self.loaded.as_mut().expect("loaded");
        while loaded.records.len() > MAX_RECORDS {
            let completed_key = loaded
                .records
                .iter()
                .filter(|(_, record)| record.status != AcceptanceStatus::Pending)
                .min_by_key(|(_, record)| record.accepted_at_ms)
                .map(|(key, _)| key.clone());
            let key = completed_key.or_else(|| {
                loaded
                    .records
                    .iter()
                    .min_by_key(|(_, record)| record.accepted_at_ms)
                    .map(|(key, _)| key.clone())
            });
            let Some(key) = key else {
                break;
            };
            if let Some(record) = loaded.records.remove(&key) {
                loaded.gaps.note_eviction(record.accepted_at_ms);
            }
        }
    }

    fn ensure_loaded(&mut self) {
        if self.loaded.is_some() {
            return;
        }
        let now = (self.now_ms)();
        let mut records = HashMap::new();
        let mut gaps = AcceptanceGaps::default();
        let mut damaged = false;

        if let Some(path) = &self.file_path {
            match fs::read_to_string(path) {
                Ok(raw) => match serde_json::from_str::<AcceptanceFile>(&raw) {
                    Ok(file) => {
                        gaps = file.history_gaps.unwrap_or_default();
                        for record in file.records {
                            if record.valid() {
                                records.insert(
                                    (record.account_slot.clone(), record.client_nonce.clone()),
                                    record,
                                );
                            } else {
                                damaged = true;
                            }
                        }
                    }
                    Err(_) => damaged = true,
                },
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => damaged = true,
            }

            if damaged {
                if path.exists() {
                    let backup = PathBuf::from(format!("{}.corrupt-{now}", path.display()));
                    let _ = fs::copy(path, backup);
                }
                gaps.note_corrupt_reset(now);
            }

            if let Some(marker) = self.read_degraded_marker() {
                gaps.note_degraded_window(DegradedWindow {
                    from_ms: marker,
                    to_ms: now,
                });
                self.marker_may_exist = true;
            }
        }

        self.loaded = Some(LoadedLedger { records, gaps });
        self.evict_past_cap();
        if damaged || self.marker_may_exist {
            self.persist();
        }
    }

    fn read_degraded_marker(&self) -> Option<u64> {
        let path = self.marker_path()?;
        let raw = fs::read_to_string(path).ok()?;
        let value = serde_json::from_str::<Value>(&raw).ok()?;
        value.get("sinceMs").and_then(Value::as_u64).or(Some(0))
    }

    fn persist(&mut self) {
        let Some(file_path) = self.file_path.clone() else {
            return;
        };
        let Some(loaded) = self.loaded.as_ref() else {
            return;
        };
        let now = (self.now_ms)();
        let mut gaps = loaded.gaps.clone();
        if let Some(from_ms) = self.degraded_since_ms {
            gaps.note_degraded_window(DegradedWindow {
                from_ms,
                to_ms: now,
            });
        }
        let mut records = loaded.records.values().cloned().collect::<Vec<_>>();
        records.sort_by_key(|record| record.accepted_at_ms);
        let payload = AcceptanceFile {
            version: 1,
            records,
            history_gaps: (!gaps.is_empty()).then_some(gaps.clone()),
        };
        let bytes = match serde_json::to_vec(&payload) {
            Ok(bytes) => bytes,
            Err(_) => return,
        };
        let part = PathBuf::from(format!("{}.part", file_path.display()));
        let write_result = (|| -> std::io::Result<()> {
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&part, bytes)?;
            fs::rename(&part, &file_path)?;
            Ok(())
        })();

        if write_result.is_err() {
            let _ = fs::remove_file(&part);
            self.degraded_since_ms.get_or_insert(now);
            if !self.marker_may_exist {
                if let Some(marker) = self.marker_path() {
                    let _ = fs::write(
                        marker,
                        serde_json::to_vec(&serde_json::json!({
                            "version": 1,
                            "sinceMs": self.degraded_since_ms.unwrap_or(now)
                        }))
                        .unwrap_or_default(),
                    );
                    self.marker_may_exist = true;
                }
            }
            return;
        }

        if let Some(loaded) = self.loaded.as_mut() {
            loaded.gaps = gaps;
        }
        self.degraded_since_ms = None;
        if self.marker_may_exist {
            if let Some(marker) = self.marker_path() {
                if fs::remove_file(marker).is_ok() {
                    self.marker_may_exist = false;
                }
            }
        }
    }
}

fn system_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
