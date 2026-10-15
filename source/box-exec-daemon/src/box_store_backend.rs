use crate::box_store_manifest_format::{
    BOX_STORE_BLOBS_PREFIX, BoxStoreManifest, BoxStoreManifestEntry, parse_box_store_manifest,
};
use crate::box_store_hydration::{
    apply_copy_in_download_owner, build_copy_in_watchdog_event,
    remove_hydration_handoff_marker, resolve_copy_in_download_owner,
    write_copy_in_status_throttled, write_hydration_handoff_marker,
    CopyInProgress, CopyInTraceStatus, BOX_STORE_HYDRATION_HANDOFF_FILE_NAME,
    COPY_IN_STUCK_THRESHOLD_MS, INCOMPLETE_LEGACY_HYDRATE_REASON,
};
use crate::box_store_workspace_ignore::{load_workspace_ignore, WorkspaceIgnore};
use crate::box_store_pack_restore::read_pack_blob;
use crate::server::{
    RemoteBackendCancelOutcome, RemoteBackendReconcileOutcome, RemoteBackendStartOutcome,
    RemoteExecutionBackend, RemoteRequestIdentity,
};
use fabushi_android_shared::{ExecutionError, ExecutionRequest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const BOX_STORE_SYNC_CAPABILITY: &str = "box.store.sync";
const MANIFEST_VERSION: u32 = 1;
const INPUT_VERSION: u32 = 1;
const MAX_STORE_ID_BYTES: usize = 512;
const MAX_MANIFEST_ENTRIES: usize = 100_000;
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 16 * 1024 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct RemoteBoxStoreBackendConfig {
    pub store_root: PathBuf,
    pub workspace_root: PathBuf,
    pub sand_data_root: PathBuf,
    pub cli_config_root: PathBuf,
    pub chrome_session_root: PathBuf,
    /// Optional Desktop/legacy BoxStore root used only to reconcile a never-seeded or
    /// incomplete frozen V2 primary during Android migration.
    pub legacy_store_root: Option<PathBuf>,
}

impl RemoteBoxStoreBackendConfig {
    pub fn validate(&self) -> Result<(), ExecutionError> {
        for (label, path) in [
            ("store root", &self.store_root),
            ("workspace root", &self.workspace_root),
            ("sand-data root", &self.sand_data_root),
            ("cli-config root", &self.cli_config_root),
            ("chrome-session root", &self.chrome_session_root),
        ] {
            if path.as_os_str().is_empty() || !path.is_absolute() {
                return Err(ExecutionError::InvalidRequest(format!(
                    "box-store {label} must be an absolute path"
                )));
            }
        }
        if let Some(path) = self.legacy_store_root.as_ref() {
            if path.as_os_str().is_empty() || !path.is_absolute() {
                return Err(ExecutionError::InvalidRequest(
                    "box-store legacy store root must be an absolute path".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Action {
    Hydrate,
    Snapshot,
    ChromeSessionStage,
    Vacuum,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    version: u32,
    action: Action,
    store_id: String,
    #[serde(default)]
    base_manifest_etag: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ManifestEntry {
    category: String,
    rel_path: String,
    object_sha256: String,
    size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    symlink_target: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    version: u32,
    generation: u64,
    last_operation_id: String,
    entries: Vec<ManifestEntry>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum OperationState {
    Pending,
    Completed,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationRecord {
    operation_id: String,
    request_id: String,
    account_fence: String,
    account_epoch: u64,
    device_id: String,
    request_fingerprint: String,
    state: OperationState,
    output_json: Option<String>,
    error: Option<String>,
}

#[derive(Debug)]
enum ActionError {
    SafeReject(String),
    OutcomeUnknown(String),
}

pub struct RemoteBoxStoreBackend {
    config: RemoteBoxStoreBackendConfig,
}

#[derive(Clone, Debug)]
struct FrozenManifestSnapshot {
    manifest: Option<BoxStoreManifest>,
    etag: Option<String>,
}

fn read_frozen_manifest_snapshot(path: &Path) -> Result<Option<FrozenManifestSnapshot>, ActionError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)
        .map_err(|error| ActionError::SafeReject(format!("read manifest: {error}")))?;
    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => {
            return Ok(Some(FrozenManifestSnapshot {
                manifest: None,
                etag: None,
            }))
        }
    };
    let manifest = parse_box_store_manifest(&value);
    Ok(Some(FrozenManifestSnapshot {
        manifest,
        etag: Some(sha256_bytes(&bytes)),
    }))
}

fn store_db_agent_ids(manifest: &BoxStoreManifest) -> BTreeSet<String> {
    const PREFIX: &str = "home/box/sand-data/agents/";
    manifest
        .entries
        .iter()
        .filter_map(|(path, entry)| {
            if !entry.is_file() {
                return None;
            }
            let rest = path.strip_prefix(PREFIX)?;
            let agent_id = rest.strip_suffix("/store.db")?;
            (!agent_id.is_empty() && !agent_id.contains('/')).then(|| agent_id.to_string())
        })
        .collect()
}

fn wall_clock_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn mark_primary_legacy_hydration_incomplete(path: &Path) -> Result<(), ActionError> {
    let existing = if path.exists() {
        let bytes = fs::read(path)
            .map_err(|error| ActionError::SafeReject(format!("read primary manifest: {error}")))?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| ActionError::SafeReject(format!("parse primary manifest: {error}")))?;
        parse_box_store_manifest(&value)
            .ok_or_else(|| ActionError::SafeReject("primary manifest is not frozen V2".into()))?;
        value
    } else {
        serde_json::json!({
            "version": 2,
            "updatedAtMs": 1,
            "fullyHydrated": false,
            "entries": {}
        })
    };
    let mut value = existing;
    let object = value
        .as_object_mut()
        .ok_or_else(|| ActionError::SafeReject("primary frozen manifest is not an object".into()))?;
    object.insert("fullyHydrated".into(), serde_json::Value::Bool(false));
    let encoded = serde_json::to_vec(&value)
        .map_err(|error| ActionError::SafeReject(format!("encode primary manifest: {error}")))?;
    atomic_write(path, &encoded)
        .map_err(|error| ActionError::OutcomeUnknown(format!("mark primary hydrate incomplete: {error}")))
}

impl RemoteBoxStoreBackend {
    pub fn open(config: RemoteBoxStoreBackendConfig) -> Result<Self, ExecutionError> {
        config.validate()?;
        fs::create_dir_all(&config.store_root).map_err(io_error)?;
        fs::create_dir_all(config.store_root.join(".runner-operations")).map_err(io_error)?;
        Ok(Self { config })
    }

    fn parse_request(&self, request: &ExecutionRequest) -> Result<Input, ExecutionError> {
        if request.capability_id != BOX_STORE_SYNC_CAPABILITY {
            return Err(ExecutionError::CapabilityUnavailable(
                request.capability_id.clone(),
            ));
        }
        let input: Input = serde_json::from_str(&request.input_json).map_err(|error| {
            ExecutionError::InvalidRequest(format!("invalid box-store sync input: {error}"))
        })?;
        if input.version != INPUT_VERSION {
            return Err(ExecutionError::InvalidRequest(
                "unsupported box-store sync input version".into(),
            ));
        }
        validate_store_id(&input.store_id)?;
        if let Some(etag) = input.base_manifest_etag.as_deref() {
            validate_etag(etag)?;
        }
        Ok(input)
    }

    fn request_fingerprint(
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> String {
        sha256_bytes(
            format!(
                "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
                identity.operation_id,
                identity.request_id,
                identity.account_fence,
                identity.account_epoch,
                identity.device_id,
                request.capability_id,
                request.timeout_ms,
                request.input_json
            )
            .as_bytes(),
        )
    }

    fn operation_path(&self, operation_id: &str) -> PathBuf {
        self.config
            .store_root
            .join(".runner-operations")
            .join(format!("{}.json", sha256_bytes(operation_id.as_bytes())))
    }

    fn load_operation(
        &self,
        operation_id: &str,
    ) -> Result<Option<OperationRecord>, ExecutionError> {
        let path = self.operation_path(operation_id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(path).map_err(io_error)?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| {
                ExecutionError::Transport(format!(
                    "box-store operation journal is corrupt: {error}"
                ))
            })
    }

    fn persist_operation(&self, record: &OperationRecord) -> Result<(), ExecutionError> {
        let bytes = serde_json::to_vec(record).map_err(|error| {
            ExecutionError::Transport(format!("encode box-store operation: {error}"))
        })?;
        atomic_write(&self.operation_path(&record.operation_id), &bytes).map_err(io_error)
    }

    fn require_operation_identity(
        record: &OperationRecord,
        identity: &RemoteRequestIdentity,
        fingerprint: &str,
    ) -> Result<(), ExecutionError> {
        if record.operation_id != identity.operation_id
            || record.request_id != identity.request_id
            || record.account_fence != identity.account_fence
            || record.account_epoch != identity.account_epoch
            || record.device_id != identity.device_id
            || record.request_fingerprint != fingerprint
        {
            return Err(ExecutionError::InvalidRequest(
                "box-store operation identity or request changed".into(),
            ));
        }
        Ok(())
    }

    fn store_dir(&self, store_id: &str) -> PathBuf {
        self.config.store_root.join(store_id)
    }

    fn manifest_path(&self, store_id: &str) -> PathBuf {
        self.store_dir(store_id).join("manifest.json")
    }

    fn objects_dir(&self, store_id: &str) -> PathBuf {
        self.store_dir(store_id).join(BOX_STORE_BLOBS_PREFIX)
    }

    fn load_manifest(
        &self,
        store_id: &str,
    ) -> Result<(Option<Manifest>, Option<String>), ActionError> {
        let path = self.manifest_path(store_id);
        if !path.exists() {
            return Ok((None, None));
        }
        let bytes = fs::read(&path)
            .map_err(|error| ActionError::SafeReject(format!("read manifest: {error}")))?;
        let manifest: Manifest = serde_json::from_slice(&bytes)
            .map_err(|error| ActionError::SafeReject(format!("invalid manifest: {error}")))?;
        if manifest.version != MANIFEST_VERSION {
            return Err(ActionError::SafeReject(format!(
                "unsupported manifest version {}",
                manifest.version
            )));
        }
        validate_manifest_entries(&manifest.entries)?;
        Ok((Some(manifest), Some(sha256_bytes(&bytes))))
    }

    fn check_base_etag(
        current: Option<&str>,
        requested: Option<&str>,
    ) -> Result<(), ActionError> {
        match (current, requested) {
            (None, None) => Ok(()),
            (Some(current), Some(requested)) if current == requested => Ok(()),
            (None, Some(_)) => Err(ActionError::SafeReject(
                "box-store manifest base ETag was supplied but no manifest exists".into(),
            )),
            (Some(_), None) => Err(ActionError::SafeReject(
                "box-store manifest already exists; baseManifestEtag is required".into(),
            )),
            (Some(_), Some(_)) => Err(ActionError::SafeReject(
                "box-store manifest CAS conflict".into(),
            )),
        }
    }

    fn execute_action(
        &self,
        identity: &RemoteRequestIdentity,
        input: &Input,
    ) -> Result<String, ActionError> {
        match input.action {
            Action::Snapshot => self.snapshot(identity, input, false),
            Action::ChromeSessionStage => self.snapshot(identity, input, true),
            Action::Hydrate => self.hydrate(identity, input),
            Action::Vacuum => self.vacuum(identity, input),
        }
    }

    fn snapshot(
        &self,
        identity: &RemoteRequestIdentity,
        input: &Input,
        chrome_only: bool,
    ) -> Result<String, ActionError> {
        let (current, current_etag) = self.load_manifest(&input.store_id)?;
        Self::check_base_etag(
            current_etag.as_deref(),
            input.base_manifest_etag.as_deref(),
        )?;

        let mut entries = if chrome_only {
            current
                .as_ref()
                .map(|value| {
                    value
                        .entries
                        .iter()
                        .filter(|entry| entry.category != "chrome_session")
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let categories = if chrome_only {
            vec![("chrome_session", self.config.chrome_session_root.as_path())]
        } else {
            vec![
                ("workspace", self.config.workspace_root.as_path()),
                ("sand_data", self.config.sand_data_root.as_path()),
                ("cli_config", self.config.cli_config_root.as_path()),
            ]
        };

        fs::create_dir_all(self.objects_dir(&input.store_id)).map_err(|error| {
            ActionError::SafeReject(format!("create object store: {error}"))
        })?;

        let mut total_bytes = entries.iter().map(|entry| entry.size_bytes).sum::<u64>();
        for (category, root) in categories {
            if !root.exists() {
                continue;
            }
            let mut captured = Vec::new();
            let workspace_ignore = (category == "workspace").then(|| load_workspace_ignore(root));
            collect_files(
                root,
                root,
                category,
                &mut captured,
                &mut total_bytes,
                workspace_ignore.as_ref(),
            )?;
            for mut entry in captured {
                if entry.symlink_target.is_some() {
                    entries.push(entry);
                    continue;
                }
                let source = root.join(rel_path_to_path(&entry.rel_path)?);
                let bytes = read_bounded_file(&source)?;
                let sha = sha256_bytes(&bytes);
                if sha != entry.object_sha256 {
                    return Err(ActionError::SafeReject(
                        "box-store source changed while snapshotting; retry from a fresh base".into(),
                    ));
                }
                let object_path = self.objects_dir(&input.store_id).join(&sha);
                if !object_path.exists() {
                    atomic_write(&object_path, &bytes).map_err(|error| {
                        ActionError::SafeReject(format!("write content-addressed object: {error}"))
                    })?;
                }
                entry.size_bytes = bytes.len() as u64;
                entries.push(entry);
            }
        }

        entries.sort_by(|a, b| {
            (&a.category, &a.rel_path).cmp(&(&b.category, &b.rel_path))
        });
        if entries.len() > MAX_MANIFEST_ENTRIES {
            return Err(ActionError::SafeReject(
                "box-store manifest exceeds entry limit".into(),
            ));
        }
        validate_manifest_entries(&entries)?;

        let manifest = Manifest {
            version: MANIFEST_VERSION,
            generation: current
                .as_ref()
                .map(|value| value.generation.saturating_add(1))
                .unwrap_or(1),
            last_operation_id: identity.operation_id.clone(),
            entries,
        };
        let bytes = serde_json::to_vec(&manifest)
            .map_err(|error| ActionError::SafeReject(format!("encode manifest: {error}")))?;
        let next_etag = sha256_bytes(&bytes);
        atomic_write(&self.manifest_path(&input.store_id), &bytes).map_err(|error| {
            ActionError::OutcomeUnknown(format!("commit manifest atomically: {error}"))
        })?;

        Ok(serde_json::json!({
            "storeId": input.store_id,
            "action": if chrome_only { "chrome_session_stage" } else { "snapshot" },
            "manifestEtag": next_etag,
            "generation": manifest.generation,
            "entries": manifest.entries.len(),
            "bytes": manifest.entries.iter().map(|entry| entry.size_bytes).sum::<u64>(),
        })
        .to_string())
    }

    fn frozen_destination<'a>(&'a self, rel_path: &str) -> Result<(&'a Path, PathBuf), ActionError> {
        const SAND_DATA: &str = "home/box/sand-data/";
        const CLI_CONFIG: &str = "home/box/cli-config/";
        const CHROME_SESSION: &str = "home/box/chrome-session/";
        if let Some(relative) = rel_path.strip_prefix("workspace/") {
            return Ok((&self.config.workspace_root, rel_path_to_path(relative)?));
        }
        if let Some(relative) = rel_path.strip_prefix(SAND_DATA) {
            return Ok((&self.config.sand_data_root, rel_path_to_path(relative)?));
        }
        if let Some(relative) = rel_path.strip_prefix(CLI_CONFIG) {
            return Ok((&self.config.cli_config_root, rel_path_to_path(relative)?));
        }
        if let Some(relative) = rel_path.strip_prefix(CHROME_SESSION) {
            return Ok((&self.config.chrome_session_root, rel_path_to_path(relative)?));
        }
        Err(ActionError::SafeReject(format!(
            "unsupported frozen BoxStore manifest path {rel_path}"
        )))
    }

    fn try_hydrate_frozen_manifest(
        &self,
        identity: &RemoteRequestIdentity,
        input: &Input,
    ) -> Result<Option<String>, ActionError> {
        let primary_dir = self.store_dir(&input.store_id);
        let primary_manifest_path = primary_dir.join("manifest.json");
        let primary = read_frozen_manifest_snapshot(&primary_manifest_path)?;

        let legacy_dir = self
            .config
            .legacy_store_root
            .as_ref()
            .map(|root| root.join(&input.store_id));
        let legacy = match legacy_dir.as_ref() {
            Some(dir) => read_frozen_manifest_snapshot(&dir.join("manifest.json"))?,
            None => None,
        };

        if primary.is_none() && legacy.is_none() {
            return Ok(None);
        }
        // A non-frozen manifest belongs to the native Android store format and must continue
        // through the existing hydrate implementation rather than being misclassified as V2.
        if primary.as_ref().is_some_and(|snapshot| snapshot.manifest.is_none())
            && legacy.as_ref().is_none_or(|snapshot| snapshot.manifest.is_none())
        {
            return Ok(None);
        }

        let handoff_marker = self
            .config
            .sand_data_root
            .join(BOX_STORE_HYDRATION_HANDOFF_FILE_NAME);

        let mut legacy_reconciliation = false;
        let mut download_from_legacy = false;
        let (manifest, source_dir, source_etag) = match primary.as_ref().and_then(|s| s.manifest.as_ref()) {
            Some(primary_manifest) if primary_manifest.fully_hydrated != Some(false) => {
                let etag = primary.as_ref().expect("primary snapshot").etag.clone();
                (primary_manifest.clone(), primary_dir.clone(), etag)
            }
            Some(primary_manifest) => {
                let legacy_manifest = legacy
                    .as_ref()
                    .and_then(|snapshot| snapshot.manifest.as_ref())
                    .ok_or_else(|| ActionError::SafeReject(format!(
                        "{INCOMPLETE_LEGACY_HYDRATE_REASON}: frozen V2 manifest requires legacy source reconciliation before restore"
                    )))?;
                legacy_reconciliation = true;
                let legacy_ids = store_db_agent_ids(legacy_manifest);
                if legacy_ids.is_empty() {
                    return Err(ActionError::SafeReject(format!(
                        "{INCOMPLETE_LEGACY_HYDRATE_REASON}: legacy source has no store.db identities to validate V2"
                    )));
                }
                let primary_ids = store_db_agent_ids(primary_manifest);
                if !legacy_ids.iter().all(|id| primary_ids.contains(id)) {
                    return Err(ActionError::SafeReject(format!(
                        "{INCOMPLETE_LEGACY_HYDRATE_REASON}: primary V2 store.db coverage is below the legacy source"
                    )));
                }
                let etag = primary.as_ref().expect("primary snapshot").etag.clone();
                (primary_manifest.clone(), primary_dir.clone(), etag)
            }
            None => {
                let legacy_manifest = legacy
                    .as_ref()
                    .and_then(|snapshot| snapshot.manifest.as_ref())
                    .filter(|manifest| !manifest.entries.is_empty())
                    .ok_or_else(|| ActionError::SafeReject(
                        "legacy source manifest is empty for unseeded primary".into(),
                    ))?;
                legacy_reconciliation = true;
                download_from_legacy = true;
                let legacy_snapshot = legacy.as_ref().expect("legacy snapshot");
                (
                    legacy_manifest.clone(),
                    legacy_dir.as_ref().expect("legacy dir").clone(),
                    legacy_snapshot.etag.clone(),
                )
            }
        };

        if let Some(requested) = input.base_manifest_etag.as_deref() {
            let comparison = if primary.is_some() {
                primary.as_ref().and_then(|snapshot| snapshot.etag.as_deref())
            } else {
                source_etag.as_deref()
            };
            if comparison != Some(requested) {
                return Err(ActionError::SafeReject(
                    "box-store hydrate manifest ETag changed".into(),
                ));
            }
        }

        if legacy_reconciliation {
            remove_hydration_handoff_marker(&handoff_marker).map_err(|error| {
                ActionError::SafeReject(format!("clear hydration handoff marker: {error}"))
            })?;
            mark_primary_legacy_hydration_incomplete(&primary_manifest_path)?;
        } else {
            remove_hydration_handoff_marker(&handoff_marker).map_err(|error| {
                ActionError::SafeReject(format!("clear hydration handoff marker: {error}"))
            })?;
        }

        let status_path = primary_dir.join("copy-in-status.json");
        let started_ms = wall_clock_now_ms();
        let mut last_status_write_ms = None;
        let mut last_watchdog_ms = started_ms;
        let mut watchdog_files = 0usize;
        let owner = self
            .config
            .sand_data_root
            .parent()
            .and_then(resolve_copy_in_download_owner);
        write_copy_in_status_throttled(
            &status_path,
            &serde_json::json!({
                "phase":"copying",
                "restored":0,
                "total":manifest.entries.len(),
                "bytes":0,
                "hydrateSource":legacy_reconciliation.then_some("legacy"),
            }),
            started_ms,
            &mut last_status_write_ms,
            true,
        )
        .map_err(|error| ActionError::OutcomeUnknown(format!(
            "write initial copy-in status: {error}"
        )))?;

        let objects = source_dir.join(BOX_STORE_BLOBS_PREFIX);
        let mut restored = 0usize;
        let mut restored_bytes = 0u64;
        let authoritative_store_db_ids = if legacy_reconciliation {
            legacy
                .as_ref()
                .and_then(|snapshot| snapshot.manifest.as_ref())
                .map(store_db_agent_ids)
                .unwrap_or_default()
        } else {
            store_db_agent_ids(&manifest)
        };

        for (rel_path, entry) in &manifest.entries {
            let (root, relative) = self.frozen_destination(rel_path)?;
            let destination = root.join(relative);
            ensure_restore_parent(root, &destination)?;
            match entry {
                BoxStoreManifestEntry::Symlink { target } => {
                    if !symlink_target_stays_within_root(root, &destination, Path::new(target)) {
                        return Err(ActionError::SafeReject(format!(
                            "box-store symlink target escapes category root for {rel_path}"
                        )));
                    }
                    restore_symlink(root, &destination, target).map_err(|error| {
                        ActionError::OutcomeUnknown(format!("hydrate symlink {rel_path}: {error}"))
                    })?;
                }
                BoxStoreManifestEntry::LegacyFile { sha, size }
                | BoxStoreManifestEntry::File { sha, size, .. } => {
                    if !is_sha256(sha) {
                        return Err(ActionError::SafeReject(format!(
                            "box-store object digest is invalid for {rel_path}"
                        )));
                    }
                    let object = objects.join(sha);
                    let payload = if object.exists() {
                        read_bounded_file(&object)?
                    } else {
                        read_pack_blob(&source_dir, sha, *size)
                            .map_err(|error| ActionError::SafeReject(format!(
                                "read frozen pack object for {rel_path}: {error}"
                            )))?
                            .ok_or_else(|| ActionError::SafeReject(format!(
                                "box-store object is missing for {rel_path}"
                            )))?
                    };
                    if payload.len() as u64 != *size || sha256_bytes(&payload) != *sha {
                        return Err(ActionError::SafeReject(format!(
                            "box-store object integrity mismatch for {rel_path}"
                        )));
                    }
                    atomic_write(&destination, &payload).map_err(|error| {
                        ActionError::OutcomeUnknown(format!("hydrate {rel_path}: {error}"))
                    })?;
                    #[cfg(unix)]
                    if let BoxStoreManifestEntry::File { mode, .. } = entry {
                        use std::os::unix::fs::PermissionsExt;
                        fs::set_permissions(&destination, fs::Permissions::from_mode(*mode))
                            .map_err(|error| ActionError::OutcomeUnknown(format!(
                                "set frozen manifest mode for {rel_path}: {error}"
                            )))?;
                    }
                    apply_copy_in_download_owner(&destination, owner).map_err(|error| {
                        ActionError::OutcomeUnknown(format!(
                            "apply frozen copy-in owner for {rel_path}: {error}"
                        ))
                    })?;
                    restored_bytes = restored_bytes.saturating_add(*size);
                }
            }
            restored += 1;

            let now_ms = wall_clock_now_ms();
            write_copy_in_status_throttled(
                &status_path,
                &serde_json::json!({
                    "phase":"copying",
                    "restored":restored,
                    "total":manifest.entries.len(),
                    "bytes":restored_bytes,
                    "hydrateSource":legacy_reconciliation.then_some("legacy"),
                }),
                now_ms,
                &mut last_status_write_ms,
                false,
            )
            .map_err(|error| ActionError::OutcomeUnknown(format!(
                "write copy-in progress status: {error}"
            )))?;

            if now_ms.saturating_sub(last_watchdog_ms) >= COPY_IN_STUCK_THRESHOLD_MS {
                let event = build_copy_in_watchdog_event(
                    restored > watchdog_files,
                    CopyInProgress {
                        total: manifest.entries.len(),
                        files: restored,
                        bytes: restored_bytes,
                    },
                    now_ms.saturating_sub(started_ms),
                    COPY_IN_STUCK_THRESHOLD_MS,
                    Some(&CopyInTraceStatus {
                        copy_stage: Some(match entry {
                            BoxStoreManifestEntry::Symlink { .. } => "symlink".into(),
                            _ => "manifest".into(),
                        }),
                        file_entries: Some(
                            manifest.entries.values().filter(|entry| entry.is_file()).count(),
                        ),
                        symlink_entries: Some(
                            manifest.entries.values().filter(|entry| entry.is_symlink()).count(),
                        ),
                        ..CopyInTraceStatus::default()
                    }),
                );
                eprintln!(
                    "[box-copy-in-watchdog] level={} metadata={}",
                    event.level,
                    serde_json::to_string(&event.metadata).unwrap_or_else(|_| "{}".into())
                );
                watchdog_files = restored;
                last_watchdog_ms = now_ms;
            }
        }

        let restored_store_db_entries = authoritative_store_db_ids
            .iter()
            .filter(|agent_id| {
                self.config
                    .sand_data_root
                    .join("agents")
                    .join(agent_id)
                    .join("store.db")
                    .is_file()
            })
            .count();
        if legacy_reconciliation && restored_store_db_entries < authoritative_store_db_ids.len() {
            return Err(ActionError::SafeReject(format!(
                "{INCOMPLETE_LEGACY_HYDRATE_REASON}: advertised_store_db={} restored_store_db={restored_store_db_entries}",
                authoritative_store_db_ids.len()
            )));
        }

        let marker = primary_dir
            .join("hydration")
            .join(format!("{}.json", sha256_bytes(identity.operation_id.as_bytes())));
        let marker_bytes = serde_json::to_vec(&serde_json::json!({
            "operationId": identity.operation_id,
            "manifestEtag": source_etag,
            "entries": restored,
            "frozenManifestVersion": manifest.version,
            "hydrateSource": legacy_reconciliation.then_some("legacy"),
            "storeDbEntries": authoritative_store_db_ids.len(),
            "restoredStoreDbEntries": restored_store_db_entries,
        }))
        .map_err(|error| ActionError::OutcomeUnknown(format!("encode hydration marker: {error}")))?;
        atomic_write(&marker, &marker_bytes).map_err(|error| {
            ActionError::OutcomeUnknown(format!("write hydration completion marker: {error}"))
        })?;

        if legacy_reconciliation {
            write_hydration_handoff_marker(&handoff_marker).map_err(|error| {
                ActionError::OutcomeUnknown(format!("write hydration handoff marker: {error}"))
            })?;
        }

        write_copy_in_status_throttled(
            &status_path,
            &serde_json::json!({
                "phase":"done",
                "restored":restored,
                "total":manifest.entries.len(),
                "bytes":restored_bytes,
                "outcome":"hydrated",
                "hydrateSource":legacy_reconciliation.then_some("legacy"),
                "storeDbEntries":authoritative_store_db_ids.len(),
                "restoredStoreDbEntries":restored_store_db_entries,
            }),
            wall_clock_now_ms(),
            &mut last_status_write_ms,
            true,
        )
        .map_err(|error| ActionError::OutcomeUnknown(format!(
            "write final copy-in status: {error}"
        )))?;

        Ok(Some(serde_json::json!({
            "storeId": input.store_id,
            "action": "hydrate",
            "manifestEtag": source_etag,
            "entries": restored,
            "bytes": restored_bytes,
            "frozenManifestVersion": manifest.version,
            "hydrateSource": legacy_reconciliation.then_some("legacy"),
            "downloadedFromLegacy": download_from_legacy,
            "storeDbEntries": authoritative_store_db_ids.len(),
            "restoredStoreDbEntries": restored_store_db_entries,
        }).to_string()))
    }

    fn hydrate(
        &self,
        identity: &RemoteRequestIdentity,
        input: &Input,
    ) -> Result<String, ActionError> {
        if let Some(output) = self.try_hydrate_frozen_manifest(identity, input)? {
            return Ok(output);
        }
        let (manifest, current_etag) = self.load_manifest(&input.store_id)?;
        let manifest = manifest.ok_or_else(|| {
            ActionError::SafeReject("box-store hydrate requires an existing manifest".into())
        })?;
        if let Some(requested) = input.base_manifest_etag.as_deref() {
            if current_etag.as_deref() != Some(requested) {
                return Err(ActionError::SafeReject(
                    "box-store hydrate manifest ETag changed".into(),
                ));
            }
        }

        let handoff_marker = self
            .config
            .sand_data_root
            .join(BOX_STORE_HYDRATION_HANDOFF_FILE_NAME);
        remove_hydration_handoff_marker(&handoff_marker).map_err(|error| {
            ActionError::SafeReject(format!("clear hydration handoff marker: {error}"))
        })?;

        let mut restored = 0usize;
        let mut restored_bytes = 0u64;
        for entry in &manifest.entries {
            let root = self.category_root(&entry.category)?;
            let relative = rel_path_to_path(&entry.rel_path)?;
            let destination = root.join(relative);
            ensure_restore_parent(root, &destination)?;
            if let Some(target) = entry.symlink_target.as_deref() {
                if !symlink_target_stays_within_root(root, &destination, Path::new(target)) {
                    return Err(ActionError::SafeReject(format!(
                        "box-store symlink target escapes category root for {}/{}",
                        entry.category, entry.rel_path
                    )));
                }
                restore_symlink(root, &destination, target).map_err(|error| {
                    ActionError::OutcomeUnknown(format!(
                        "hydrate symlink {}/{}: {error}",
                        entry.category, entry.rel_path
                    ))
                })?;
                restored += 1;
                continue;
            }
            let object = self.objects_dir(&input.store_id).join(&entry.object_sha256);
            let bytes = read_bounded_file(&object)?;
            if bytes.len() as u64 != entry.size_bytes || sha256_bytes(&bytes) != entry.object_sha256 {
                return Err(ActionError::SafeReject(format!(
                    "box-store object integrity mismatch for {}/{}",
                    entry.category, entry.rel_path
                )));
            }
            atomic_write(&destination, &bytes).map_err(|error| {
                ActionError::OutcomeUnknown(format!(
                    "hydrate {}/{}: {error}",
                    entry.category, entry.rel_path
                ))
            })?;
            restored += 1;
            restored_bytes = restored_bytes.saturating_add(entry.size_bytes);
        }

        write_hydration_handoff_marker(&handoff_marker).map_err(|error| {
            ActionError::OutcomeUnknown(format!("commit hydration handoff marker: {error}"))
        })?;

        let marker = self
            .store_dir(&input.store_id)
            .join("hydration")
            .join(format!("{}.json", sha256_bytes(identity.operation_id.as_bytes())));
        let marker_bytes = serde_json::to_vec(&serde_json::json!({
            "operationId": identity.operation_id,
            "manifestEtag": current_etag,
            "entries": restored,
        }))
        .map_err(|error| ActionError::OutcomeUnknown(format!("encode hydration marker: {error}")))?;
        atomic_write(&marker, &marker_bytes).map_err(|error| {
            ActionError::OutcomeUnknown(format!("write hydration completion marker: {error}"))
        })?;

        Ok(serde_json::json!({
            "storeId": input.store_id,
            "action": "hydrate",
            "manifestEtag": current_etag,
            "entries": restored,
            "bytes": restored_bytes,
        })
        .to_string())
    }

    fn vacuum(
        &self,
        _identity: &RemoteRequestIdentity,
        input: &Input,
    ) -> Result<String, ActionError> {
        let (manifest, current_etag) = self.load_manifest(&input.store_id)?;
        let manifest = manifest.ok_or_else(|| {
            ActionError::SafeReject("box-store vacuum requires an existing manifest".into())
        })?;
        Self::check_base_etag(
            current_etag.as_deref(),
            input.base_manifest_etag.as_deref(),
        )?;
        let referenced = manifest
            .entries
            .iter()
            .map(|entry| entry.object_sha256.as_str())
            .collect::<BTreeSet<_>>();
        let objects = self.objects_dir(&input.store_id);
        let mut removed = 0usize;
        if objects.exists() {
            for entry in fs::read_dir(&objects).map_err(|error| {
                ActionError::SafeReject(format!("read object store: {error}"))
            })? {
                let entry = entry.map_err(|error| {
                    ActionError::SafeReject(format!("read object-store entry: {error}"))
                })?;
                let file_type = entry.file_type().map_err(|error| {
                    ActionError::SafeReject(format!("stat object-store entry: {error}"))
                })?;
                if !file_type.is_file() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if !is_sha256(&name) {
                    continue;
                }
                if !referenced.contains(name.as_str()) {
                    fs::remove_file(entry.path()).map_err(|error| {
                        ActionError::OutcomeUnknown(format!("vacuum object {name}: {error}"))
                    })?;
                    removed += 1;
                }
            }
        }
        Ok(serde_json::json!({
            "storeId": input.store_id,
            "action": "vacuum",
            "manifestEtag": current_etag,
            "removedObjects": removed,
        })
        .to_string())
    }

    fn category_root(&self, category: &str) -> Result<&Path, ActionError> {
        match category {
            "workspace" => Ok(&self.config.workspace_root),
            "sand_data" => Ok(&self.config.sand_data_root),
            "cli_config" => Ok(&self.config.cli_config_root),
            "chrome_session" => Ok(&self.config.chrome_session_root),
            _ => Err(ActionError::SafeReject(format!(
                "unsupported box-store manifest category {category}"
            ))),
        }
    }
}

impl RemoteExecutionBackend for RemoteBoxStoreBackend {
    fn start(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendStartOutcome, ExecutionError> {
        let input = self.parse_request(request)?;
        let fingerprint = Self::request_fingerprint(identity, request);
        if let Some(existing) = self.load_operation(&identity.operation_id)? {
            Self::require_operation_identity(&existing, identity, &fingerprint)?;
            return Ok(match existing.state {
                OperationState::Completed => RemoteBackendStartOutcome::Completed {
                    output_json: existing.output_json.unwrap_or_else(|| "{}".into()),
                },
                OperationState::Rejected => RemoteBackendStartOutcome::Rejected {
                    reason: existing.error.unwrap_or_else(|| "box-store operation rejected".into()),
                },
                OperationState::Pending => RemoteBackendStartOutcome::OutcomeUnknown {
                    reason: "box-store operation has a durable pending side-effect boundary".into(),
                },
            });
        }

        let mut durable = OperationRecord {
            operation_id: identity.operation_id.clone(),
            request_id: identity.request_id.clone(),
            account_fence: identity.account_fence.clone(),
            account_epoch: identity.account_epoch,
            device_id: identity.device_id.clone(),
            request_fingerprint: fingerprint,
            state: OperationState::Pending,
            output_json: None,
            error: None,
        };
        self.persist_operation(&durable)?;

        match self.execute_action(identity, &input) {
            Ok(output_json) => {
                durable.state = OperationState::Completed;
                durable.output_json = Some(output_json.clone());
                self.persist_operation(&durable)?;
                Ok(RemoteBackendStartOutcome::Completed { output_json })
            }
            Err(ActionError::SafeReject(reason)) => {
                durable.state = OperationState::Rejected;
                durable.error = Some(reason.clone());
                self.persist_operation(&durable)?;
                Ok(RemoteBackendStartOutcome::Rejected { reason })
            }
            Err(ActionError::OutcomeUnknown(reason)) => {
                durable.error = Some(reason.clone());
                self.persist_operation(&durable)?;
                Ok(RemoteBackendStartOutcome::OutcomeUnknown { reason })
            }
        }
    }

    fn reconcile(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
        self.parse_request(request)?;
        let fingerprint = Self::request_fingerprint(identity, request);
        let Some(record) = self.load_operation(&identity.operation_id)? else {
            return Ok(RemoteBackendReconcileOutcome::Pending);
        };
        Self::require_operation_identity(&record, identity, &fingerprint)?;
        Ok(match record.state {
            OperationState::Completed => RemoteBackendReconcileOutcome::Completed {
                output_json: record.output_json.unwrap_or_else(|| "{}".into()),
            },
            OperationState::Rejected => RemoteBackendReconcileOutcome::Rejected {
                reason: record.error.unwrap_or_else(|| "box-store operation rejected".into()),
            },
            OperationState::Pending => RemoteBackendReconcileOutcome::Pending,
        })
    }

    fn cancel(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendCancelOutcome, ExecutionError> {
        self.parse_request(request)?;
        let fingerprint = Self::request_fingerprint(identity, request);
        let Some(record) = self.load_operation(&identity.operation_id)? else {
            return Ok(RemoteBackendCancelOutcome::Confirmed);
        };
        Self::require_operation_identity(&record, identity, &fingerprint)?;
        Ok(match record.state {
            OperationState::Rejected => RemoteBackendCancelOutcome::Confirmed,
            OperationState::Pending | OperationState::Completed => {
                RemoteBackendCancelOutcome::OutcomeUnknown {
                    reason: "box-store side effect crossed its durable execution boundary".into(),
                }
            }
        })
    }
}

fn collect_files(
    root: &Path,
    directory: &Path,
    category: &str,
    output: &mut Vec<ManifestEntry>,
    total_bytes: &mut u64,
    ignore: Option<&WorkspaceIgnore>,
) -> Result<(), ActionError> {
    let mut children = fs::read_dir(directory)
        .map_err(|error| ActionError::SafeReject(format!("read {}: {error}", directory.display())))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ActionError::SafeReject(format!("read directory entry: {error}")))?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        if output.len() >= MAX_MANIFEST_ENTRIES {
            return Err(ActionError::SafeReject(
                "box-store snapshot exceeds entry limit".into(),
            ));
        }
        let path = child.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| ActionError::SafeReject(format!("stat snapshot path: {error}")))?;
        let relative = path
            .strip_prefix(root)
            .map_err(|_| ActionError::SafeReject("snapshot path escaped category root".into()))?
            .to_path_buf();
        let rel_path = path_to_rel_string(&relative)?;
        if metadata.file_type().is_symlink() {
            if ignore.is_some_and(|rules| rules.ignores(&rel_path)) {
                continue;
            }
            let target = fs::read_link(&path)
                .map_err(|error| ActionError::SafeReject(format!("read snapshot symlink: {error}")))?;
            if !symlink_target_stays_within_root(root, &path, &target) {
                return Err(ActionError::SafeReject(format!(
                    "box-store symlink target escapes category root: {}",
                    path.display()
                )));
            }
            let target = target
                .to_str()
                .filter(|value| !value.is_empty() && value.len() <= 8 * 1024 && !value.contains('\0'))
                .ok_or_else(|| ActionError::SafeReject("box-store symlink target is invalid".into()))?
                .to_string();
            output.push(ManifestEntry {
                category: category.to_string(),
                rel_path,
                object_sha256: String::new(),
                size_bytes: 0,
                symlink_target: Some(target),
            });
            continue;
        }
        if metadata.is_dir() {
            if ignore.is_some_and(|rules| rules.can_prune_dir(&rel_path)) {
                continue;
            }
            collect_files(root, &path, category, output, total_bytes, ignore)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        if ignore.is_some_and(|rules| rules.ignores(&rel_path)) {
            continue;
        }
        if metadata.len() > MAX_FILE_BYTES {
            return Err(ActionError::SafeReject(format!(
                "box-store file exceeds per-file limit: {}",
                path.display()
            )));
        }
        *total_bytes = total_bytes.saturating_add(metadata.len());
        if *total_bytes > MAX_TOTAL_BYTES {
            return Err(ActionError::SafeReject(
                "box-store snapshot exceeds total byte limit".into(),
            ));
        }
        let bytes = read_bounded_file(&path)?;
        output.push(ManifestEntry {
            category: category.to_string(),
            rel_path,
            object_sha256: sha256_bytes(&bytes),
            size_bytes: bytes.len() as u64,
            symlink_target: None,
        });
    }
    Ok(())
}

fn read_bounded_file(path: &Path) -> Result<Vec<u8>, ActionError> {
    let metadata = fs::metadata(path)
        .map_err(|error| ActionError::SafeReject(format!("stat {}: {error}", path.display())))?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(ActionError::SafeReject(format!(
            "box-store file exceeds per-file limit: {}",
            path.display()
        )));
    }
    let mut file = File::open(path)
        .map_err(|error| ActionError::SafeReject(format!("open {}: {error}", path.display())))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ActionError::SafeReject(format!("read {}: {error}", path.display())))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(ActionError::SafeReject(
            "box-store file grew past per-file limit during read".into(),
        ));
    }
    Ok(bytes)
}

fn validate_manifest_entries(entries: &[ManifestEntry]) -> Result<(), ActionError> {
    if entries.len() > MAX_MANIFEST_ENTRIES {
        return Err(ActionError::SafeReject(
            "box-store manifest exceeds entry limit".into(),
        ));
    }
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for entry in entries {
        if !matches!(
            entry.category.as_str(),
            "workspace" | "sand_data" | "cli_config" | "chrome_session"
        ) {
            return Err(ActionError::SafeReject(
                "box-store manifest contains unknown category".into(),
            ));
        }
        rel_path_to_path(&entry.rel_path)?;
        if let Some(target) = entry.symlink_target.as_deref() {
            if target.is_empty()
                || target.len() > 8 * 1024
                || target.contains('\0')
                || !entry.object_sha256.is_empty()
                || entry.size_bytes != 0
            {
                return Err(ActionError::SafeReject(
                    "box-store manifest symlink entry is invalid".into(),
                ));
            }
        } else {
            if !is_sha256(&entry.object_sha256) {
                return Err(ActionError::SafeReject(
                    "box-store manifest object digest is invalid".into(),
                ));
            }
            if entry.size_bytes > MAX_FILE_BYTES {
                return Err(ActionError::SafeReject(
                    "box-store manifest file exceeds per-file limit".into(),
                ));
            }
            total = total.saturating_add(entry.size_bytes);
            if total > MAX_TOTAL_BYTES {
                return Err(ActionError::SafeReject(
                    "box-store manifest exceeds total byte limit".into(),
                ));
            }
        }
        if !seen.insert((entry.category.as_str(), entry.rel_path.as_str())) {
            return Err(ActionError::SafeReject(
                "box-store manifest contains duplicate path".into(),
            ));
        }
    }
    Ok(())
}

fn validate_store_id(store_id: &str) -> Result<(), ExecutionError> {
    if store_id.is_empty()
        || store_id.len() > MAX_STORE_ID_BYTES
        || store_id == "."
        || store_id == ".."
        || store_id.chars().any(|character| {
            character.is_control()
                || character == '/'
                || character == '\\'
                || character.is_whitespace()
        })
    {
        return Err(ExecutionError::InvalidRequest(
            "box-store store id is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_etag(etag: &str) -> Result<(), ExecutionError> {
    if !is_sha256(etag) {
        return Err(ExecutionError::InvalidRequest(
            "box-store manifest ETag must be a lowercase SHA-256 digest".into(),
        ));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn rel_path_to_path(value: &str) -> Result<PathBuf, ActionError> {
    if value.is_empty() || value.len() > 8 * 1024 {
        return Err(ActionError::SafeReject(
            "box-store relative path is invalid".into(),
        ));
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ActionError::SafeReject(
            "box-store relative path escapes category root".into(),
        ));
    }
    Ok(path.to_path_buf())
}

fn path_to_rel_string(path: &Path) -> Result<String, ActionError> {
    if path.as_os_str().is_empty()
        || path.components().any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ActionError::SafeReject(
            "box-store relative path is invalid".into(),
        ));
    }
    let value = path
        .to_str()
        .ok_or_else(|| ActionError::SafeReject("box-store path is not UTF-8".into()))?
        .replace('\\', "/");
    rel_path_to_path(&value)?;
    Ok(value)
}

fn ensure_within(root: &Path, destination: &Path) -> Result<(), ActionError> {
    if !destination.starts_with(root) {
        return Err(ActionError::SafeReject(
            "box-store destination escaped category root".into(),
        ));
    }
    Ok(())
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn symlink_target_stays_within_root(root: &Path, symlink_path: &Path, target: &Path) -> bool {
    let root = lexical_normalize(root);
    let candidate = if target.is_absolute() {
        lexical_normalize(target)
    } else {
        let Some(parent) = symlink_path.parent() else {
            return false;
        };
        lexical_normalize(&parent.join(target))
    };
    candidate == root || candidate.starts_with(&root)
}

fn ensure_restore_parent(root: &Path, destination: &Path) -> Result<(), ActionError> {
    ensure_within(root, destination)?;
    fs::create_dir_all(root)
        .map_err(|error| ActionError::SafeReject(format!("create category root: {error}")))?;
    let root_metadata = fs::symlink_metadata(root)
        .map_err(|error| ActionError::SafeReject(format!("stat category root: {error}")))?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(ActionError::SafeReject(
            "box-store category root is not a real directory".into(),
        ));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| ActionError::SafeReject("box-store destination has no parent".into()))?;
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| ActionError::SafeReject("box-store destination escaped category root".into()))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(ActionError::SafeReject(
                "box-store restore parent is invalid".into(),
            ));
        }
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ActionError::SafeReject(format!(
                    "box-store restore parent contains symlink: {}",
                    current.display()
                )));
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(ActionError::SafeReject(format!(
                    "box-store restore parent is not a directory: {}",
                    current.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|error| {
                    ActionError::SafeReject(format!(
                        "create restore parent {}: {error}",
                        current.display()
                    ))
                })?;
            }
            Err(error) => {
                return Err(ActionError::SafeReject(format!(
                    "stat restore parent {}: {error}",
                    current.display()
                )));
            }
        }
    }
    Ok(())
}

fn restore_symlink(root: &Path, destination: &Path, target: &str) -> Result<(), String> {
    let target_path = Path::new(target);
    if !symlink_target_stays_within_root(root, destination, target_path) {
        return Err("symlink target escapes category root".into());
    }
    match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(destination).map_err(|error| error.to_string())?;
        }
        Ok(_) => {
            fs::remove_file(destination).map_err(|error| error.to_string())?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    let temp = destination.with_extension(format!(
        "symlink-tmp-{}-{}",
        std::process::id(),
        &sha256_bytes(format!("{}:{}", destination.display(), target).as_bytes())[..12]
    ));
    let _ = fs::remove_file(&temp);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target_path, &temp).map_err(|error| error.to_string())?;
    }
    #[cfg(not(unix))]
    {
        let _ = target_path;
        return Err("box-store symlink restore is unsupported on this runner platform".into());
    }
    let result = fs::rename(&temp, destination).map_err(|error| error.to_string());
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut file = File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn io_error(error: std::io::Error) -> ExecutionError {
    ExecutionError::Transport(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::box_store_workspace_ignore::SAND_WORKSPACE_IGNORE_FILE_NAME;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-box-store-backend-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn config(label: &str) -> RemoteBoxStoreBackendConfig {
        let root = root(label);
        RemoteBoxStoreBackendConfig {
            store_root: root.join("store"),
            workspace_root: root.join("box/workspace"),
            sand_data_root: root.join("box/home/box/sand-data"),
            cli_config_root: root.join("box/home/box/cli-config"),
            chrome_session_root: root.join("box/home/box/chrome-session"),
            legacy_store_root: None,
        }
    }

    fn identity(operation: &str, request: &str) -> RemoteRequestIdentity {
        RemoteRequestIdentity {
            credential_id: "credential-a".into(),
            operation_id: operation.into(),
            request_id: request.into(),
            account_fence: "account-a:epoch-7".into(),
            account_epoch: 7,
            permission_grant_id: "grant-a".into(),
            device_id: "device-a".into(),
        }
    }

    fn request(operation: &str, action: &str, base: Option<&str>) -> ExecutionRequest {
        ExecutionRequest {
            operation_id: operation.into(),
            capability_id: BOX_STORE_SYNC_CAPABILITY.into(),
            input_json: serde_json::json!({
                "version": 1,
                "action": action,
                "storeId": "store-a",
                "baseManifestEtag": base,
            })
            .to_string(),
            timeout_ms: 120_000,
        }
    }

    fn completed(outcome: RemoteBackendStartOutcome) -> String {
        match outcome {
            RemoteBackendStartOutcome::Completed { output_json } => output_json,
            other => panic!("expected completed outcome, got {other:?}"),
        }
    }

    #[test]
    fn snapshot_hydrate_and_cas_are_dedicated_and_conflict_safe() {
        let config = config("roundtrip");
        fs::create_dir_all(&config.workspace_root).unwrap();
        fs::write(config.workspace_root.join("note.txt"), b"one").unwrap();
        let root = config.store_root.parent().unwrap().to_path_buf();
        let mut backend = RemoteBoxStoreBackend::open(config.clone()).unwrap();

        let first = completed(
            backend
                .start(&identity("op-1", "req-1"), &request("op-1", "snapshot", None))
                .unwrap(),
        );
        let first_etag = serde_json::from_str::<serde_json::Value>(&first).unwrap()
            ["manifestEtag"]
            .as_str()
            .unwrap()
            .to_string();

        fs::write(config.workspace_root.join("note.txt"), b"two").unwrap();
        let second = completed(
            backend
                .start(
                    &identity("op-2", "req-2"),
                    &request("op-2", "snapshot", Some(&first_etag)),
                )
                .unwrap(),
        );
        let second_etag = serde_json::from_str::<serde_json::Value>(&second).unwrap()
            ["manifestEtag"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(first_etag, second_etag);

        let stale = backend
            .start(
                &identity("op-3", "req-3"),
                &request("op-3", "snapshot", Some(&first_etag)),
            )
            .unwrap();
        assert!(matches!(stale, RemoteBackendStartOutcome::Rejected { .. }));

        fs::remove_file(config.workspace_root.join("note.txt")).unwrap();
        completed(
            backend
                .start(
                    &identity("op-4", "req-4"),
                    &request("op-4", "hydrate", Some(&second_etag)),
                )
                .unwrap(),
        );
        assert_eq!(fs::read(config.workspace_root.join("note.txt")).unwrap(), b"two");
        assert_eq!(
            fs::read_to_string(
                config
                    .sand_data_root
                    .join(BOX_STORE_HYDRATION_HANDOFF_FILE_NAME)
            )
            .unwrap(),
            "complete\n"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn snapshot_and_hydrate_preserve_safe_relative_symlink() {
        let config = config("symlink-roundtrip");
        fs::create_dir_all(&config.workspace_root).unwrap();
        fs::write(config.workspace_root.join("note.txt"), b"one").unwrap();
        std::os::unix::fs::symlink("note.txt", config.workspace_root.join("alias.txt")).unwrap();
        let mut backend = RemoteBoxStoreBackend::open(config.clone()).unwrap();

        let first = completed(
            backend
                .start(
                    &identity("op-sym-1", "req-sym-1"),
                    &request("op-sym-1", "snapshot", None),
                )
                .unwrap(),
        );
        let etag = serde_json::from_str::<serde_json::Value>(&first).unwrap()["manifestEtag"]
            .as_str()
            .unwrap()
            .to_string();

        fs::remove_file(config.workspace_root.join("alias.txt")).unwrap();
        completed(
            backend
                .start(
                    &identity("op-sym-2", "req-sym-2"),
                    &request("op-sym-2", "hydrate", Some(&etag)),
                )
                .unwrap(),
        );
        let alias = config.workspace_root.join("alias.txt");
        assert!(fs::symlink_metadata(&alias).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_link(alias).unwrap(), PathBuf::from("note.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn hydrate_rejects_existing_parent_symlink_escape() {
        let config = config("parent-symlink");
        let nested = config.workspace_root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("note.txt"), b"inside").unwrap();
        let outside = config.store_root.parent().unwrap().join("outside");
        fs::create_dir_all(&outside).unwrap();
        let mut backend = RemoteBoxStoreBackend::open(config.clone()).unwrap();

        let first = completed(
            backend
                .start(
                    &identity("op-parent-1", "req-parent-1"),
                    &request("op-parent-1", "snapshot", None),
                )
                .unwrap(),
        );
        let etag = serde_json::from_str::<serde_json::Value>(&first).unwrap()["manifestEtag"]
            .as_str()
            .unwrap()
            .to_string();

        fs::remove_dir_all(&nested).unwrap();
        std::os::unix::fs::symlink(&outside, &nested).unwrap();
        let outcome = backend
            .start(
                &identity("op-parent-2", "req-parent-2"),
                &request("op-parent-2", "hydrate", Some(&etag)),
            )
            .unwrap();
        assert!(matches!(outcome, RemoteBackendStartOutcome::Rejected { .. }));
        assert!(!outside.join("note.txt").exists());
    }

    #[test]
    fn snapshot_applies_desktop_workspace_ignore_and_negation_semantics() {
        let config = config("workspace-ignore");
        fs::create_dir_all(config.workspace_root.join("node_modules/pkg")).unwrap();
        fs::create_dir_all(config.workspace_root.join("target/debug")).unwrap();
        fs::create_dir_all(config.workspace_root.join("generated")).unwrap();
        fs::write(
            config.workspace_root.join(SAND_WORKSPACE_IGNORE_FILE_NAME),
            "generated/\n!generated/keep.txt\n",
        )
        .unwrap();
        fs::write(config.workspace_root.join("node_modules/pkg/index.js"), b"ignored").unwrap();
        fs::write(config.workspace_root.join("target/debug/app"), b"ignored").unwrap();
        fs::write(config.workspace_root.join("generated/drop.txt"), b"ignored").unwrap();
        fs::write(config.workspace_root.join("generated/keep.txt"), b"kept").unwrap();
        fs::write(config.workspace_root.join("main.rs"), b"kept").unwrap();

        let root = config.store_root.parent().unwrap().to_path_buf();
        let mut backend = RemoteBoxStoreBackend::open(config.clone()).unwrap();
        completed(
            backend
                .start(&identity("op-ignore", "req-ignore"), &request("op-ignore", "snapshot", None))
                .unwrap(),
        );
        let (manifest, _) = backend.load_manifest("store-a").unwrap();
        let paths = manifest
            .unwrap()
            .entries
            .into_iter()
            .filter(|entry| entry.category == "workspace")
            .map(|entry| entry.rel_path)
            .collect::<BTreeSet<_>>();

        assert!(paths.contains("main.rs"));
        assert!(paths.contains("generated/keep.txt"));
        assert!(paths.contains(SAND_WORKSPACE_IGNORE_FILE_NAME));
        assert!(!paths.contains("generated/drop.txt"));
        assert!(!paths.iter().any(|path| path.starts_with("node_modules/")));
        assert!(!paths.iter().any(|path| path.starts_with("target/")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_execute_returns_durable_result_without_replaying_source_changes() {
        let config = config("duplicate");
        fs::create_dir_all(&config.workspace_root).unwrap();
        fs::write(config.workspace_root.join("note.txt"), b"one").unwrap();
        let root = config.store_root.parent().unwrap().to_path_buf();
        let mut backend = RemoteBoxStoreBackend::open(config.clone()).unwrap();
        let identity = identity("op-1", "req-1");
        let request = request("op-1", "snapshot", None);
        let first = completed(backend.start(&identity, &request).unwrap());
        fs::write(config.workspace_root.join("note.txt"), b"changed-after-completion").unwrap();
        let second = completed(backend.start(&identity, &request).unwrap());
        assert_eq!(first, second);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pending_side_effect_is_never_blind_replayed_after_reopen() {
        let config = config("pending");
        let root = config.store_root.parent().unwrap().to_path_buf();
        let backend = RemoteBoxStoreBackend::open(config.clone()).unwrap();
        let identity = identity("op-1", "req-1");
        let request = request("op-1", "snapshot", None);
        let fingerprint = RemoteBoxStoreBackend::request_fingerprint(&identity, &request);
        backend
            .persist_operation(&OperationRecord {
                operation_id: identity.operation_id.clone(),
                request_id: identity.request_id.clone(),
                account_fence: identity.account_fence.clone(),
                account_epoch: identity.account_epoch,
                device_id: identity.device_id.clone(),
                request_fingerprint: fingerprint,
                state: OperationState::Pending,
                output_json: None,
                error: None,
            })
            .unwrap();

        let mut reopened = RemoteBoxStoreBackend::open(config).unwrap();
        assert!(matches!(
            reopened.start(&identity, &request).unwrap(),
            RemoteBackendStartOutcome::OutcomeUnknown { .. }
        ));
        assert!(matches!(
            reopened.reconcile(&identity, &request).unwrap(),
            RemoteBackendReconcileOutcome::Pending
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn store_id_and_manifest_paths_fail_closed() {
        let config = config("validation");
        let root = config.store_root.parent().unwrap().to_path_buf();
        let mut backend = RemoteBoxStoreBackend::open(config).unwrap();
        let bad = ExecutionRequest {
            operation_id: "op-1".into(),
            capability_id: BOX_STORE_SYNC_CAPABILITY.into(),
            input_json: r#"{"version":1,"action":"snapshot","storeId":"../escape"}"#.into(),
            timeout_ms: 120_000,
        };
        assert!(backend.start(&identity("op-1", "req-1"), &bad).is_err());
        assert!(rel_path_to_path("../escape").is_err());
        assert!(rel_path_to_path("/absolute").is_err());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn hydrate_accepts_frozen_desktop_v2_manifest_on_shipping_backend() {
        let config = config("frozen-v2");
        let store = config.store_root.join("store-a");
        fs::create_dir_all(store.join("blobs")).unwrap();
        let payload = b"desktop-v2-copy-in\n";
        let digest = sha256_bytes(payload);
        fs::write(store.join("blobs").join(&digest), payload).unwrap();
        fs::write(
            store.join("manifest.json"),
            serde_json::to_vec(&serde_json::json!({
                "version": 2,
                "updatedAtMs": 1,
                "writerWindowId": "desktop-copy-in",
                "fullyHydrated": true,
                "entries": {
                    "workspace/run.sh": {
                        "kind": "file", "sha": digest, "size": payload.len(), "mode": 384
                    },
                    "workspace/current": {
                        "kind": "symlink", "target": "run.sh"
                    }
                }
            })).unwrap(),
        ).unwrap();
        let mut backend = RemoteBoxStoreBackend::open(config.clone()).unwrap();
        let output = completed(backend.start(
            &identity("frozen-hydrate", "frozen-request"),
            &request("frozen-hydrate", "hydrate", None),
        ).unwrap());
        assert!(output.contains("\"frozenManifestVersion\":2"));
        assert_eq!(fs::read(config.workspace_root.join("run.sh")).unwrap(), payload);
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(config.workspace_root.join("run.sh")).unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(fs::read_link(config.workspace_root.join("current")).unwrap(), PathBuf::from("run.sh"));
        }
    }

    fn write_frozen_file_entry(
        store_root: &Path,
        store_id: &str,
        rel_path: &str,
        payload: &[u8],
        fully_hydrated: bool,
    ) {
        let store = store_root.join(store_id);
        fs::create_dir_all(store.join("blobs")).unwrap();
        let digest = sha256_bytes(payload);
        fs::write(store.join("blobs").join(&digest), payload).unwrap();
        fs::write(
            store.join("manifest.json"),
            serde_json::to_vec(&serde_json::json!({
                "version":2,
                "updatedAtMs":1,
                "writerWindowId":"legacy-contract",
                "fullyHydrated":fully_hydrated,
                "entries":{
                    rel_path:{
                        "kind":"file",
                        "sha":digest,
                        "size":payload.len(),
                        "mode":384
                    }
                }
            })).unwrap(),
        ).unwrap();
    }

    #[test]
    fn unseeded_primary_hydrates_from_legacy_and_persists_incomplete_primary_handoff() {
        let mut config = config("legacy-unseeded");
        let legacy_root = config.store_root.parent().unwrap().join("legacy-store");
        config.legacy_store_root = Some(legacy_root.clone());
        let payload = b"legacy-agent-store\n";
        write_frozen_file_entry(
            &legacy_root,
            "store-a",
            "home/box/sand-data/agents/agent-a/store.db",
            payload,
            true,
        );
        let marker = config
            .sand_data_root
            .join(BOX_STORE_HYDRATION_HANDOFF_FILE_NAME);
        fs::create_dir_all(&config.sand_data_root).unwrap();
        fs::write(&marker, b"stale-complete\n").unwrap();

        let mut backend = RemoteBoxStoreBackend::open(config.clone()).unwrap();
        let output = completed(
            backend
                .start(
                    &identity("legacy-unseeded", "legacy-unseeded-request"),
                    &request("legacy-unseeded", "hydrate", None),
                )
                .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["hydrateSource"], "legacy");
        assert_eq!(parsed["downloadedFromLegacy"], true);
        assert_eq!(parsed["storeDbEntries"], 1);
        assert_eq!(parsed["restoredStoreDbEntries"], 1);
        assert_eq!(
            fs::read(config.sand_data_root.join("agents/agent-a/store.db")).unwrap(),
            payload
        );
        let primary: serde_json::Value = serde_json::from_slice(
            &fs::read(config.store_root.join("store-a/manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(primary["fullyHydrated"], false);
        assert_eq!(primary["entries"].as_object().unwrap().len(), 0);
        assert_eq!(fs::read_to_string(marker).unwrap(), "complete\n");
    }

    #[test]
    fn incomplete_primary_requires_legacy_store_db_identity_coverage() {
        let mut config = config("legacy-coverage");
        let legacy_root = config.store_root.parent().unwrap().join("legacy-store");
        config.legacy_store_root = Some(legacy_root.clone());

        write_frozen_file_entry(
            &config.store_root,
            "store-a",
            "home/box/sand-data/agents/agent-a/store.db",
            b"a",
            false,
        );
        let legacy_store = legacy_root.join("store-a");
        fs::create_dir_all(legacy_store.join("blobs")).unwrap();
        let digest_a = sha256_bytes(b"a");
        let digest_b = sha256_bytes(b"b");
        fs::write(legacy_store.join("blobs").join(&digest_a), b"a").unwrap();
        fs::write(legacy_store.join("blobs").join(&digest_b), b"b").unwrap();
        fs::write(
            legacy_store.join("manifest.json"),
            serde_json::to_vec(&serde_json::json!({
                "version":2,
                "updatedAtMs":1,
                "fullyHydrated":true,
                "entries":{
                    "home/box/sand-data/agents/agent-a/store.db":{
                        "kind":"file","sha":digest_a,"size":1,"mode":384
                    },
                    "home/box/sand-data/agents/agent-b/store.db":{
                        "kind":"file","sha":digest_b,"size":1,"mode":384
                    }
                }
            })).unwrap(),
        ).unwrap();

        let mut backend = RemoteBoxStoreBackend::open(config.clone()).unwrap();
        let outcome = backend
            .start(
                &identity("legacy-coverage", "legacy-coverage-request"),
                &request("legacy-coverage", "hydrate", None),
            )
            .unwrap();
        match outcome {
            RemoteBackendStartOutcome::Rejected { reason } => {
                assert!(reason.contains("primary V2 store.db coverage is below"));
            }
            other => panic!("expected coverage rejection, got {other:?}"),
        }
        assert!(
            !config
                .sand_data_root
                .join(BOX_STORE_HYDRATION_HANDOFF_FILE_NAME)
                .exists()
        );
    }

    #[test]
    fn incomplete_primary_fails_when_legacy_has_no_store_db_identity() {
        let mut config = config("legacy-no-store-db");
        let legacy_root = config.store_root.parent().unwrap().join("legacy-store");
        config.legacy_store_root = Some(legacy_root.clone());

        write_frozen_file_entry(
            &config.store_root,
            "store-a",
            "home/box/sand-data/agents/agent-a/store.db",
            b"a",
            false,
        );
        write_frozen_file_entry(
            &legacy_root,
            "store-a",
            "workspace/readme.txt",
            b"legacy",
            true,
        );

        let mut backend = RemoteBoxStoreBackend::open(config.clone()).unwrap();
        let outcome = backend
            .start(
                &identity("legacy-no-db", "legacy-no-db-request"),
                &request("legacy-no-db", "hydrate", None),
            )
            .unwrap();
        match outcome {
            RemoteBackendStartOutcome::Rejected { reason } => {
                assert!(reason.contains("no store.db identities to validate V2"));
            }
            other => panic!("expected no-store-db rejection, got {other:?}"),
        }
    }

    #[test]
    fn frozen_v2_hydrate_restores_pack_only_member_on_shipping_backend() {
        use flate2::{Compression, write::GzEncoder};
        use std::io::Write as _;
        let config = config("frozen-pack-only");
        let store = config.store_root.join("store-a");
        fs::create_dir_all(store.join("packs")).unwrap();
        let payload = b"packed-desktop-member\n";
        let digest = sha256_bytes(payload);
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(payload).unwrap();
        let compressed = encoder.finish().unwrap();
        let pack_id = sha256_bytes(&compressed);
        fs::write(store.join("packs").join(&pack_id), &compressed).unwrap();
        fs::write(store.join("packs/index.json"), serde_json::to_vec(&serde_json::json!({
            "version": 1, "maxVmtime": 1,
            "packs": [{ "id": pack_id, "bytes": compressed.len(), "members": [{
                "sha": digest, "size": payload.len(), "offset": 0, "clen": compressed.len(), "vmtime": 1
            }]}]
        })).unwrap()).unwrap();
        fs::write(store.join("manifest.json"), serde_json::to_vec(&serde_json::json!({
            "version": 2, "updatedAtMs": 1, "writerWindowId": "desktop-pack", "fullyHydrated": true,
            "entries": { "workspace/packed.txt": { "kind":"file", "sha": digest, "size": payload.len(), "mode": 384 } }
        })).unwrap()).unwrap();
        assert!(!store.join("blobs").join(&digest).exists());
        let mut backend=RemoteBoxStoreBackend::open(config.clone()).unwrap();
        completed(backend.start(&identity("pack-hydrate","pack-request"),&request("pack-hydrate","hydrate",None)).unwrap());
        assert_eq!(fs::read(config.workspace_root.join("packed.txt")).unwrap(),payload);
    }

    #[test]
    fn frozen_v2_missing_pack_falls_back_to_loose_blob() {
        let config=config("frozen-pack-fallback");
        let store=config.store_root.join("store-a");
        fs::create_dir_all(store.join("blobs")).unwrap();
        fs::create_dir_all(store.join("packs")).unwrap();
        let payload=b"loose-fallback\n";
        let digest=sha256_bytes(payload);
        fs::write(store.join("blobs").join(&digest),payload).unwrap();
        fs::write(store.join("packs/index.json"),serde_json::to_vec(&serde_json::json!({
            "version":1,"maxVmtime":1,"packs":[{"id":"missing-pack","bytes":16,"members":[{
                "sha":digest,"size":payload.len(),"offset":0,"clen":1,"vmtime":1
            }]}]
        })).unwrap()).unwrap();
        fs::write(store.join("manifest.json"),serde_json::to_vec(&serde_json::json!({
            "version":2,"updatedAtMs":1,"fullyHydrated":true,
            "entries":{"workspace/fallback.txt":{"kind":"file","sha":digest,"size":payload.len(),"mode":384}}
        })).unwrap()).unwrap();
        let mut backend=RemoteBoxStoreBackend::open(config.clone()).unwrap();
        completed(backend.start(&identity("fallback-hydrate","fallback-request"),&request("fallback-hydrate","hydrate",None)).unwrap());
        assert_eq!(fs::read(config.workspace_root.join("fallback.txt")).unwrap(),payload);
    }

}
