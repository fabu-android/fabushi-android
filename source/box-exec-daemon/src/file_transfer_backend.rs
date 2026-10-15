use crate::protected_path_guard::assert_path_outside_protected_roots;
use crate::server::{
    RemoteBackendCancelOutcome, RemoteBackendReconcileOutcome, RemoteBackendStartOutcome,
    RemoteExecutionAuthorizer, RemoteExecutionBackend, RemoteExecutionService, RemoteRequestIdentity,
};
use fabushi_android_shared::{ExecutionError, ExecutionRequest};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

pub const BOX_FILE_TRANSFER_CAPABILITY: &str = "box.file.transfer";
pub const MAX_TRANSFER_BYTES: u64 = 100 * 1024 * 1024;
pub const TRANSFER_CHUNK_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug)]
pub struct RemoteFileTransferConfig {
    pub user_root: PathBuf,
    pub agent_box_root: PathBuf,
    /// Host-only roots that must never be exposed through CopyToBox/CopyFromBox,
    /// even through relative traversal or a symlink alias.
    pub protected_roots: Vec<PathBuf>,
}

impl RemoteFileTransferConfig {
    pub fn validate(&self) -> Result<(), ExecutionError> {
        for (label, root) in [
            ("user root", &self.user_root),
            ("agent box root", &self.agent_box_root),
        ] {
            if root.as_os_str().is_empty() || !root.is_absolute() {
                return Err(ExecutionError::InvalidRequest(format!(
                    "file-transfer {label} must be absolute"
                )));
            }
        }
        for root in &self.protected_roots {
            if root.as_os_str().is_empty() || !root.is_absolute() {
                return Err(ExecutionError::InvalidRequest(
                    "file-transfer protected root must be absolute".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
enum TransferTool {
    CopyToBox,
    CopyFromBox,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferEnvelope {
    tool: TransferTool,
    arguments: TransferArguments,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferArguments {
    #[serde(default)]
    computer_path: Option<String>,
    #[serde(default)]
    box_path: Option<String>,
    #[serde(default)]
    computer: Option<String>,
    #[serde(default)]
    expected_destination_version: Option<String>,
    #[serde(default = "default_overwrite")]
    overwrite: bool,
}

fn default_overwrite() -> bool {
    true
}

#[derive(Clone, Debug)]
struct TransferPlan {
    tool: TransferTool,
    source: PathBuf,
    destination: PathBuf,
    source_display: String,
    destination_display: String,
    expected_destination_version: Option<String>,
    overwrite: bool,
}

#[derive(Debug)]
struct StagedTransfer {
    destination: PathBuf,
    staging: PathBuf,
    total_bytes: u64,
    received_bytes: u64,
    expected_sha256: String,
    starting_version: Option<String>,
    hasher: Sha256,
}

pub struct RemoteFileTransferBackend {
    config: RemoteFileTransferConfig,
}

impl RemoteFileTransferBackend {
    pub fn open(config: RemoteFileTransferConfig) -> Result<Self, ExecutionError> {
        config.validate()?;
        fs::create_dir_all(&config.user_root).map_err(io_error)?;
        fs::create_dir_all(&config.agent_box_root).map_err(io_error)?;
        Ok(Self { config })
    }

    fn parse(&self, request: &ExecutionRequest) -> Result<TransferPlan, ExecutionError> {
        if request.capability_id != BOX_FILE_TRANSFER_CAPABILITY {
            return Err(ExecutionError::CapabilityUnavailable(
                request.capability_id.clone(),
            ));
        }
        let input: TransferEnvelope = serde_json::from_str(&request.input_json)
            .map_err(|_| ExecutionError::InvalidRequest("invalid box file-transfer request".into()))?;
        if let Some(computer) = input.arguments.computer.as_deref() {
            if computer.trim().is_empty() || computer.len() > 512 || computer.chars().any(char::is_control) {
                return Err(ExecutionError::InvalidRequest(
                    "file-transfer computer selector is invalid".into(),
                ));
            }
        }
        let (source_root, source_raw, destination_root, destination_raw) = match input.tool {
            TransferTool::CopyToBox => {
                let source = required_path(input.arguments.computer_path.as_deref(), "computer_path")?;
                let destination = input
                    .arguments
                    .box_path
                    .clone()
                    .unwrap_or_else(|| format!("uploads/{}", basename(&source)));
                (
                    &self.config.user_root,
                    source,
                    &self.config.agent_box_root,
                    destination,
                )
            }
            TransferTool::CopyFromBox => {
                let source = required_path(input.arguments.box_path.as_deref(), "box_path")?;
                let destination = input
                    .arguments
                    .computer_path
                    .clone()
                    .unwrap_or_else(|| basename(&source));
                (
                    &self.config.agent_box_root,
                    source,
                    &self.config.user_root,
                    destination,
                )
            }
        };
        assert_path_outside_protected_roots(
            &self.config.protected_roots,
            Path::new(&source_raw),
            source_root,
        )
        .map_err(|error| ExecutionError::InvalidRequest(error.to_string()))?;
        assert_path_outside_protected_roots(
            &self.config.protected_roots,
            Path::new(&destination_raw),
            destination_root,
        )
        .map_err(|error| ExecutionError::InvalidRequest(error.to_string()))?;

        let source = bounded_path(source_root, &source_raw, false)?;
        let destination = bounded_path(destination_root, &destination_raw, true)?;
        Ok(TransferPlan {
            tool: input.tool,
            source,
            destination,
            source_display: source_raw,
            destination_display: destination_raw,
            expected_destination_version: input.arguments.expected_destination_version,
            overwrite: input.arguments.overwrite,
        })
    }

    fn start_staged_transfer(
        &self,
        identity: &RemoteRequestIdentity,
        plan: &TransferPlan,
        total_bytes: u64,
        expected_sha256: String,
    ) -> Result<StagedTransfer, ExecutionError> {
        if total_bytes > MAX_TRANSFER_BYTES {
            return Err(ExecutionError::InvalidRequest(
                "file-transfer source exceeds 100 MiB".into(),
            ));
        }
        let starting_version = file_version(&plan.destination)?;
        if !plan.overwrite && starting_version.is_some() {
            return Err(ExecutionError::InvalidRequest(
                "file-transfer destination exists and overwrite=false".into(),
            ));
        }
        if plan.expected_destination_version.is_some()
            && plan.expected_destination_version != starting_version
        {
            return Err(ExecutionError::InvalidRequest(
                "file-transfer destination version conflict".into(),
            ));
        }
        let parent = plan
            .destination
            .parent()
            .ok_or_else(|| ExecutionError::InvalidRequest("file-transfer destination has no parent".into()))?;
        ensure_bounded_parent(parent, match plan.tool {
            TransferTool::CopyToBox => &self.config.agent_box_root,
            TransferTool::CopyFromBox => &self.config.user_root,
        })?;
        let temp_name = format!(
            ".{}.{}.fabushi-transfer",
            plan.destination
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("file"),
            short_hash(&identity.operation_id),
        );
        let staging = parent.join(temp_name);
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&staging)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    ExecutionError::InvalidRequest(
                        "file-transfer staging identity already exists; reconcile the original operation".into(),
                    )
                } else {
                    io_error(error)
                }
            })?;
        file.sync_all().map_err(io_error)?;
        Ok(StagedTransfer {
            destination: plan.destination.clone(),
            staging,
            total_bytes,
            received_bytes: 0,
            expected_sha256,
            starting_version,
            hasher: Sha256::new(),
        })
    }

    fn append_staged_chunk(
        &self,
        transfer: &mut StagedTransfer,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), ExecutionError> {
        if bytes.is_empty() || bytes.len() > TRANSFER_CHUNK_BYTES {
            return Err(ExecutionError::InvalidRequest(
                "file-transfer chunk is empty or exceeds 32 KiB".into(),
            ));
        }
        if offset != transfer.received_bytes {
            return Err(ExecutionError::InvalidRequest(
                "file-transfer chunk is duplicate or out of order".into(),
            ));
        }
        if transfer
            .received_bytes
            .saturating_add(bytes.len() as u64)
            > transfer.total_bytes
        {
            return Err(ExecutionError::InvalidRequest(
                "file-transfer chunk exceeds declared transfer size".into(),
            ));
        }
        let mut file = OpenOptions::new()
            .append(true)
            .open(&transfer.staging)
            .map_err(io_error)?;
        file.write_all(bytes).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        transfer.hasher.update(bytes);
        transfer.received_bytes += bytes.len() as u64;
        Ok(())
    }

    fn commit_staged_transfer(
        &self,
        plan: &TransferPlan,
        transfer: StagedTransfer,
    ) -> Result<(u64, String), ExecutionError> {
        let result = (|| {
            if transfer.received_bytes != transfer.total_bytes {
                return Err(ExecutionError::InvalidRequest(
                    "file-transfer staged transfer is incomplete".into(),
                ));
            }
            let digest = format!("{:x}", transfer.hasher.finalize());
            if digest != transfer.expected_sha256 {
                return Err(ExecutionError::InvalidRequest(
                    "file-transfer SHA-256 does not match staged source".into(),
                ));
            }
            if file_version(&plan.destination)? != transfer.starting_version {
                return Err(ExecutionError::InvalidRequest(
                    "file-transfer destination version changed during transfer".into(),
                ));
            }
            fs::rename(&transfer.staging, &transfer.destination).map_err(io_error)?;
            #[cfg(unix)]
            if let Some(parent) = transfer.destination.parent() {
                File::open(parent).and_then(|file| file.sync_all()).map_err(io_error)?;
            }
            let committed = file_version(&transfer.destination)?
                .ok_or_else(|| ExecutionError::Transport("file-transfer commit disappeared".into()))?;
            if committed != digest {
                return Err(ExecutionError::Transport(
                    "file-transfer committed digest mismatch".into(),
                ));
            }
            Ok((transfer.received_bytes, digest))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&transfer.staging);
        }
        result
    }

    fn transfer(
        &self,
        identity: &RemoteRequestIdentity,
        plan: &TransferPlan,
    ) -> Result<String, ExecutionError> {
        validate_source(&plan.source)?;
        let source_len = fs::metadata(&plan.source).map_err(io_error)?.len();
        if source_len > MAX_TRANSFER_BYTES {
            return Err(ExecutionError::InvalidRequest(
                "file-transfer source exceeds 100 MiB".into(),
            ));
        }

        // Freeze a source version before crossing the staged side-effect boundary. The transfer
        // then reads again in bounded chunks and commit refuses if content/size changed.
        let expected_sha256 = file_version(&plan.source)?
            .ok_or_else(|| ExecutionError::InvalidRequest("file-transfer source disappeared".into()))?;
        let mut staged = self.start_staged_transfer(
            identity,
            plan,
            source_len,
            expected_sha256.clone(),
        )?;
        let staging_path = staged.staging.clone();
        let result = (|| {
            let mut input = File::open(&plan.source).map_err(io_error)?;
            let mut buffer = vec![0_u8; TRANSFER_CHUNK_BYTES];
            loop {
                let count = input.read(&mut buffer).map_err(io_error)?;
                if count == 0 {
                    break;
                }
                let offset = staged.received_bytes;
                self.append_staged_chunk(&mut staged, offset, &buffer[..count])?;
            }
            if file_version(&plan.source)?.as_deref() != Some(expected_sha256.as_str()) {
                return Err(ExecutionError::InvalidRequest(
                    "file-transfer source version changed during transfer".into(),
                ));
            }
            let (total, digest) = self.commit_staged_transfer(plan, staged)?;
            Ok(json!({
                "tool": match plan.tool {
                    TransferTool::CopyToBox => "CopyToBox",
                    TransferTool::CopyFromBox => "CopyFromBox",
                },
                "sourcePath": plan.source_display,
                "destinationPath": plan.destination_display,
                "bytes": total,
                "sha256": digest,
                "version": digest,
            }).to_string())
        })();
        if result.is_err() {
            let _ = fs::remove_file(staging_path);
        }
        result
    }

    fn reconcile_completed(&self, plan: &TransferPlan) -> Result<Option<String>, ExecutionError> {
        validate_source(&plan.source)?;
        let source_len = fs::metadata(&plan.source).map_err(io_error)?.len();
        if source_len > MAX_TRANSFER_BYTES || !plan.destination.exists() {
            return Ok(None);
        }
        let source_version = file_version(&plan.source)?;
        let destination_version = file_version(&plan.destination)?;
        if source_version.is_some() && source_version == destination_version {
            let digest = source_version.unwrap();
            return Ok(Some(json!({
                "tool": match plan.tool {
                    TransferTool::CopyToBox => "CopyToBox",
                    TransferTool::CopyFromBox => "CopyFromBox",
                },
                "sourcePath": plan.source_display,
                "destinationPath": plan.destination_display,
                "bytes": source_len,
                "sha256": digest,
                "version": digest,
                "reconciled": true,
            }).to_string()));
        }
        Ok(None)
    }

    fn cleanup_staging(&self, identity: &RemoteRequestIdentity, plan: &TransferPlan) {
        if let Some(parent) = plan.destination.parent() {
            let name = format!(
                ".{}.{}.fabushi-transfer",
                plan.destination
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("file"),
                short_hash(&identity.operation_id),
            );
            let _ = fs::remove_file(parent.join(name));
        }
    }
}

impl RemoteExecutionBackend for RemoteFileTransferBackend {
    fn start(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendStartOutcome, ExecutionError> {
        let plan = self.parse(request)?;
        match self.transfer(identity, &plan) {
            Ok(output_json) => Ok(RemoteBackendStartOutcome::Completed { output_json }),
            Err(ExecutionError::InvalidRequest(reason))
            | Err(ExecutionError::CapabilityUnavailable(reason)) => {
                Ok(RemoteBackendStartOutcome::Rejected { reason })
            }
            Err(error) => Ok(RemoteBackendStartOutcome::OutcomeUnknown {
                reason: format!("{error:?}"),
            }),
        }
    }

    fn reconcile(
        &mut self,
        _identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
        _progress_json: Option<&str>,
    ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
        let plan = self.parse(request)?;
        match self.reconcile_completed(&plan)? {
            Some(output_json) => Ok(RemoteBackendReconcileOutcome::Completed { output_json }),
            None => Ok(RemoteBackendReconcileOutcome::Pending),
        }
    }

    fn cancel(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
        _progress_json: Option<&str>,
    ) -> Result<RemoteBackendCancelOutcome, ExecutionError> {
        let plan = self.parse(request)?;
        if self.reconcile_completed(&plan)?.is_some() {
            return Ok(RemoteBackendCancelOutcome::OutcomeUnknown {
                reason: "file-transfer already appears committed; reconcile the original operation".into(),
            });
        }
        self.cleanup_staging(identity, &plan);
        Ok(RemoteBackendCancelOutcome::Confirmed)
    }
}

pub fn open_file_transfer_service<A: RemoteExecutionAuthorizer>(
    journal_path: impl Into<PathBuf>,
    authorizer: A,
    config: RemoteFileTransferConfig,
    now_ms: u64,
) -> Result<RemoteExecutionService<A, RemoteFileTransferBackend>, ExecutionError> {
    let backend = RemoteFileTransferBackend::open(config)?;
    RemoteExecutionService::open(journal_path, authorizer, backend, now_ms)
}

fn required_path(value: Option<&str>, key: &str) -> Result<String, ExecutionError> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ExecutionError::InvalidRequest(format!("file-transfer requires {key}")))?;
    if value.len() > 4096 || value.contains('\0') {
        return Err(ExecutionError::InvalidRequest(format!(
            "file-transfer {key} is invalid or too large"
        )));
    }
    Ok(value.to_string())
}

fn basename(value: &str) -> String {
    value
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("file")
        .to_string()
}

fn bounded_path(root: &Path, raw: &str, allow_missing_final: bool) -> Result<PathBuf, ExecutionError> {
    let root = fs::canonicalize(root).map_err(io_error)?;
    let input = Path::new(raw);
    let relative = if input.is_absolute() {
        input.strip_prefix(&root).map_err(|_| {
            ExecutionError::InvalidRequest("file-transfer path escapes configured root".into())
        })?
    } else {
        input
    };
    if relative.as_os_str().is_empty() {
        return Err(ExecutionError::InvalidRequest("file-transfer path is empty".into()));
    }
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(ExecutionError::InvalidRequest(
                "file-transfer path contains traversal or non-normal components".into(),
            ));
        }
    }
    let candidate = root.join(relative);
    let mut current = root.clone();
    let components = relative.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(ExecutionError::InvalidRequest(
                "file-transfer path contains traversal or non-normal components".into(),
            ));
        };
        current.push(name);
        let is_final = index + 1 == components.len();
        if current.exists() {
            let metadata = fs::symlink_metadata(&current).map_err(io_error)?;
            if metadata.file_type().is_symlink() {
                return Err(ExecutionError::InvalidRequest(
                    "file-transfer path may not traverse a symlink".into(),
                ));
            }
            if !is_final && !metadata.is_dir() {
                return Err(ExecutionError::InvalidRequest(
                    "file-transfer path ancestor is not a directory".into(),
                ));
            }
        } else if !allow_missing_final || !is_final {
            // A missing destination ancestor is allowed here and will be created one-by-one by
            // ensure_bounded_parent; a source must already be fully present.
            if !allow_missing_final {
                return Err(ExecutionError::InvalidRequest(
                    "file-transfer source does not exist".into(),
                ));
            }
        }
    }
    if !allow_missing_final && !candidate.exists() {
        return Err(ExecutionError::InvalidRequest(
            "file-transfer source does not exist".into(),
        ));
    }
    Ok(candidate)
}

fn ensure_bounded_parent(parent: &Path, root: &Path) -> Result<(), ExecutionError> {
    let root = fs::canonicalize(root).map_err(io_error)?;
    let relative = parent.strip_prefix(&root).map_err(|_| {
        ExecutionError::InvalidRequest("file-transfer parent escapes configured root".into())
    })?;
    let mut current = root;
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(ExecutionError::InvalidRequest(
                "file-transfer parent contains traversal".into(),
            ));
        };
        current.push(name);
        if current.exists() {
            if fs::symlink_metadata(&current).map_err(io_error)?.file_type().is_symlink() {
                return Err(ExecutionError::InvalidRequest(
                    "file-transfer parent may not traverse a symlink".into(),
                ));
            }
            if !current.is_dir() {
                return Err(ExecutionError::InvalidRequest(
                    "file-transfer parent component is not a directory".into(),
                ));
            }
        } else {
            fs::create_dir(&current).map_err(io_error)?;
        }
    }
    Ok(())
}

fn validate_source(path: &Path) -> Result<(), ExecutionError> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ExecutionError::InvalidRequest(
            "file-transfer source must be a regular non-symlink file".into(),
        ));
    }
    Ok(())
}

fn file_version(path: &Path) -> Result<Option<String>, ExecutionError> {
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ExecutionError::InvalidRequest(
            "file-transfer destination must be a regular non-symlink file".into(),
        ));
    }
    if metadata.len() > MAX_TRANSFER_BYTES {
        return Err(ExecutionError::InvalidRequest(
            "file-transfer existing file exceeds 100 MiB".into(),
        ));
    }
    let mut file = File::open(path).map_err(io_error)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; TRANSFER_CHUNK_BYTES];
    loop {
        let count = file.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(Some(format!("{:x}", hasher.finalize())))
}

fn short_hash(value: &str) -> String {
    let full = format!("{:x}", Sha256::digest(value.as_bytes()));
    full[..24].to_string()
}

fn io_error(error: std::io::Error) -> ExecutionError {
    ExecutionError::Transport(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-remote-file-transfer-{label}-{}",
            std::process::id()
        ))
    }

    fn setup(label: &str) -> (PathBuf, RemoteFileTransferBackend) {
        let root = root(label);
        let _ = fs::remove_dir_all(&root);
        let user = root.join("user");
        let box_root = root.join("box");
        fs::create_dir_all(&user).unwrap();
        fs::create_dir_all(&box_root).unwrap();
        (
            root,
            RemoteFileTransferBackend::open(RemoteFileTransferConfig {
                user_root: user,
                agent_box_root: box_root,
                protected_roots: Vec::new(),
            })
            .unwrap(),
        )
    }

    fn identity(operation_id: &str) -> RemoteRequestIdentity {
        RemoteRequestIdentity {
            credential_id: "credential".into(),
            operation_id: operation_id.into(),
            request_id: format!("request-{operation_id}"),
            account_fence: "session:a".into(),
            account_epoch: 7,
            permission_grant_id: format!("grant-{operation_id}"),
            device_id: "device-a".into(),
        }
    }

    fn request(operation_id: &str, tool: &str, arguments: serde_json::Value) -> ExecutionRequest {
        ExecutionRequest {
            operation_id: operation_id.into(),
            capability_id: BOX_FILE_TRANSFER_CAPABILITY.into(),
            input_json: json!({"tool":tool,"arguments":arguments}).to_string(),
            timeout_ms: 120_000,
        }
    }

    #[test]
    fn copy_to_and_from_box_preserve_digest_and_default_paths() {
        let (root, mut backend) = setup("roundtrip");
        fs::write(root.join("user/report.bin"), b"hello transfer").unwrap();
        let to_box = backend
            .start(
                &identity("op-to"),
                &request("op-to", "CopyToBox", json!({"computer_path":"report.bin"})),
            )
            .unwrap();
        assert!(matches!(to_box, RemoteBackendStartOutcome::Completed { .. }));
        assert_eq!(fs::read(root.join("box/uploads/report.bin")).unwrap(), b"hello transfer");
        assert!(matches!(
            backend
                .cancel(
                    &identity("op-to"),
                    &request("op-to", "CopyToBox", json!({"computer_path":"report.bin"})),
                )
                .unwrap(),
            RemoteBackendCancelOutcome::OutcomeUnknown { .. }
        ));

        let from_box = backend
            .start(
                &identity("op-from"),
                &request("op-from", "CopyFromBox", json!({"box_path":"uploads/report.bin"})),
            )
            .unwrap();
        assert!(matches!(from_box, RemoteBackendStartOutcome::Completed { .. }));
        assert_eq!(fs::read(root.join("user/report.bin")).unwrap(), b"hello transfer");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn configured_host_only_roots_are_rejected_directly_relatively_and_through_symlink_alias() {
        let root = root("protected");
        let _ = fs::remove_dir_all(&root);
        let user = root.join("user");
        let box_root = root.join("box");
        let protected = user.join("host-store");
        fs::create_dir_all(&protected).unwrap();
        fs::create_dir_all(&box_root).unwrap();
        fs::write(protected.join("secret.db"), b"secret").unwrap();

        let mut backend = RemoteFileTransferBackend::open(RemoteFileTransferConfig {
            user_root: user.clone(),
            agent_box_root: box_root,
            protected_roots: vec![protected.clone()],
        }).unwrap();

        let direct = request(
            "op-protected-direct",
            "CopyToBox",
            json!({"computer_path":"host-store/secret.db","box_path":"safe.db"}),
        );
        assert!(backend.start(&identity("op-protected-direct"), &direct).is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let alias = user.join("alias");
            symlink(&protected, &alias).unwrap();
            let aliased = request(
                "op-protected-alias",
                "CopyToBox",
                json!({"computer_path":"alias/secret.db","box_path":"safe.db"}),
            );
            assert!(backend.start(&identity("op-protected-alias"), &aliased).is_err());
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn oversize_version_conflict_traversal_and_symlink_fail_closed() {
        let (root, mut backend) = setup("reject");
        fs::write(root.join("user/source"), b"new").unwrap();
        fs::write(root.join("box/existing"), b"old").unwrap();
        let conflict = request(
            "op-conflict",
            "CopyToBox",
            json!({
                "computer_path":"source",
                "box_path":"existing",
                "expected_destination_version":"0".repeat(64)
            }),
        );
        assert!(matches!(
            backend.start(&identity("op-conflict"), &conflict).unwrap(),
            RemoteBackendStartOutcome::Rejected { .. }
        ));
        let traversal = request(
            "op-traversal",
            "CopyToBox",
            json!({"computer_path":"../source","box_path":"safe"}),
        );
        assert!(backend.start(&identity("op-traversal"), &traversal).is_err());

        let huge = root.join("user/huge");
        let file = File::create(&huge).unwrap();
        file.set_len(MAX_TRANSFER_BYTES + 1).unwrap();
        let oversize = request(
            "op-huge",
            "CopyToBox",
            json!({"computer_path":"huge","box_path":"huge"}),
        );
        assert!(matches!(
            backend.start(&identity("op-huge"), &oversize).unwrap(),
            RemoteBackendStartOutcome::Rejected { .. }
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn runner_backend_reopen_reconciles_committed_transfer_without_replay() {
        let (root, mut backend) = setup("runner-reopen");
        fs::write(root.join("user/source"), b"survives restart").unwrap();
        let req = request(
            "op-reopen",
            "CopyToBox",
            json!({"computer_path":"source","box_path":"uploads/reopen.bin"}),
        );
        assert!(matches!(
            backend.start(&identity("op-reopen"), &req).unwrap(),
            RemoteBackendStartOutcome::Completed { .. }
        ));
        drop(backend);

        let mut reopened = RemoteFileTransferBackend::open(RemoteFileTransferConfig {
            user_root: root.join("user"),
            agent_box_root: root.join("box"),
            protected_roots: Vec::new(),
        })
        .unwrap();
        assert!(matches!(
            reopened.reconcile(&identity("op-reopen"), &req).unwrap(),
            RemoteBackendReconcileOutcome::Completed { .. }
        ));
        assert_eq!(
            fs::read(root.join("box/uploads/reopen.bin")).unwrap(),
            b"survives restart"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn staged_protocol_fences_duplicate_out_of_order_digest_and_incomplete_commit() {
        let (root, backend) = setup("staged-protocol");
        fs::write(root.join("user/source"), b"abcdef").unwrap();
        let plan = backend
            .parse(&request(
                "op-stage",
                "CopyToBox",
                json!({"computer_path":"source","box_path":"target"}),
            ))
            .unwrap();
        let expected = file_version(&plan.source).unwrap().unwrap();
        let mut staged = backend
            .start_staged_transfer(&identity("op-stage"), &plan, 6, expected.clone())
            .unwrap();
        backend.append_staged_chunk(&mut staged, 0, b"abc").unwrap();
        assert!(backend.append_staged_chunk(&mut staged, 0, b"abc").is_err());
        assert!(backend.append_staged_chunk(&mut staged, 4, b"def").is_err());
        assert!(backend.commit_staged_transfer(&plan, staged).is_err());
        assert!(!root.join("box/target").exists());

        let mut mismatch = backend
            .start_staged_transfer(&identity("op-digest"), &plan, 6, "0".repeat(64))
            .unwrap();
        backend.append_staged_chunk(&mut mismatch, 0, b"abcdef").unwrap();
        assert!(backend.commit_staged_transfer(&plan, mismatch).is_err());
        assert!(!root.join("box/target").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reconcile_detects_committed_copy_and_cancel_cleans_staging_without_replay() {
        let (root, mut backend) = setup("reconcile");
        fs::write(root.join("user/source"), b"stable").unwrap();
        fs::create_dir_all(root.join("box/uploads")).unwrap();
        fs::write(root.join("box/uploads/source"), b"stable").unwrap();
        let req = request("op-r", "CopyToBox", json!({"computer_path":"source"}));
        assert!(matches!(
            backend.reconcile(&identity("op-r"), &req).unwrap(),
            RemoteBackendReconcileOutcome::Completed { .. }
        ));

        fs::remove_file(root.join("box/uploads/source")).unwrap();
        let plan = backend.parse(&req).unwrap();
        let parent = plan.destination.parent().unwrap();
        let staging = parent.join(format!(
            ".{}.{}.fabushi-transfer",
            plan.destination.file_name().unwrap().to_string_lossy(),
            short_hash("op-r")
        ));
        fs::write(&staging, b"partial").unwrap();
        assert_eq!(
            backend.cancel(&identity("op-r"), &req).unwrap(),
            RemoteBackendCancelOutcome::Confirmed
        );
        assert!(!staging.exists());
        let _ = fs::remove_dir_all(root);
    }
}
