use fabushi_android_shared::ExecutionRequest;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use crate::sha256::sha256_hex;

pub const REMOTE_BROWSER_UA_CAPABILITY: &str = "browser.ua.reconcile";
pub const REMOTE_BROWSER_UA_TIMEOUT_MS: u64 = 30_000;
pub const REMOTE_BROWSER_UA_OWNER_STAMP_LENGTH: usize = 16;
const MAX_IDENTITY_BYTES: usize = 512;

pub fn owner_stamp_for_subject(subject: &str) -> Result<String, String> {
    let subject = subject.trim();
    if subject.is_empty() || subject.len() > MAX_IDENTITY_BYTES || subject.chars().any(char::is_control) {
        return Err("browser UA subject is invalid".into());
    }
    Ok(sha256_hex(subject.as_bytes())[..REMOTE_BROWSER_UA_OWNER_STAMP_LENGTH].to_string())
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBrowserUaProjection {
    pub owner_stamp: Option<String>,
    pub token_disabled: bool,
}

impl RemoteBrowserUaProjection {
    pub fn from_subject(subject: Option<&str>, token_disabled: bool) -> Result<Self, String> {
        Ok(Self {
            owner_stamp: subject.map(owner_stamp_for_subject).transpose()?,
            token_disabled,
        })
    }

    fn validate(&self) -> Result<(), String> {
        if let Some(stamp) = self.owner_stamp.as_deref() {
            if stamp.len() != REMOTE_BROWSER_UA_OWNER_STAMP_LENGTH
                || !stamp.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err("browser UA owner stamp is invalid".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteBrowserUaState {
    Prepared,
    Sent,
    Settled,
    Cancelled,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBrowserUaRecord {
    pub operation_id: String,
    pub request_id: String,
    pub account_fence: String,
    pub account_epoch: u64,
    pub device_id: String,
    pub generation: u64,
    pub projection: RemoteBrowserUaProjection,
    pub state: RemoteBrowserUaState,
    pub updated_at_ms: u64,
}

impl RemoteBrowserUaRecord {
    pub fn prepared(
        operation_id: impl Into<String>,
        request_id: impl Into<String>,
        account_fence: impl Into<String>,
        account_epoch: u64,
        device_id: impl Into<String>,
        generation: u64,
        projection: RemoteBrowserUaProjection,
        now_ms: u64,
    ) -> Result<Self, String> {
        let record = Self {
            operation_id: operation_id.into(),
            request_id: request_id.into(),
            account_fence: account_fence.into(),
            account_epoch,
            device_id: device_id.into(),
            generation,
            projection,
            state: RemoteBrowserUaState::Prepared,
            updated_at_ms: now_ms,
        };
        record.validate()?;
        Ok(record)
    }

    fn validate(&self) -> Result<(), String> {
        validate_identity("operation", &self.operation_id)?;
        validate_identity("request", &self.request_id)?;
        validate_identity("account fence", &self.account_fence)?;
        validate_identity("device", &self.device_id)?;
        if self.account_epoch == 0 || self.generation == 0 {
            return Err("browser UA epoch/generation must be positive".into());
        }
        self.projection.validate()
    }

    pub fn execution_request(&self) -> Result<ExecutionRequest, String> {
        if self.state != RemoteBrowserUaState::Prepared {
            return Err("browser UA reconcile can dispatch only prepared state".into());
        }
        let request = ExecutionRequest {
            operation_id: self.operation_id.clone(),
            capability_id: REMOTE_BROWSER_UA_CAPABILITY.into(),
            input_json: serde_json::json!({
                "version": 1,
                "generation": self.generation,
                "ownerStamp": self.projection.owner_stamp,
                "tokenDisabled": self.projection.token_disabled,
            })
            .to_string(),
            timeout_ms: REMOTE_BROWSER_UA_TIMEOUT_MS,
        };
        request
            .validate()
            .map_err(|error| format!("invalid browser UA execution request: {error:?}"))?;
        Ok(request)
    }
}

pub struct RemoteBrowserUaJournal {
    path: PathBuf,
    record: Option<RemoteBrowserUaRecord>,
}

impl RemoteBrowserUaJournal {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let path = path.into();
        let mut record = if path.exists() {
            Some(
                serde_json::from_slice::<RemoteBrowserUaRecord>(
                    &fs::read(&path).map_err(|error| format!("read browser UA journal: {error}"))?,
                )
                .map_err(|error| format!("invalid browser UA journal: {error}"))?,
            )
        } else {
            None
        };
        let mut changed = false;
        if let Some(existing) = record.as_mut() {
            existing.validate()?;
            match existing.state {
                RemoteBrowserUaState::Prepared => {
                    existing.state = RemoteBrowserUaState::Cancelled;
                    existing.updated_at_ms = now_ms;
                    changed = true;
                }
                RemoteBrowserUaState::Sent => {
                    existing.state = RemoteBrowserUaState::OutcomeUnknown;
                    existing.updated_at_ms = now_ms;
                    changed = true;
                }
                _ => {}
            }
        }
        let journal = Self { path, record };
        if changed {
            journal.persist()?;
        }
        Ok(journal)
    }

    pub fn replace_prepared(&mut self, next: RemoteBrowserUaRecord) -> Result<(), String> {
        next.validate()?;
        if next.state != RemoteBrowserUaState::Prepared {
            return Err("browser UA replacement must be prepared".into());
        }
        if let Some(current) = self.record.as_ref() {
            if matches!(current.state, RemoteBrowserUaState::Sent | RemoteBrowserUaState::OutcomeUnknown) {
                return Err("browser UA reconcile has ambiguous in-flight outcome".into());
            }
            if next.account_fence == current.account_fence
                && next.account_epoch == current.account_epoch
                && next.device_id == current.device_id
                && next.generation <= current.generation
            {
                return Err("browser UA generation must advance monotonically".into());
            }
        }
        self.record = Some(next);
        self.persist()
    }

    pub fn record(&self) -> Option<&RemoteBrowserUaRecord> {
        self.record.as_ref()
    }

    pub fn mark_sent(
        &mut self,
        operation_id: &str,
        account_fence: &str,
        account_epoch: u64,
        device_id: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        let record = self.checked_mut(operation_id, account_fence, account_epoch, device_id)?;
        if record.state != RemoteBrowserUaState::Prepared {
            return Err("browser UA dispatch requires prepared state".into());
        }
        record.state = RemoteBrowserUaState::Sent;
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn mark_outcome_unknown(
        &mut self,
        operation_id: &str,
        account_fence: &str,
        account_epoch: u64,
        device_id: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        let record = self.checked_mut(operation_id, account_fence, account_epoch, device_id)?;
        if record.state != RemoteBrowserUaState::Sent {
            return Err("only sent browser UA reconcile can become outcome-unknown".into());
        }
        record.state = RemoteBrowserUaState::OutcomeUnknown;
        record.updated_at_ms = now_ms;
        self.persist()
    }

    pub fn settle(
        &mut self,
        operation_id: &str,
        request_id: &str,
        account_fence: &str,
        account_epoch: u64,
        device_id: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        let record = self.checked_mut(operation_id, account_fence, account_epoch, device_id)?;
        if record.request_id != request_id {
            return Err("stale browser UA reconciliation response".into());
        }
        if !matches!(record.state, RemoteBrowserUaState::Sent | RemoteBrowserUaState::OutcomeUnknown) {
            return Err("browser UA settlement arrived from invalid state".into());
        }
        record.state = RemoteBrowserUaState::Settled;
        record.updated_at_ms = now_ms;
        self.persist()
    }

    fn checked_mut(
        &mut self,
        operation_id: &str,
        account_fence: &str,
        account_epoch: u64,
        device_id: &str,
    ) -> Result<&mut RemoteBrowserUaRecord, String> {
        let record = self.record.as_mut().ok_or("browser UA operation is unknown")?;
        if record.operation_id != operation_id
            || record.account_fence != account_fence
            || record.account_epoch != account_epoch
            || record.device_id != device_id
        {
            return Err("stale browser UA callback crossed account/device epoch".into());
        }
        Ok(record)
    }

    fn persist(&self) -> Result<(), String> {
        let Some(record) = self.record.as_ref() else { return Ok(()); };
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|error| format!("create browser UA journal parent: {error}"))?;
        let temp = self.path.with_extension(format!("tmp-{}", std::process::id()));
        let result = (|| -> Result<(), String> {
            let bytes = serde_json::to_vec(record)
                .map_err(|error| format!("encode browser UA journal: {error}"))?;
            let mut file = File::create(&temp)
                .map_err(|error| format!("create browser UA journal temp: {error}"))?;
            file.write_all(&bytes)
                .map_err(|error| format!("write browser UA journal temp: {error}"))?;
            file.sync_all()
                .map_err(|error| format!("sync browser UA journal temp: {error}"))?;
            fs::rename(&temp, &self.path)
                .map_err(|error| format!("replace browser UA journal: {error}"))?;
            Ok(())
        })();
        if result.is_err() { let _ = fs::remove_file(&temp); }
        result
    }
}

fn validate_identity(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > MAX_IDENTITY_BYTES || value.chars().any(char::is_control) {
        return Err(format!("browser UA {label} is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-remote-browser-ua-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()
        ))
    }

    fn prepared(generation: u64) -> RemoteBrowserUaRecord {
        RemoteBrowserUaRecord::prepared(
            format!("op-{generation}"),
            format!("req-{generation}"),
            "account-a:epoch-7",
            7,
            "device-a",
            generation,
            RemoteBrowserUaProjection::from_subject(Some("cursor-subject-a"), generation % 2 == 0).unwrap(),
            generation,
        ).unwrap()
    }

    #[test]
    fn owner_stamp_matches_desktop_subject_hash_contract_without_bearer() {
        let stamp = owner_stamp_for_subject("subject-a").unwrap();
        assert_eq!(stamp.len(), 16);
        assert!(stamp.bytes().all(|byte| byte.is_ascii_hexdigit()));
        let projection = RemoteBrowserUaProjection::from_subject(Some("subject-a"), true).unwrap();
        assert_eq!(projection.owner_stamp.as_deref(), Some(stamp.as_str()));
    }

    #[test]
    fn request_uses_dedicated_capability_and_contains_no_access_token() {
        let request = prepared(1).execution_request().unwrap();
        assert_eq!(request.capability_id, REMOTE_BROWSER_UA_CAPABILITY);
        assert!(request.input_json.contains("ownerStamp"));
        assert!(request.input_json.contains("tokenDisabled"));
        assert!(!request.input_json.to_ascii_lowercase().contains("access_token"));
        assert!(!request.input_json.contains("Bearer "));
    }

    #[test]
    fn process_reopen_never_blind_replays_sent_reconcile() {
        let journal_path = path("reopen");
        {
            let mut journal = RemoteBrowserUaJournal::open(&journal_path, 1).unwrap();
            journal.replace_prepared(prepared(1)).unwrap();
            journal.mark_sent("op-1", "account-a:epoch-7", 7, "device-a", 2).unwrap();
        }
        let reopened = RemoteBrowserUaJournal::open(&journal_path, 9).unwrap();
        assert_eq!(reopened.record().unwrap().state, RemoteBrowserUaState::OutcomeUnknown);
        let _ = fs::remove_file(journal_path);
    }

    #[test]
    fn in_flight_unknown_blocks_new_generation_until_reconciled() {
        let journal_path = path("generation");
        let mut journal = RemoteBrowserUaJournal::open(&journal_path, 1).unwrap();
        journal.replace_prepared(prepared(1)).unwrap();
        journal.mark_sent("op-1", "account-a:epoch-7", 7, "device-a", 2).unwrap();
        journal.mark_outcome_unknown("op-1", "account-a:epoch-7", 7, "device-a", 3).unwrap();
        assert!(journal.replace_prepared(prepared(2)).is_err());
        journal.settle("op-1", "req-1", "account-a:epoch-7", 7, "device-a", 4).unwrap();
        journal.replace_prepared(prepared(2)).unwrap();
        let _ = fs::remove_file(journal_path);
    }

    #[test]
    fn stale_account_or_device_callback_fails_closed() {
        let journal_path = path("fence");
        let mut journal = RemoteBrowserUaJournal::open(&journal_path, 1).unwrap();
        journal.replace_prepared(prepared(1)).unwrap();
        assert!(journal.mark_sent("op-1", "account-b:epoch-8", 8, "device-a", 2).is_err());
        assert!(journal.mark_sent("op-1", "account-a:epoch-7", 7, "device-b", 2).is_err());
        assert!(journal.mark_sent("op-1", "account-a:epoch-7", 7, "device-a", 2).is_ok());
        let _ = fs::remove_file(journal_path);
    }
}
