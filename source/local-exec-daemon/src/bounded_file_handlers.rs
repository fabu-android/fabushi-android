use crate::{bounded_fs::BoundedLocalFs, AndroidLocalRunner, LocalCapabilityHandler};
use fabushi_android_shared::{
    ExecutionCapability, ExecutionError, ExecutionRequest, ExecutionResult, ExecutionTarget,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self as stdfs, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use uuid::Uuid;

pub const LOCAL_FILE_READ_CAPABILITY: &str = "android.local.file.read";
pub const LOCAL_FILE_WRITE_CAPABILITY: &str = "android.local.file.write";
pub const LOCAL_FILE_UPLOAD_START_CAPABILITY: &str = "android.local.file.upload.start";
pub const LOCAL_FILE_UPLOAD_CHUNK_CAPABILITY: &str = "android.local.file.upload.chunk";
pub const LOCAL_FILE_UPLOAD_COMMIT_CAPABILITY: &str = "android.local.file.upload.commit";
pub const DEFAULT_MAX_LOCAL_FILE_BYTES: u64 = 100 * 1024 * 1024;
pub const MAX_LOCAL_TOOL_READ_BYTES: usize = 48 * 1024;
pub const MAX_LOCAL_UPLOAD_CHUNK_BYTES: usize = 32 * 1024;

#[derive(Clone)]
struct ReadHandler {
    fs: Arc<BoundedLocalFs>,
}

#[derive(Clone)]
struct WriteHandler {
    fs: Arc<BoundedLocalFs>,
}

#[derive(Debug)]
struct UploadTransfer {
    path: String,
    target: PathBuf,
    staging: PathBuf,
    total_bytes: u64,
    received_bytes: u64,
    expected_sha256: String,
    starting_version: Option<String>,
    hasher: Sha256,
}

type UploadRegistry = Arc<Mutex<BTreeMap<String, UploadTransfer>>>;

#[derive(Clone)]
struct UploadStartHandler {
    fs: Arc<BoundedLocalFs>,
    uploads: UploadRegistry,
}

#[derive(Clone)]
struct UploadChunkHandler {
    fs: Arc<BoundedLocalFs>,
    uploads: UploadRegistry,
}

#[derive(Clone)]
struct UploadCommitHandler {
    fs: Arc<BoundedLocalFs>,
    uploads: UploadRegistry,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadArgs {
    path: String,
    #[serde(default)]
    offset: u64,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WriteArgs {
    path: String,
    content: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UploadStartArgs {
    transfer_id: String,
    path: String,
    total_bytes: u64,
    sha256: String,
    expected_version: Option<String>,
    #[serde(default = "default_overwrite")]
    overwrite: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UploadChunkArgs {
    transfer_id: String,
    path: String,
    offset: u64,
    bytes_base64: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UploadCommitArgs {
    transfer_id: String,
    path: String,
}

fn default_overwrite() -> bool { true }

fn valid_transfer_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn current_version(fs: &BoundedLocalFs, path: &Path) -> Result<Option<String>, ExecutionError> {
    match fs.read_file(path) {
        Ok(bytes) => Ok(Some(sha256_hex(&bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(error)),
    }
}

fn cleanup_transfer(uploads: &UploadRegistry, transfer_id: &str) {
    if let Ok(mut registry) = uploads.lock() {
        if let Some(transfer) = registry.remove(transfer_id) {
            let _ = stdfs::remove_file(transfer.staging);
        }
    }
}

fn io_error(error: std::io::Error) -> ExecutionError {
    ExecutionError::InvalidRequest(error.to_string())
}

fn parse_path(value: &str) -> Result<&Path, ExecutionError> {
    if value.trim().is_empty() || value.len() > 4096 || value.contains('\0') {
        return Err(ExecutionError::InvalidRequest(
            "local file path is invalid or too large".into(),
        ));
    }
    Ok(Path::new(value))
}

impl LocalCapabilityHandler for ReadHandler {
    fn execute(&mut self, request: &ExecutionRequest) -> Result<ExecutionResult, ExecutionError> {
        let args: ReadArgs = serde_json::from_str(&request.input_json)
            .map_err(|_| ExecutionError::InvalidRequest("invalid local Read arguments".into()))?;
        let path = parse_path(&args.path)?;
        let limit = args.limit.unwrap_or(MAX_LOCAL_TOOL_READ_BYTES);
        if limit == 0 || limit > MAX_LOCAL_TOOL_READ_BYTES {
            return Err(ExecutionError::InvalidRequest(
                "local Read limit is outside the bounded range".into(),
            ));
        }
        let (bytes, eof) = self
            .fs
            .read_file_range(path, args.offset, limit)
            .map_err(io_error)?;
        let content = String::from_utf8(bytes).map_err(|_| {
            ExecutionError::InvalidRequest(
                "local Read currently supports UTF-8 text; binary file transfer requires a dedicated bounded transport".into(),
            )
        })?;
        let output_json = json!({
            "path": args.path,
            "offset": args.offset,
            "content": content,
            "eof": eof,
        })
        .to_string();
        let result = ExecutionResult {
            operation_id: request.operation_id.clone(),
            output_json,
        };
        result.validate()?;
        Ok(result)
    }

    fn cancel(&mut self, _operation_id: &str) -> Result<(), ExecutionError> {
        Err(ExecutionError::InvalidRequest(
            "bounded local Read is synchronous and non-cancellable".into(),
        ))
    }
}

impl LocalCapabilityHandler for UploadStartHandler {
    fn execute(&mut self, request: &ExecutionRequest) -> Result<ExecutionResult, ExecutionError> {
        let args: UploadStartArgs = serde_json::from_str(&request.input_json)
            .map_err(|_| ExecutionError::InvalidRequest("invalid staged upload start arguments".into()))?;
        if !valid_transfer_id(&args.transfer_id) || !valid_sha256(&args.sha256) {
            return Err(ExecutionError::InvalidRequest("staged upload identity or SHA-256 is invalid".into()));
        }
        if args.total_bytes > DEFAULT_MAX_LOCAL_FILE_BYTES {
            return Err(ExecutionError::InvalidRequest("staged upload exceeds bounded file size".into()));
        }
        let requested = parse_path(&args.path)?;
        let target = self.fs.resolve_contained(requested).map_err(io_error)?;
        if let Some(parent) = target.parent() {
            stdfs::create_dir_all(parent).map_err(io_error)?;
        }
        let target = self.fs.resolve_contained(requested).map_err(io_error)?;
        let starting_version = current_version(&self.fs, requested)?;
        if !args.overwrite && starting_version.is_some() {
            return Err(ExecutionError::InvalidRequest("staged upload target exists and overwrite=false".into()));
        }
        if args.expected_version.as_deref() != None && args.expected_version != starting_version {
            return Err(ExecutionError::InvalidRequest("staged upload expectedVersion conflict".into()));
        }
        let parent = target.parent().ok_or_else(|| ExecutionError::InvalidRequest("staged upload target has no parent".into()))?;
        let name = target.file_name().and_then(|value| value.to_str()).unwrap_or("upload");
        let staging = parent.join(format!(".{name}.{}.fabushi-upload", Uuid::new_v4().simple()));
        let file = OpenOptions::new().write(true).create_new(true).open(&staging).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        let transfer = UploadTransfer {
            path: args.path.clone(),
            target,
            staging,
            total_bytes: args.total_bytes,
            received_bytes: 0,
            expected_sha256: args.sha256.to_ascii_lowercase(),
            starting_version,
            hasher: Sha256::new(),
        };
        let mut registry = self.uploads.lock().map_err(|_| ExecutionError::Transport("staged upload registry unavailable".into()))?;
        if registry.contains_key(&args.transfer_id) {
            let _ = stdfs::remove_file(&transfer.staging);
            return Err(ExecutionError::InvalidRequest("staged upload transfer id is already active".into()));
        }
        registry.insert(args.transfer_id.clone(), transfer);
        Ok(ExecutionResult { operation_id: request.operation_id.clone(), output_json: json!({"transferId":args.transfer_id,"acceptedBytes":0}).to_string() })
    }

    fn cancel(&mut self, operation_id: &str) -> Result<(), ExecutionError> {
        cleanup_transfer(&self.uploads, operation_id);
        Ok(())
    }
}

impl LocalCapabilityHandler for UploadChunkHandler {
    fn execute(&mut self, request: &ExecutionRequest) -> Result<ExecutionResult, ExecutionError> {
        let args: UploadChunkArgs = serde_json::from_str(&request.input_json)
            .map_err(|_| ExecutionError::InvalidRequest("invalid staged upload chunk arguments".into()))?;
        let decoded = BASE64.decode(args.bytes_base64.as_bytes())
            .map_err(|_| ExecutionError::InvalidRequest("staged upload chunk is not valid base64".into()))?;
        if decoded.is_empty() || decoded.len() > MAX_LOCAL_UPLOAD_CHUNK_BYTES {
            cleanup_transfer(&self.uploads, &args.transfer_id);
            return Err(ExecutionError::InvalidRequest("staged upload chunk exceeds the bounded chunk size".into()));
        }
        let result = (|| {
            let mut registry = self.uploads.lock().map_err(|_| ExecutionError::Transport("staged upload registry unavailable".into()))?;
            let transfer = registry.get_mut(&args.transfer_id)
                .ok_or_else(|| ExecutionError::InvalidRequest("staged upload transfer is unknown".into()))?;
            if transfer.path != args.path || transfer.received_bytes != args.offset {
                return Err(ExecutionError::InvalidRequest("staged upload chunk is stale, out of order, or bound to another path".into()));
            }
            if transfer.received_bytes.saturating_add(decoded.len() as u64) > transfer.total_bytes {
                return Err(ExecutionError::InvalidRequest("staged upload chunk exceeds declared transfer size".into()));
            }
            let revalidated = self.fs.resolve_contained(Path::new(&args.path)).map_err(io_error)?;
            if revalidated != transfer.target {
                return Err(ExecutionError::InvalidRequest("staged upload target changed during transfer".into()));
            }
            let mut file = OpenOptions::new().append(true).open(&transfer.staging).map_err(io_error)?;
            file.write_all(&decoded).map_err(io_error)?;
            file.sync_all().map_err(io_error)?;
            transfer.hasher.update(&decoded);
            transfer.received_bytes += decoded.len() as u64;
            Ok(ExecutionResult { operation_id: request.operation_id.clone(), output_json: json!({"transferId":args.transfer_id,"acceptedBytes":transfer.received_bytes}).to_string() })
        })();
        if result.is_err() { cleanup_transfer(&self.uploads, &args.transfer_id); }
        result
    }

    fn cancel(&mut self, operation_id: &str) -> Result<(), ExecutionError> {
        cleanup_transfer(&self.uploads, operation_id);
        Ok(())
    }
}

impl LocalCapabilityHandler for UploadCommitHandler {
    fn execute(&mut self, request: &ExecutionRequest) -> Result<ExecutionResult, ExecutionError> {
        let args: UploadCommitArgs = serde_json::from_str(&request.input_json)
            .map_err(|_| ExecutionError::InvalidRequest("invalid staged upload commit arguments".into()))?;
        let transfer = {
            let mut registry = self.uploads.lock().map_err(|_| ExecutionError::Transport("staged upload registry unavailable".into()))?;
            registry.remove(&args.transfer_id).ok_or_else(|| ExecutionError::InvalidRequest("staged upload transfer is unknown".into()))?
        };
        let result = (|| {
            if transfer.path != args.path || transfer.received_bytes != transfer.total_bytes {
                return Err(ExecutionError::InvalidRequest("staged upload is incomplete or bound to another path".into()));
            }
            let digest = format!("{:x}", transfer.hasher.finalize());
            if digest != transfer.expected_sha256 {
                return Err(ExecutionError::InvalidRequest("staged upload SHA-256 does not match".into()));
            }
            let requested = Path::new(&args.path);
            let target = self.fs.resolve_contained(requested).map_err(io_error)?;
            if target != transfer.target || current_version(&self.fs, requested)? != transfer.starting_version {
                return Err(ExecutionError::InvalidRequest("staged upload target version changed during transfer".into()));
            }
            stdfs::rename(&transfer.staging, &target).map_err(io_error)?;
            #[cfg(unix)]
            if let Some(parent) = target.parent() { stdfs::File::open(parent).and_then(|file| file.sync_all()).map_err(io_error)?; }
            let committed = self.fs.resolve_contained(requested).map_err(io_error)?;
            if committed != target {
                return Err(ExecutionError::InvalidRequest("staged upload target changed during commit".into()));
            }
            Ok(ExecutionResult { operation_id: request.operation_id.clone(), output_json: json!({"path":args.path,"bytesWritten":transfer.received_bytes,"sha256":digest,"version":digest}).to_string() })
        })();
        if result.is_err() { let _ = stdfs::remove_file(&transfer.staging); }
        result
    }

    fn cancel(&mut self, operation_id: &str) -> Result<(), ExecutionError> {
        cleanup_transfer(&self.uploads, operation_id);
        Ok(())
    }
}

impl LocalCapabilityHandler for WriteHandler {
    fn execute(&mut self, request: &ExecutionRequest) -> Result<ExecutionResult, ExecutionError> {
        let args: WriteArgs = serde_json::from_str(&request.input_json)
            .map_err(|_| ExecutionError::InvalidRequest("invalid local Write arguments".into()))?;
        let path = parse_path(&args.path)?;
        self.fs
            .write_file_atomic(path, args.content.as_bytes())
            .map_err(io_error)?;
        let result = ExecutionResult {
            operation_id: request.operation_id.clone(),
            output_json: json!({
                "path": args.path,
                "bytesWritten": args.content.len(),
            })
            .to_string(),
        };
        result.validate()?;
        Ok(result)
    }

    fn cancel(&mut self, _operation_id: &str) -> Result<(), ExecutionError> {
        Err(ExecutionError::InvalidRequest(
            "bounded local Write is synchronous and non-cancellable".into(),
        ))
    }
}

pub fn bounded_file_runner(
    root: impl Into<std::path::PathBuf>,
) -> Result<AndroidLocalRunner, ExecutionError> {
    let fs = Arc::new(
        BoundedLocalFs::open(root, DEFAULT_MAX_LOCAL_FILE_BYTES)
            .map_err(|error| ExecutionError::Transport(error.to_string()))?,
    );
    let uploads: UploadRegistry = Arc::new(Mutex::new(BTreeMap::new()));
    let mut runner = AndroidLocalRunner::new();
    for (id, handler) in [
        (
            LOCAL_FILE_READ_CAPABILITY,
            Box::new(ReadHandler { fs: Arc::clone(&fs) }) as Box<dyn LocalCapabilityHandler>,
        ),
        (
            LOCAL_FILE_WRITE_CAPABILITY,
            Box::new(WriteHandler { fs: Arc::clone(&fs) }) as Box<dyn LocalCapabilityHandler>,
        ),
        (
            LOCAL_FILE_UPLOAD_START_CAPABILITY,
            Box::new(UploadStartHandler { fs: Arc::clone(&fs), uploads: Arc::clone(&uploads) }) as Box<dyn LocalCapabilityHandler>,
        ),
        (
            LOCAL_FILE_UPLOAD_CHUNK_CAPABILITY,
            Box::new(UploadChunkHandler { fs: Arc::clone(&fs), uploads: Arc::clone(&uploads) }) as Box<dyn LocalCapabilityHandler>,
        ),
        (
            LOCAL_FILE_UPLOAD_COMMIT_CAPABILITY,
            Box::new(UploadCommitHandler { fs, uploads }) as Box<dyn LocalCapabilityHandler>,
        ),
    ] {
        runner.register(
            ExecutionCapability {
                id: id.into(),
                target: ExecutionTarget::AndroidLocal,
                cancellable: false,
                supports_streaming: false,
            },
            handler,
        )?;
    }
    Ok(runner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-local-file-handlers-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn request(operation: &str, capability: &str, input_json: String) -> ExecutionRequest {
        ExecutionRequest {
            operation_id: operation.into(),
            capability_id: capability.into(),
            input_json,
            timeout_ms: 10_000,
        }
    }

    #[test]
    fn production_factory_registers_only_explicit_bounded_file_capabilities() {
        let root = root("factory");
        let runner = bounded_file_runner(&root).unwrap();
        let mut ids = runner
            .advertised_capabilities()
            .into_iter()
            .map(|capability| capability.id)
            .collect::<Vec<_>>();
        ids.sort();
        assert_eq!(
            ids,
            vec![
                LOCAL_FILE_READ_CAPABILITY.to_string(),
                LOCAL_FILE_UPLOAD_CHUNK_CAPABILITY.to_string(),
                LOCAL_FILE_UPLOAD_COMMIT_CAPABILITY.to_string(),
                LOCAL_FILE_UPLOAD_START_CAPABILITY.to_string(),
                LOCAL_FILE_WRITE_CAPABILITY.to_string()
            ]
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn write_then_ranged_read_stays_inside_root() {
        let root = root("roundtrip");
        let mut runner = bounded_file_runner(&root).unwrap();
        runner
            .execute(request(
                "write-1",
                LOCAL_FILE_WRITE_CAPABILITY,
                json!({"path":"notes/a.txt","content":"0123456789"}).to_string(),
            ))
            .unwrap();
        let read = runner
            .execute(request(
                "read-1",
                LOCAL_FILE_READ_CAPABILITY,
                json!({"path":"notes/a.txt","offset":3,"limit":4}).to_string(),
            ))
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&read.output_json).unwrap();
        assert_eq!(value["content"], "3456");
        assert_eq!(value["eof"], false);
        assert!(
            runner
                .execute(request(
                    "read-escape",
                    LOCAL_FILE_READ_CAPABILITY,
                    json!({"path":"../escape.txt"}).to_string(),
                ))
                .is_err()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn staged_upload_commits_only_complete_digest_matched_transfer() {
        let root = root("staged-success");
        let mut runner = bounded_file_runner(&root).unwrap();
        let content = b"hello staged upload";
        let digest = sha256_hex(content);
        runner.execute(request("start-1", LOCAL_FILE_UPLOAD_START_CAPABILITY, json!({
            "transferId":"transfer-1","path":"files/a.bin","totalBytes":content.len(),"sha256":digest
        }).to_string())).unwrap();
        runner.execute(request("chunk-1", LOCAL_FILE_UPLOAD_CHUNK_CAPABILITY, json!({
            "transferId":"transfer-1","path":"files/a.bin","offset":0,"bytesBase64":BASE64.encode(content)
        }).to_string())).unwrap();
        let committed = runner.execute(request("commit-1", LOCAL_FILE_UPLOAD_COMMIT_CAPABILITY, json!({
            "transferId":"transfer-1","path":"files/a.bin"
        }).to_string())).unwrap();
        let value: serde_json::Value = serde_json::from_str(&committed.output_json).unwrap();
        assert_eq!(value["sha256"], digest);
        assert_eq!(stdfs::read(root.join("files/a.bin")).unwrap(), content);
        let _ = stdfs::remove_dir_all(root);
    }

    #[test]
    fn staged_upload_rejects_duplicate_out_of_order_and_digest_mismatch() {
        let root = root("staged-failures");
        let mut runner = bounded_file_runner(&root).unwrap();
        let expected = sha256_hex(b"abc");
        runner.execute(request("start-1", LOCAL_FILE_UPLOAD_START_CAPABILITY, json!({
            "transferId":"transfer-1","path":"a.bin","totalBytes":3,"sha256":expected
        }).to_string())).unwrap();
        assert!(runner.execute(request("start-duplicate", LOCAL_FILE_UPLOAD_START_CAPABILITY, json!({
            "transferId":"transfer-1","path":"a.bin","totalBytes":3,"sha256":expected
        }).to_string())).is_err());
        assert!(runner.execute(request("chunk-stale", LOCAL_FILE_UPLOAD_CHUNK_CAPABILITY, json!({
            "transferId":"transfer-1","path":"a.bin","offset":1,"bytesBase64":BASE64.encode(b"abc")
        }).to_string())).is_err());
        assert!(!root.join("a.bin").exists());

        runner.execute(request("start-2", LOCAL_FILE_UPLOAD_START_CAPABILITY, json!({
            "transferId":"transfer-2","path":"b.bin","totalBytes":3,"sha256":sha256_hex(b"xyz")
        }).to_string())).unwrap();
        runner.execute(request("chunk-2", LOCAL_FILE_UPLOAD_CHUNK_CAPABILITY, json!({
            "transferId":"transfer-2","path":"b.bin","offset":0,"bytesBase64":BASE64.encode(b"abc")
        }).to_string())).unwrap();
        assert!(runner.execute(request("commit-2", LOCAL_FILE_UPLOAD_COMMIT_CAPABILITY, json!({
            "transferId":"transfer-2","path":"b.bin"
        }).to_string())).is_err());
        assert!(!root.join("b.bin").exists());
        let _ = stdfs::remove_dir_all(root);
    }

    #[test]
    fn staged_upload_expected_version_and_overwrite_fail_closed() {
        let root = root("staged-version");
        stdfs::create_dir_all(&root).unwrap();
        stdfs::write(root.join("existing.bin"), b"old").unwrap();
        let mut runner = bounded_file_runner(&root).unwrap();
        assert!(runner.execute(request("start-no-overwrite", LOCAL_FILE_UPLOAD_START_CAPABILITY, json!({
            "transferId":"transfer-no-overwrite","path":"existing.bin","totalBytes":3,"sha256":sha256_hex(b"new"),"overwrite":false
        }).to_string())).is_err());
        assert!(runner.execute(request("start-wrong-version", LOCAL_FILE_UPLOAD_START_CAPABILITY, json!({
            "transferId":"transfer-wrong-version","path":"existing.bin","totalBytes":3,"sha256":sha256_hex(b"new"),"expectedVersion":sha256_hex(b"other")
        }).to_string())).is_err());
        let _ = stdfs::remove_dir_all(root);
    }

}
