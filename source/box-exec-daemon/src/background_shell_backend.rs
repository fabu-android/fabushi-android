use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use fabushi_android_shared::{ExecutionError, ExecutionRequest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::server::{
    RemoteBackendCancelOutcome, RemoteBackendReconcileOutcome, RemoteBackendStartOutcome,
    RemoteExecutionBackend, RemoteRequestIdentity,
};

pub const BACKGROUND_SHELL_CAPABILITY: &str = "background-shell";
const MAX_COMMAND_BYTES: usize = 32 * 1024;
const MAX_TERMINAL_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct BackgroundShellConfig {
    pub workspace_root: PathBuf,
    pub terminals_root: PathBuf,
}

impl BackgroundShellConfig {
    pub fn validate(self) -> Result<Self, ExecutionError> {
        let workspace_root = canonical_dir(&self.workspace_root, "background-shell workspace")?;
        fs::create_dir_all(&self.terminals_root).map_err(io_error)?;
        let terminals_root = fs::canonicalize(&self.terminals_root).map_err(io_error)?;
        Ok(Self { workspace_root, terminals_root })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct BackgroundShellRequest {
    command: String,
    #[serde(default = "default_working_directory")]
    working_directory: String,
}

fn default_working_directory() -> String {
    "/workspace".into()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundShellResult {
    pub shell_id: String,
    pub output_path: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub detail: Option<String>,
}

#[derive(Clone)]
struct LiveShell {
    cancelled: Arc<AtomicBool>,
}

pub struct ProductionBackgroundShellBackend {
    config: BackgroundShellConfig,
    live: Arc<Mutex<BTreeMap<String, LiveShell>>>,
}

impl ProductionBackgroundShellBackend {
    pub fn open(config: BackgroundShellConfig) -> Result<Self, ExecutionError> {
        Ok(Self {
            config: config.validate()?,
            live: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    fn parse(
        &self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<(BackgroundShellRequest, String, PathBuf, PathBuf), ExecutionError> {
        if request.capability_id != BACKGROUND_SHELL_CAPABILITY {
            return Err(ExecutionError::CapabilityUnavailable(format!(
                "background-shell backend rejects capability {}",
                request.capability_id
            )));
        }
        let parsed: BackgroundShellRequest = serde_json::from_str(&request.input_json)
            .map_err(|error| ExecutionError::InvalidRequest(format!(
                "invalid background-shell request: {error}"
            )))?;
        let command = parsed.command.trim();
        if command.is_empty()
            || command.len() > MAX_COMMAND_BYTES
            || command.as_bytes().contains(&0)
        {
            return Err(ExecutionError::InvalidRequest(
                "background-shell command is empty, contains NUL, or exceeds 32 KiB".into(),
            ));
        }
        let cwd = resolve_workspace_path(&self.config.workspace_root, &parsed.working_directory)?;
        if !cwd.is_dir() {
            return Err(ExecutionError::InvalidRequest(
                "background-shell working directory does not exist".into(),
            ));
        }
        let shell_id = deterministic_shell_id(identity);
        let terminal = self.config.terminals_root.join(format!("{shell_id}.txt"));
        Ok((parsed, shell_id, cwd, terminal))
    }

    fn observe(
        &self,
        shell_id: &str,
        terminal: &Path,
    ) -> Result<Option<BackgroundShellResult>, ExecutionError> {
        if !terminal.exists() {
            return Ok(None);
        }
        let metadata = fs::metadata(terminal).map_err(io_error)?;
        if metadata.len() > MAX_TERMINAL_BYTES {
            return Err(ExecutionError::Transport(
                "background-shell terminal exceeded 8 MiB safety limit".into(),
            ));
        }
        let content = fs::read_to_string(terminal).map_err(io_error)?;
        let Some(exit_code) = parse_exit_code(&content) else {
            return Ok(None);
        };
        Ok(Some(BackgroundShellResult {
            shell_id: shell_id.to_string(),
            output_path: terminal.to_string_lossy().to_string(),
            status: if exit_code == 0 { "success" } else { "error" }.into(),
            exit_code: Some(exit_code),
            detail: terminal_tail(&content),
        }))
    }

    fn spawn(
        &mut self,
        identity: &RemoteRequestIdentity,
        parsed: &BackgroundShellRequest,
        shell_id: &str,
        cwd: &Path,
        terminal: &Path,
    ) -> Result<RemoteBackendStartOutcome, ExecutionError> {
        if let Some(settled) = self.observe(shell_id, terminal)? {
            return completed(settled);
        }
        if terminal.exists() {
            // A prior process crossed the spawn boundary but has no terminal footer.
            // Never launch it again; only the original live owner may still settle it.
            return Ok(RemoteBackendStartOutcome::Accepted);
        }

        let mut command = Command::new("/bin/sh");
        command
            .arg("-lc")
            .arg(&parsed.command)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|error| {
            ExecutionError::Transport(format!("spawn background-shell: {error}"))
        })?;
        let pid = child.id();
        let stdout = child.stdout.take().ok_or_else(|| {
            ExecutionError::Transport("background-shell stdout pipe unavailable".into())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            ExecutionError::Transport("background-shell stderr pipe unavailable".into())
        })?;

        let started_at_ms = now_ms();
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(terminal)
            .map_err(io_error)?;
        write!(
            file,
            "---\npid: {pid}\ncwd: {}\ncommand: {}\nstatus: running\nstarted_at_ms: {started_at_ms}\n---\n",
            serde_json::to_string(&parsed.working_directory).unwrap_or_else(|_| "\"\"".into()),
            serde_json::to_string(&parsed.command).unwrap_or_else(|_| "\"\"".into()),
        )
        .map_err(io_error)?;
        file.flush().map_err(io_error)?;

        let writer = Arc::new(Mutex::new(
            OpenOptions::new().append(true).open(terminal).map_err(io_error)?,
        ));
        let stdout_worker = spawn_output_copy(stdout, Arc::clone(&writer));
        let stderr_worker = spawn_output_copy(stderr, Arc::clone(&writer));
        let cancelled = Arc::new(AtomicBool::new(false));
        let live_key = identity.operation_id.clone();
        {
            let mut live = self.live.lock().map_err(|_| {
                ExecutionError::Transport("background-shell live registry poisoned".into())
            })?;
            if live.insert(
                live_key.clone(),
                LiveShell {
                    cancelled: Arc::clone(&cancelled),
                },
            ).is_some()
            {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ExecutionError::InvalidRequest(
                    "background-shell operation already has a live owner".into(),
                ));
            }
        }

        let live = Arc::clone(&self.live);
        let monitor = thread::Builder::new()
            .name(format!("fabushi-remote-shell-{shell_id}"))
            .spawn(move || {
                let status = loop {
                    if cancelled.load(Ordering::Acquire) {
                        let _ = child.kill();
                        break child.wait().ok();
                    }
                    match child.try_wait() {
                        Ok(Some(status)) => break Some(status),
                        Ok(None) => thread::sleep(std::time::Duration::from_millis(100)),
                        Err(_) => break child.wait().ok(),
                    }
                };
                let _ = stdout_worker.join();
                let _ = stderr_worker.join();
                let exit_code = status.and_then(|value| value.code()).unwrap_or(1);
                if let Ok(mut writer) = writer.lock() {
                    let _ = write!(
                        writer,
                        "\n---\nexit_code: {exit_code}\nelapsed_ms: {}\nended_at_ms: {}\n---\n",
                        now_ms().saturating_sub(started_at_ms),
                        now_ms(),
                    );
                    let _ = writer.flush();
                }
                if let Ok(mut live) = live.lock() {
                    live.remove(&live_key);
                }
            });
        if let Err(error) = monitor {
            return Ok(RemoteBackendStartOutcome::OutcomeUnknown {
                reason: format!(
                    "background-shell process crossed spawn boundary but monitor creation failed: {error}"
                ),
            });
        }

        Ok(RemoteBackendStartOutcome::Accepted)
    }
}

impl RemoteExecutionBackend for ProductionBackgroundShellBackend {
    fn start(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendStartOutcome, ExecutionError> {
        let (parsed, shell_id, cwd, terminal) = self.parse(identity, request)?;
        self.spawn(identity, &parsed, &shell_id, &cwd, &terminal)
    }

    fn reconcile(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
        let (_, shell_id, _, terminal) = self.parse(identity, request)?;
        match self.observe(&shell_id, &terminal)? {
            Some(result) => Ok(RemoteBackendReconcileOutcome::Completed {
                output_json: serde_json::to_string(&result).map_err(json_error)?,
            }),
            None => Ok(RemoteBackendReconcileOutcome::Pending),
        }
    }

    fn cancel(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendCancelOutcome, ExecutionError> {
        let (_, shell_id, _, terminal) = self.parse(identity, request)?;
        if self.observe(&shell_id, &terminal)?.is_some() {
            return Ok(RemoteBackendCancelOutcome::Confirmed);
        }
        let cancelled = self
            .live
            .lock()
            .map_err(|_| ExecutionError::Transport(
                "background-shell live registry poisoned".into()
            ))?
            .get(&identity.operation_id)
            .map(|shell| Arc::clone(&shell.cancelled));
        match cancelled {
            Some(cancelled) => {
                cancelled.store(true, Ordering::Release);
                Ok(RemoteBackendCancelOutcome::Confirmed)
            }
            None => Ok(RemoteBackendCancelOutcome::OutcomeUnknown {
                reason: "background-shell live process owner was lost; terminal has no footer"
                    .into(),
            }),
        }
    }
}

pub fn open_background_shell_service<A: crate::server::RemoteExecutionAuthorizer>(
    journal_path: impl Into<PathBuf>,
    authorizer: A,
    config: BackgroundShellConfig,
    now_ms: u64,
) -> Result<
    crate::server::RemoteExecutionService<A, ProductionBackgroundShellBackend>,
    ExecutionError,
> {
    crate::server::RemoteExecutionService::open(
        journal_path,
        authorizer,
        ProductionBackgroundShellBackend::open(config)?,
        now_ms,
    )
}

fn completed(
    result: BackgroundShellResult,
) -> Result<RemoteBackendStartOutcome, ExecutionError> {
    Ok(RemoteBackendStartOutcome::Completed {
        output_json: serde_json::to_string(&result).map_err(json_error)?,
    })
}

fn deterministic_shell_id(identity: &RemoteRequestIdentity) -> String {
    let material = format!(
        "{}\n{}\n{}\n{}",
        identity.account_fence, identity.account_epoch, identity.device_id, identity.operation_id
    );
    let digest = Sha256::digest(material.as_bytes());
    format!("remote-{}", hex_prefix(&digest, 24))
}

fn hex_prefix(bytes: &[u8], count: usize) -> String {
    bytes
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .take(count)
        .map(|nibble| char::from(b"0123456789abcdef"[usize::from(nibble)]))
        .collect()
}

fn resolve_workspace_path(root: &Path, requested: &str) -> Result<PathBuf, ExecutionError> {
    let requested = requested.trim();
    let relative = if requested.is_empty() || requested == "/workspace" {
        Path::new("")
    } else if let Some(relative) = requested.strip_prefix("/workspace/") {
        Path::new(relative)
    } else if Path::new(requested).is_absolute() {
        return Err(ExecutionError::InvalidRequest(
            "background-shell working directory must stay under /workspace".into(),
        ));
    } else {
        Path::new(requested)
    };
    if relative
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(ExecutionError::InvalidRequest(
            "background-shell working directory traversal is forbidden".into(),
        ));
    }
    let candidate = root.join(relative);
    let canonical = fs::canonicalize(&candidate).map_err(io_error)?;
    if canonical != root && !canonical.starts_with(root) {
        return Err(ExecutionError::InvalidRequest(
            "background-shell working directory escaped workspace root".into(),
        ));
    }
    Ok(canonical)
}

fn canonical_dir(path: &Path, label: &str) -> Result<PathBuf, ExecutionError> {
    let canonical = fs::canonicalize(path).map_err(io_error)?;
    if !canonical.is_dir() {
        return Err(ExecutionError::InvalidRequest(format!("{label} is not a directory")));
    }
    Ok(canonical)
}

fn parse_exit_code(content: &str) -> Option<i32> {
    let marker = content.rfind("\n---\n")?;
    let footer = &content[marker + 5..];
    footer
        .lines()
        .find_map(|line| line.strip_prefix("exit_code:"))
        .and_then(|value| value.trim().parse::<i32>().ok())
}

fn terminal_tail(content: &str) -> Option<String> {
    let body = content
        .rfind("\n---\n")
        .map(|index| &content[..index])
        .unwrap_or(content);
    let tail = body.chars().rev().take(4096).collect::<String>();
    let tail = tail.chars().rev().collect::<String>();
    let tail = tail.trim();
    (!tail.is_empty()).then(|| tail.to_string())
}

fn spawn_output_copy<R: Read + Send + 'static>(
    mut reader: R,
    writer: Arc<Mutex<File>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if let Ok(mut writer) = writer.lock() {
                        let _ = writer.write_all(&buffer[..count]);
                        let _ = writer.flush();
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn io_error(error: std::io::Error) -> ExecutionError {
    ExecutionError::Transport(error.to_string())
}

fn json_error(error: serde_json::Error) -> ExecutionError {
    ExecutionError::Transport(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::RemoteExecutionBackend;
    use tempfile::tempdir;

    fn identity(operation_id: &str) -> RemoteRequestIdentity {
        RemoteRequestIdentity {
            credential_id: "credential".into(),
            operation_id: operation_id.into(),
            request_id: format!("request-{operation_id}"),
            account_fence: "account".into(),
            account_epoch: 7,
            permission_grant_id: "grant".into(),
            device_id: "device".into(),
        }
    }

    fn request(operation_id: &str, command: &str) -> ExecutionRequest {
        ExecutionRequest {
            operation_id: operation_id.into(),
            capability_id: BACKGROUND_SHELL_CAPABILITY.into(),
            input_json: serde_json::json!({
                "command":command,
                "workingDirectory":"/workspace"
            })
            .to_string(),
            timeout_ms: 30_000,
        }
    }

    fn backend() -> (tempfile::TempDir, ProductionBackgroundShellBackend) {
        let root = tempdir().unwrap();
        let workspace = root.path().join("workspace");
        let terminals = root.path().join("terminals");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&terminals).unwrap();
        let backend = ProductionBackgroundShellBackend::open(BackgroundShellConfig {
            workspace_root: workspace,
            terminals_root: terminals,
        })
        .unwrap();
        (root, backend)
    }

    #[test]
    fn background_shell_settles_through_reconcile_without_respawn() {
        let (_root, mut backend) = backend();
        let identity = identity("op");
        let request = request("op", "printf hello");
        assert_eq!(
            backend.start(&identity, &request).unwrap(),
            RemoteBackendStartOutcome::Accepted
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            match backend.reconcile(&identity, &request).unwrap() {
                RemoteBackendReconcileOutcome::Completed { output_json } => {
                    let result: BackgroundShellResult =
                        serde_json::from_str(&output_json).unwrap();
                    assert_eq!(result.status, "success");
                    assert!(result.detail.unwrap_or_default().contains("hello"));
                    break;
                }
                RemoteBackendReconcileOutcome::Pending => {
                    assert!(std::time::Instant::now() < deadline);
                    thread::sleep(std::time::Duration::from_millis(20));
                }
                other => panic!("unexpected reconcile outcome: {other:?}"),
            }
        }
        assert!(matches!(
            backend.start(&identity, &request).unwrap(),
            RemoteBackendStartOutcome::Completed { .. }
        ));
    }

    #[test]
    fn reopen_with_unsettled_terminal_never_replays_side_effect() {
        let (_root, backend) = backend();
        let identity = identity("lost");
        let request = request("lost", "printf never");
        let (_, shell_id, _, terminal) = backend.parse(&identity, &request).unwrap();
        fs::write(&terminal, "---\nstatus: running\n---\n").unwrap();
        let mut reopened = ProductionBackgroundShellBackend::open(backend.config.clone()).unwrap();
        assert_eq!(
            reopened.start(&identity, &request).unwrap(),
            RemoteBackendStartOutcome::Accepted
        );
        assert_eq!(
            reopened.cancel(&identity, &request).unwrap(),
            RemoteBackendCancelOutcome::OutcomeUnknown {
                reason:
                    "background-shell live process owner was lost; terminal has no footer".into()
            }
        );
        assert_eq!(deterministic_shell_id(&identity), shell_id);
    }

    #[test]
    fn workspace_traversal_and_wrong_capability_fail_closed() {
        let (_root, mut backend) = backend();
        let scope_identity = identity("scope");
        let mut traversal = request("scope", "pwd");
        traversal.input_json = serde_json::json!({
            "command":"pwd",
            "workingDirectory":"/workspace/../outside"
        })
        .to_string();
        assert!(backend.start(&scope_identity, &traversal).is_err());
        let mut wrong = request("scope-2", "pwd");
        wrong.capability_id = "shell".into();
        assert!(matches!(
            backend.start(&identity("scope-2"), &wrong),
            Err(ExecutionError::CapabilityUnavailable(_))
        ));
    }
}
