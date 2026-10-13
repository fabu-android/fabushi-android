use crate::{bounded_fs::BoundedLocalFs, AndroidLocalRunner, LocalCapabilityHandler};
use fabushi_android_shared::{
    ExecutionCapability, ExecutionError, ExecutionRequest, ExecutionResult, ExecutionTarget,
};
use serde::Deserialize;
use serde_json::json;
use std::{path::Path, sync::Arc};

pub const LOCAL_FILE_READ_CAPABILITY: &str = "android.local.file.read";
pub const LOCAL_FILE_WRITE_CAPABILITY: &str = "android.local.file.write";
pub const DEFAULT_MAX_LOCAL_FILE_BYTES: u64 = 100 * 1024 * 1024;
pub const MAX_LOCAL_TOOL_READ_BYTES: usize = 48 * 1024;

#[derive(Clone)]
struct ReadHandler {
    fs: Arc<BoundedLocalFs>,
}

#[derive(Clone)]
struct WriteHandler {
    fs: Arc<BoundedLocalFs>,
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
    let mut runner = AndroidLocalRunner::new();
    for (id, handler) in [
        (
            LOCAL_FILE_READ_CAPABILITY,
            Box::new(ReadHandler { fs: Arc::clone(&fs) }) as Box<dyn LocalCapabilityHandler>,
        ),
        (
            LOCAL_FILE_WRITE_CAPABILITY,
            Box::new(WriteHandler { fs }) as Box<dyn LocalCapabilityHandler>,
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
}
