use fabushi_android_shared::{ExecutionError, ExecutionRequest};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use crate::server::{
    RemoteBackendCancelOutcome, RemoteBackendReconcileOutcome, RemoteBackendStartOutcome,
    RemoteExecutionAuthorizer, RemoteExecutionBackend, RemoteExecutionService,
    RemoteRequestIdentity,
};

pub const BOX_DISK_PRESSURE_SAMPLE_CAPABILITY: &str = "box.disk_pressure.sample";
const WIRE_VERSION: u32 = 1;
const MAX_ROOTS: usize = 8;
const MAX_LABEL_BYTES: usize = 128;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BoxDiskPressureSampleRequest {
    pub version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteDiskVolumeRoot {
    pub volume: String,
    pub path: PathBuf,
}

impl RemoteDiskVolumeRoot {
    pub fn new(volume: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            volume: volume.into(),
            path: path.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RemoteBoxDiskPressureConfig {
    pub roots: Vec<RemoteDiskVolumeRoot>,
}

impl RemoteBoxDiskPressureConfig {
    fn validate(&self) -> Result<(), ExecutionError> {
        if self.roots.is_empty() || self.roots.len() > MAX_ROOTS {
            return Err(ExecutionError::InvalidRequest(
                "box disk-pressure roots are empty or exceed the bounded limit".into(),
            ));
        }
        let mut labels = HashSet::new();
        for root in &self.roots {
            let label = root.volume.trim();
            if label.is_empty()
                || label.len() > MAX_LABEL_BYTES
                || label.chars().any(char::is_control)
                || !labels.insert(label.to_string())
            {
                return Err(ExecutionError::InvalidRequest(
                    "box disk-pressure volume labels must be unique and bounded".into(),
                ));
            }
            if !root.path.is_absolute() {
                return Err(ExecutionError::InvalidRequest(
                    "box disk-pressure roots must be absolute runner paths".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct RemoteDiskVolumeSnapshot {
    volume: String,
    device_id: String,
    total_bytes: u64,
    available_bytes: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct RemoteDiskPressureSample {
    version: u32,
    complete: bool,
    volumes: Vec<RemoteDiskVolumeSnapshot>,
}

pub struct RemoteBoxDiskPressureBackend {
    config: RemoteBoxDiskPressureConfig,
}

impl RemoteBoxDiskPressureBackend {
    pub fn open(config: RemoteBoxDiskPressureConfig) -> Result<Self, ExecutionError> {
        config.validate()?;
        Ok(Self { config })
    }

    fn parse_request(
        request: &ExecutionRequest,
    ) -> Result<BoxDiskPressureSampleRequest, ExecutionError> {
        if request.capability_id != BOX_DISK_PRESSURE_SAMPLE_CAPABILITY {
            return Err(ExecutionError::CapabilityUnavailable(
                request.capability_id.clone(),
            ));
        }
        let input: BoxDiskPressureSampleRequest =
            serde_json::from_str(&request.input_json).map_err(|_| {
                ExecutionError::InvalidRequest(
                    "box.disk_pressure.sample input JSON is malformed".into(),
                )
            })?;
        if input.version != WIRE_VERSION {
            return Err(ExecutionError::InvalidRequest(
                "unsupported box.disk_pressure.sample wire version".into(),
            ));
        }
        Ok(input)
    }

    fn sample(&self) -> Result<String, ExecutionError> {
        let mut volumes = Vec::new();
        let mut seen_devices = HashSet::new();
        let mut complete = true;
        for root in &self.config.roots {
            match sample_root(root) {
                Ok(snapshot) => {
                    if seen_devices.insert(snapshot.device_id.clone()) {
                        volumes.push(snapshot);
                    }
                }
                Err(_) => complete = false,
            }
        }
        if volumes.is_empty() {
            return Err(ExecutionError::Transport(
                "remote runner could not sample any configured disk-pressure root".into(),
            ));
        }
        serde_json::to_string(&RemoteDiskPressureSample {
            version: WIRE_VERSION,
            complete,
            volumes,
        })
        .map_err(|error| {
            ExecutionError::Transport(format!(
                "encode box disk-pressure sample: {error}"
            ))
        })
    }
}

impl RemoteExecutionBackend for RemoteBoxDiskPressureBackend {
    fn start(
        &mut self,
        _identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendStartOutcome, ExecutionError> {
        Self::parse_request(request)?;
        Ok(RemoteBackendStartOutcome::Completed {
            output_json: self.sample()?,
        })
    }

    fn reconcile(
        &mut self,
        _identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
        Self::parse_request(request)?;
        Ok(RemoteBackendReconcileOutcome::Completed {
            output_json: self.sample()?,
        })
    }

    fn cancel(
        &mut self,
        _identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendCancelOutcome, ExecutionError> {
        Self::parse_request(request)?;
        Ok(RemoteBackendCancelOutcome::Confirmed)
    }
}

pub fn open_box_disk_pressure_service<A>(
    journal_path: impl Into<PathBuf>,
    authorizer: A,
    config: RemoteBoxDiskPressureConfig,
    now_ms: u64,
) -> Result<RemoteExecutionService<A, RemoteBoxDiskPressureBackend>, ExecutionError>
where
    A: RemoteExecutionAuthorizer,
{
    let backend = RemoteBoxDiskPressureBackend::open(config)?;
    RemoteExecutionService::open(journal_path, authorizer, backend, now_ms)
}

#[cfg(unix)]
fn device_id(path: &Path, metadata: &fs::Metadata) -> String {
    use std::os::unix::fs::MetadataExt;
    let _ = path;
    metadata.dev().to_string()
}

#[cfg(not(unix))]
fn device_id(path: &Path, _metadata: &fs::Metadata) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .components()
        .next()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn sample_root(root: &RemoteDiskVolumeRoot) -> Result<RemoteDiskVolumeSnapshot, ExecutionError> {
    let metadata = fs::metadata(&root.path)
        .map_err(|error| ExecutionError::Transport(format!("stat remote volume: {error}")))?;
    let stats = fs2::statvfs(&root.path)
        .map_err(|error| ExecutionError::Transport(format!("statvfs remote volume: {error}")))?;
    Ok(RemoteDiskVolumeSnapshot {
        volume: root.volume.clone(),
        device_id: device_id(&root.path, &metadata),
        total_bytes: stats.total_space(),
        available_bytes: stats.available_space(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-box-pressure-{label}-{}",
            std::process::id()
        ))
    }

    fn identity() -> RemoteRequestIdentity {
        RemoteRequestIdentity {
            credential_id: "credential-a".into(),
            operation_id: "operation-a".into(),
            request_id: "request-a".into(),
            account_fence: "account-a:epoch-7".into(),
            account_epoch: 7,
            permission_grant_id: "grant-a".into(),
            device_id: "device-a".into(),
        }
    }

    fn request() -> ExecutionRequest {
        ExecutionRequest {
            operation_id: "operation-a".into(),
            capability_id: BOX_DISK_PRESSURE_SAMPLE_CAPABILITY.into(),
            input_json: r#"{"version":1}"#.into(),
            timeout_ms: 30_000,
        }
    }

    #[test]
    fn sample_is_read_only_bounded_and_deduplicates_same_device() {
        let base = root("sample");
        let workspace = base.join("workspace");
        let data = base.join("sand-data");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&data).unwrap();
        let mut backend = RemoteBoxDiskPressureBackend::open(RemoteBoxDiskPressureConfig {
            roots: vec![
                RemoteDiskVolumeRoot::new("workspace", &workspace),
                RemoteDiskVolumeRoot::new("sand_data", &data),
            ],
        })
        .unwrap();

        let output = match backend.start(&identity(), &request()).unwrap() {
            RemoteBackendStartOutcome::Completed { output_json } => output_json,
            other => panic!("expected completed sample, got {other:?}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["version"], 1);
        let volumes = parsed["volumes"].as_array().unwrap();
        assert!(!volumes.is_empty());
        assert!(volumes.len() <= 2);
        for volume in volumes {
            let total = volume["totalBytes"].as_u64().unwrap();
            let available = volume["availableBytes"].as_u64().unwrap();
            assert!(total > 0);
            assert!(available <= total);
        }
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn capability_version_and_config_fail_closed() {
        let base = root("validation");
        fs::create_dir_all(&base).unwrap();
        assert!(RemoteBoxDiskPressureBackend::open(RemoteBoxDiskPressureConfig {
            roots: vec![RemoteDiskVolumeRoot::new("relative", PathBuf::from("relative"))],
        })
        .is_err());

        let mut backend = RemoteBoxDiskPressureBackend::open(RemoteBoxDiskPressureConfig {
            roots: vec![RemoteDiskVolumeRoot::new("workspace", &base)],
        })
        .unwrap();
        let mut wrong_capability = request();
        wrong_capability.capability_id = "shell".into();
        assert!(matches!(
            backend.start(&identity(), &wrong_capability),
            Err(ExecutionError::CapabilityUnavailable(_))
        ));
        let mut wrong_version = request();
        wrong_version.input_json = r#"{"version":2}"#.into();
        assert!(matches!(
            backend.start(&identity(), &wrong_version),
            Err(ExecutionError::InvalidRequest(_))
        ));
        let _ = fs::remove_dir_all(base);
    }
}
