use fabushi_android_shared::{ExecutionError, ExecutionRequest};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::server::{
    RemoteBackendCancelOutcome, RemoteBackendReconcileOutcome, RemoteBackendStartOutcome,
    RemoteExecutionBackend, RemoteRequestIdentity,
};

pub const BOX_DISK_PRESSURE_CAPABILITY: &str = "box.disk_pressure";
const MAX_ROOTS: usize = 8;
const MAX_LABEL_BYTES: usize = 128;
const MAX_PATH_BYTES: usize = 4096;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiskPressureRequest {
    pub version: u32,
}

impl DiskPressureRequest {
    fn parse(request: &ExecutionRequest) -> Result<Self, ExecutionError> {
        if request.capability_id != BOX_DISK_PRESSURE_CAPABILITY {
            return Err(ExecutionError::CapabilityUnavailable(format!(
                "unsupported remote capability {}",
                request.capability_id
            )));
        }
        let parsed: Self = serde_json::from_str(&request.input_json).map_err(|_| {
            ExecutionError::InvalidRequest("box.disk_pressure input JSON is malformed".into())
        })?;
        if parsed.version != 1 {
            return Err(ExecutionError::InvalidRequest(
                "unsupported box.disk_pressure wire version".into(),
            ));
        }
        Ok(parsed)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiskPressureRoot {
    pub label: String,
    pub path: PathBuf,
}

impl DiskPressureRoot {
    pub fn new(label: impl Into<String>, path: impl Into<PathBuf>) -> Result<Self, ExecutionError> {
        let label = label.into();
        let path = path.into();
        validate_label(&label)?;
        validate_path(&path)?;
        Ok(Self { label, path })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiskPressureSnapshot {
    pub volume: String,
    pub device_id: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiskPressureSample {
    pub version: u32,
    pub complete: bool,
    pub snapshots: Vec<DiskPressureSnapshot>,
}

pub struct DiskPressureBackend {
    roots: Vec<DiskPressureRoot>,
}

impl DiskPressureBackend {
    pub fn new(roots: Vec<DiskPressureRoot>) -> Result<Self, ExecutionError> {
        if roots.is_empty() || roots.len() > MAX_ROOTS {
            return Err(ExecutionError::InvalidRequest(
                "box.disk_pressure roots are empty or exceed the bounded limit".into(),
            ));
        }
        let mut labels = HashSet::new();
        for root in &roots {
            validate_label(&root.label)?;
            validate_path(&root.path)?;
            if !labels.insert(root.label.clone()) {
                return Err(ExecutionError::InvalidRequest(
                    "box.disk_pressure root labels must be unique".into(),
                ));
            }
        }
        Ok(Self { roots })
    }

    pub fn sample(&self) -> DiskPressureSample {
        let mut complete = true;
        let mut snapshots = Vec::new();
        let mut devices = HashSet::new();
        for root in &self.roots {
            match sample_root(root) {
                Ok(snapshot) => {
                    if devices.insert(snapshot.device_id.clone()) {
                        snapshots.push(snapshot);
                    }
                }
                Err(_) => complete = false,
            }
        }
        DiskPressureSample {
            version: 1,
            complete,
            snapshots,
        }
    }

    fn output_json(&self) -> Result<String, ExecutionError> {
        serde_json::to_string(&self.sample()).map_err(|error| {
            ExecutionError::Transport(format!(
                "failed to encode box.disk_pressure sample: {error}"
            ))
        })
    }
}

impl RemoteExecutionBackend for DiskPressureBackend {
    fn start(
        &mut self,
        _identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendStartOutcome, ExecutionError> {
        DiskPressureRequest::parse(request)?;
        Ok(RemoteBackendStartOutcome::Completed {
            output_json: self.output_json()?,
        })
    }

    fn reconcile(
        &mut self,
        _identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
        DiskPressureRequest::parse(request)?;
        Ok(RemoteBackendReconcileOutcome::Completed {
            output_json: self.output_json()?,
        })
    }

    fn cancel(
        &mut self,
        _identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendCancelOutcome, ExecutionError> {
        DiskPressureRequest::parse(request)?;
        Ok(RemoteBackendCancelOutcome::Confirmed)
    }
}

fn validate_label(label: &str) -> Result<(), ExecutionError> {
    if label.trim().is_empty()
        || label.len() > MAX_LABEL_BYTES
        || label.chars().any(char::is_control)
    {
        return Err(ExecutionError::InvalidRequest(
            "box.disk_pressure root label is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_path(path: &Path) -> Result<(), ExecutionError> {
    let raw = path.to_string_lossy();
    if !path.is_absolute()
        || raw.len() > MAX_PATH_BYTES
        || raw.chars().any(char::is_control)
    {
        return Err(ExecutionError::InvalidRequest(
            "box.disk_pressure root path must be a bounded absolute path".into(),
        ));
    }
    Ok(())
}

fn sample_root(root: &DiskPressureRoot) -> Result<DiskPressureSnapshot, std::io::Error> {
    let metadata = fs::metadata(&root.path)?;
    let stats = fs2::statvfs(&root.path)?;
    Ok(DiskPressureSnapshot {
        volume: root.label.clone(),
        device_id: device_id(&root.path, &metadata),
        total_bytes: stats.total_space(),
        available_bytes: stats.available_space(),
    })
}

#[cfg(unix)]
fn device_id(_path: &Path, metadata: &fs::Metadata) -> String {
    use std::os::unix::fs::MetadataExt;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::RemoteRequestIdentity;

    fn identity() -> RemoteRequestIdentity {
        RemoteRequestIdentity {
            credential_id: "credential-1".into(),
            operation_id: "operation-1".into(),
            request_id: "request-1".into(),
            device_id: "device-1".into(),
            account_fence: "session:a".into(),
            account_epoch: 1,
            permission_grant_id: "grant-1".into(),
        }
    }

    fn request(input_json: &str) -> ExecutionRequest {
        ExecutionRequest {
            operation_id: "operation-1".into(),
            capability_id: BOX_DISK_PRESSURE_CAPABILITY.into(),
            input_json: input_json.into(),
            timeout_ms: 5_000,
        }
    }

    #[test]
    fn backend_samples_real_bounded_root_without_shell_fallback() {
        let root = std::env::temp_dir();
        let mut backend = DiskPressureBackend::new(vec![
            DiskPressureRoot::new("temp", &root).unwrap(),
        ])
        .unwrap();
        let output = backend.start(&identity(), &request(r#"{"version":1}"#)).unwrap();
        let RemoteBackendStartOutcome::Completed { output_json } = output else {
            panic!("disk pressure probe is synchronous and read-only");
        };
        let sample: DiskPressureSample = serde_json::from_str(&output_json).unwrap();
        assert_eq!(sample.version, 1);
        assert!(!sample.snapshots.is_empty());
        assert!(sample.snapshots[0].total_bytes >= sample.snapshots[0].available_bytes);
    }

    #[test]
    fn malformed_or_wrong_capability_requests_fail_closed() {
        let mut backend = DiskPressureBackend::new(vec![
            DiskPressureRoot::new("temp", std::env::temp_dir()).unwrap(),
        ])
        .unwrap();
        assert!(backend.start(&identity(), &request(r#"{"version":2}"#)).is_err());
        let mut wrong = request(r#"{"version":1}"#);
        wrong.capability_id = "remote.shell".into();
        assert!(backend.start(&identity(), &wrong).is_err());
    }

    #[test]
    fn invalid_roots_are_rejected_before_sampling() {
        assert!(DiskPressureRoot::new("relative", "tmp").is_err());
        assert!(DiskPressureBackend::new(Vec::new()).is_err());
    }
}
