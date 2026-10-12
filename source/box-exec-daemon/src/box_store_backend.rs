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

enum ActionError {
    SafeReject(String),
    OutcomeUnknown(String),
}

pub struct RemoteBoxStoreBackend {
    config: RemoteBoxStoreBackendConfig,
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
        self.store_dir(store_id).join("objects")
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
            collect_files(root, root, category, &mut captured, &mut total_bytes)?;
            for mut entry in captured {
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

    fn hydrate(
        &self,
        identity: &RemoteRequestIdentity,
        input: &Input,
    ) -> Result<String, ActionError> {
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

        let mut restored = 0usize;
        let mut restored_bytes = 0u64;
        for entry in &manifest.entries {
            let root = self.category_root(&entry.category)?;
            let relative = rel_path_to_path(&entry.rel_path)?;
            let destination = root.join(relative);
            ensure_within(root, &destination)?;
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
        let metadata = fs::symlink_metadata(child.path())
            .map_err(|error| ActionError::SafeReject(format!("stat snapshot path: {error}")))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_files(root, &child.path(), category, output, total_bytes)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        if metadata.len() > MAX_FILE_BYTES {
            return Err(ActionError::SafeReject(format!(
                "box-store file exceeds per-file limit: {}",
                child.path().display()
            )));
        }
        *total_bytes = total_bytes.saturating_add(metadata.len());
        if *total_bytes > MAX_TOTAL_BYTES {
            return Err(ActionError::SafeReject(
                "box-store snapshot exceeds total byte limit".into(),
            ));
        }
        let relative = child
            .path()
            .strip_prefix(root)
            .map_err(|_| ActionError::SafeReject("snapshot path escaped category root".into()))?
            .to_path_buf();
        let rel_path = path_to_rel_string(&relative)?;
        let bytes = read_bounded_file(&child.path())?;
        output.push(ManifestEntry {
            category: category.to_string(),
            rel_path,
            object_sha256: sha256_bytes(&bytes),
            size_bytes: bytes.len() as u64,
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
        let root = config.store_root.parent().unwrap().parent().unwrap().to_path_buf();
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
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_execute_returns_durable_result_without_replaying_source_changes() {
        let config = config("duplicate");
        fs::create_dir_all(&config.workspace_root).unwrap();
        fs::write(config.workspace_root.join("note.txt"), b"one").unwrap();
        let root = config.store_root.parent().unwrap().parent().unwrap().to_path_buf();
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
        let root = config.store_root.parent().unwrap().parent().unwrap().to_path_buf();
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
        let root = config.store_root.parent().unwrap().parent().unwrap().to_path_buf();
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
}
