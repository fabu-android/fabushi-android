use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fs, io::Write, path::PathBuf};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HumanRemoteResourcePhase {
    Pending,
    Dispatching,
    Uploaded,
    Failed,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HumanRemoteResourceRecord {
    pub account_slot: Option<String>,
    pub account_fence: String,
    pub actor_id: String,
    pub client_request_id: String,
    pub blob_id: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub phase: HumanRemoteResourcePhase,
    pub generation: u64,
    pub resource: Option<Value>,
    pub reason: Option<String>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct HumanRemoteResourceState {
    generation: u64,
    records: BTreeMap<String, HumanRemoteResourceRecord>,
}

pub struct DurableHumanRemoteResourceOutboundOwner {
    path: PathBuf,
    state: HumanRemoteResourceState,
}

impl DurableHumanRemoteResourceOutboundOwner {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        let mut state = match fs::read(&path) {
            Ok(raw) => serde_json::from_slice::<HumanRemoteResourceState>(&raw)
                .map_err(|error| format!("invalid Human remote resource journal: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                HumanRemoteResourceState::default()
            }
            Err(error) => return Err(error.to_string()),
        };
        state.generation = state.generation.saturating_add(1).max(1);
        for record in state.records.values_mut() {
            record.generation = state.generation;
            if record.phase == HumanRemoteResourcePhase::Dispatching {
                record.phase = HumanRemoteResourcePhase::OutcomeUnknown;
                record.reason = Some(
                    "Host reopened while Human attachment upload outcome was unknown; automatic re-upload is fenced"
                        .into(),
                );
            }
        }
        let owner = Self { path, state };
        owner.persist()?;
        Ok(owner)
    }

    pub fn begin(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        actor_id: &str,
        client_request_id: &str,
        blob_id: &str,
        file_name: &str,
        size_bytes: u64,
    ) -> Result<HumanRemoteResourceRecord, String> {
        for (value, label) in [
            (account_fence, "account fence"),
            (actor_id, "actor id"),
            (client_request_id, "client request id"),
            (blob_id, "blob id"),
            (file_name, "file name"),
        ] {
            validate_text(value, label)?;
        }
        if size_bytes == 0 || size_bytes > 32 * 1024 * 1024 {
            return Err("Human remote resource size is invalid".into());
        }
        let key = scope_key(
            account_slot,
            account_fence,
            actor_id,
            client_request_id,
            blob_id,
        );
        if let Some(existing) = self.state.records.get(&key) {
            if existing.file_name == file_name && existing.size_bytes == size_bytes {
                return Ok(existing.clone());
            }
            return Err(
                "Human remote attachment clientRequestId/blobId was reused with different payload"
                    .into(),
            );
        }
        let record = HumanRemoteResourceRecord {
            account_slot: account_slot.map(str::to_string),
            account_fence: account_fence.to_string(),
            actor_id: actor_id.to_string(),
            client_request_id: client_request_id.to_string(),
            blob_id: blob_id.to_string(),
            file_name: file_name.to_string(),
            size_bytes,
            phase: HumanRemoteResourcePhase::Pending,
            generation: self.state.generation,
            resource: None,
            reason: None,
        };
        self.state.records.insert(key, record.clone());
        self.persist()?;
        Ok(record)
    }

    pub fn mark_dispatching(
        &mut self,
        source: &HumanRemoteResourceRecord,
    ) -> Result<HumanRemoteResourceRecord, String> {
        self.transition(
            source,
            HumanRemoteResourcePhase::Dispatching,
            &[HumanRemoteResourcePhase::Pending],
            None,
            None,
        )
    }

    pub fn mark_uploaded(
        &mut self,
        source: &HumanRemoteResourceRecord,
        resource: Value,
    ) -> Result<HumanRemoteResourceRecord, String> {
        validate_resource(&resource)?;
        self.transition(
            source,
            HumanRemoteResourcePhase::Uploaded,
            &[HumanRemoteResourcePhase::Dispatching],
            Some(resource),
            None,
        )
    }

    pub fn mark_failed(
        &mut self,
        source: &HumanRemoteResourceRecord,
        reason: impl Into<String>,
    ) -> Result<HumanRemoteResourceRecord, String> {
        self.transition(
            source,
            HumanRemoteResourcePhase::Failed,
            &[HumanRemoteResourcePhase::Pending],
            None,
            Some(reason.into()),
        )
    }

    pub fn mark_outcome_unknown(
        &mut self,
        source: &HumanRemoteResourceRecord,
        reason: impl Into<String>,
    ) -> Result<HumanRemoteResourceRecord, String> {
        self.transition(
            source,
            HumanRemoteResourcePhase::OutcomeUnknown,
            &[HumanRemoteResourcePhase::Dispatching],
            None,
            Some(reason.into()),
        )
    }

    pub fn record(
        &self,
        account_slot: Option<&str>,
        account_fence: &str,
        actor_id: &str,
        client_request_id: &str,
        blob_id: &str,
    ) -> Option<HumanRemoteResourceRecord> {
        self.state
            .records
            .get(&scope_key(
                account_slot,
                account_fence,
                actor_id,
                client_request_id,
                blob_id,
            ))
            .cloned()
    }

    pub fn clear_uploaded(
        &mut self,
        source: &HumanRemoteResourceRecord,
    ) -> Result<bool, String> {
        let key = scope_key(
            source.account_slot.as_deref(),
            &source.account_fence,
            &source.actor_id,
            &source.client_request_id,
            &source.blob_id,
        );
        let Some(current) = self.state.records.get(&key) else {
            return Ok(false);
        };
        if current.generation != self.state.generation
            || source.generation != current.generation
            || current.phase != HumanRemoteResourcePhase::Uploaded
        {
            return Ok(false);
        }
        self.state.records.remove(&key);
        self.persist()?;
        Ok(true)
    }

    fn transition(
        &mut self,
        source: &HumanRemoteResourceRecord,
        next: HumanRemoteResourcePhase,
        allowed: &[HumanRemoteResourcePhase],
        resource: Option<Value>,
        reason: Option<String>,
    ) -> Result<HumanRemoteResourceRecord, String> {
        let key = scope_key(
            source.account_slot.as_deref(),
            &source.account_fence,
            &source.actor_id,
            &source.client_request_id,
            &source.blob_id,
        );
        let record = self
            .state
            .records
            .get_mut(&key)
            .ok_or("Human remote resource record no longer exists")?;
        if source.generation == 0
            || source.generation != self.state.generation
            || record.generation != source.generation
        {
            return Err("stale Human remote resource callback fenced by generation".into());
        }
        if !allowed.contains(&record.phase) {
            return Err(format!(
                "Human remote resource transition from {:?} to {:?} is not allowed",
                record.phase, next
            ));
        }
        record.phase = next;
        if resource.is_some() {
            record.resource = resource;
        }
        record.reason = reason;
        let result = record.clone();
        self.persist()?;
        Ok(result)
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(&self.state)
            .map_err(|error| format!("failed to encode Human remote resource journal: {error}"))?;
        let tmp = self.path.with_extension(format!("tmp-{}", std::process::id()));
        let result = (|| -> Result<(), String> {
            let mut file = fs::File::create(&tmp).map_err(|error| error.to_string())?;
            file.write_all(&bytes).map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            fs::rename(&tmp, &self.path).map_err(|error| error.to_string())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }
}

fn validate_resource(resource: &Value) -> Result<(), String> {
    let object = resource
        .as_object()
        .ok_or("Human remote uploaded resource must be an object")?;
    let resource_id = object
        .get("id")
        .or_else(|| object.get("resourceId"))
        .and_then(Value::as_str)
        .filter(|value| {
            !value.trim().is_empty()
                && value.len() <= 128
                && !value.contains('/')
                && !value.contains('\\')
        })
        .ok_or("Human remote uploaded resource id is invalid")?;
    if resource_id.chars().any(char::is_control) {
        return Err("Human remote uploaded resource id is invalid".into());
    }
    let size = object
        .get("size")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0 && *value <= 32 * 1024 * 1024)
        .ok_or("Human remote uploaded resource size is invalid")?;
    let _ = size;
    Ok(())
}

fn scope_key(
    account_slot: Option<&str>,
    account_fence: &str,
    actor_id: &str,
    client_request_id: &str,
    blob_id: &str,
) -> String {
    format!(
        "{}\0{}\0{}\0{}\0{}",
        account_slot.unwrap_or("anonymous"),
        account_fence,
        actor_id,
        client_request_id,
        blob_id,
    )
}

fn validate_text(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.len() > 1024
        || value.chars().any(|ch| ch == '\0' || ch.is_control())
    {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reopen_fences_inflight_upload_as_outcome_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("human-resource-outbound.json");
        let old_generation = {
            let mut owner = DurableHumanRemoteResourceOutboundOwner::open(&path).unwrap();
            let pending = owner
                .begin(
                    Some("slot-a"),
                    "session:a",
                    "human:self",
                    "client-a",
                    "blob-a",
                    "a.txt",
                    5,
                )
                .unwrap();
            owner.mark_dispatching(&pending).unwrap().generation
        };

        let reopened = DurableHumanRemoteResourceOutboundOwner::open(&path).unwrap();
        let recovered = reopened
            .record(
                Some("slot-a"),
                "session:a",
                "human:self",
                "client-a",
                "blob-a",
            )
            .unwrap();
        assert!(recovered.generation > old_generation);
        assert_eq!(recovered.phase, HumanRemoteResourcePhase::OutcomeUnknown);
    }

    #[test]
    fn uploaded_resource_is_durable_and_payload_reuse_is_fenced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("human-resource-outbound.json");
        {
            let mut owner = DurableHumanRemoteResourceOutboundOwner::open(&path).unwrap();
            let pending = owner
                .begin(
                    None,
                    "session:a",
                    "human:self",
                    "client-a",
                    "blob-a",
                    "a.txt",
                    5,
                )
                .unwrap();
            let dispatching = owner.mark_dispatching(&pending).unwrap();
            let uploaded = owner
                .mark_uploaded(
                    &dispatching,
                    json!({"id":"resource-a","name":"a.txt","size":5}),
                )
                .unwrap();
            assert_eq!(uploaded.phase, HumanRemoteResourcePhase::Uploaded);
            assert!(owner
                .begin(
                    None,
                    "session:a",
                    "human:self",
                    "client-a",
                    "blob-a",
                    "different.txt",
                    5,
                )
                .is_err());
        }
        let reopened = DurableHumanRemoteResourceOutboundOwner::open(&path).unwrap();
        let restored = reopened
            .record(None, "session:a", "human:self", "client-a", "blob-a")
            .unwrap();
        assert_eq!(restored.phase, HumanRemoteResourcePhase::Uploaded);
        assert_eq!(restored.resource.unwrap()["id"], "resource-a");
    }

    #[test]
    fn stale_generation_cannot_settle_new_owner_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("human-resource-outbound.json");
        let stale = {
            let mut owner = DurableHumanRemoteResourceOutboundOwner::open(&path).unwrap();
            owner
                .begin(
                    None,
                    "session:a",
                    "human:self",
                    "client-a",
                    "blob-a",
                    "a.txt",
                    5,
                )
                .unwrap()
        };
        let mut reopened = DurableHumanRemoteResourceOutboundOwner::open(&path).unwrap();
        assert!(reopened.mark_dispatching(&stale).is_err());
    }
}
