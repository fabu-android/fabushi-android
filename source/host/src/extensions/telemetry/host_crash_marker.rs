use serde::{Deserialize, Serialize};
use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

pub const HOST_CRASH_EXIT_SIGNALS: &[&str] = &[
    "SIGABRT", "SIGALRM", "SIGBUS", "SIGFPE", "SIGHUP", "SIGILL", "SIGINT", "SIGKILL",
    "SIGPIPE", "SIGQUIT", "SIGSEGV", "SIGTERM", "SIGTRAP", "SIGUSR1", "SIGUSR2", "SIGXCPU",
    "SIGXFSZ",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostCrashErrorClass {
    SignalExit,
    NonzeroExit,
    UnexpectedCleanExit,
    UnobservedExit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostCrashMarker {
    pub schema_version: u8,
    pub error_class: HostCrashErrorClass,
    pub exit_signal: String,
    pub crashed_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostCrashMarkerRead {
    Present(String),
    Absent,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostCrashDeleteResult {
    Deleted,
    Unavailable,
}

pub trait HostCrashMarkerStore {
    fn read(&self) -> HostCrashMarkerRead;
    fn delete(&self) -> HostCrashDeleteResult;
}

#[derive(Clone, Debug)]
pub struct FileHostCrashMarkerStore {
    path: PathBuf,
}

impl FileHostCrashMarkerStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl HostCrashMarkerStore for FileHostCrashMarkerStore {
    fn read(&self) -> HostCrashMarkerRead {
        match fs::read_to_string(&self.path) {
            Ok(raw) => HostCrashMarkerRead::Present(raw),
            Err(error) if error.kind() == io::ErrorKind::NotFound => HostCrashMarkerRead::Absent,
            Err(_) => HostCrashMarkerRead::Unavailable,
        }
    }

    fn delete(&self) -> HostCrashDeleteResult {
        match fs::remove_file(&self.path) {
            Ok(()) => HostCrashDeleteResult::Deleted,
            Err(error) if error.kind() == io::ErrorKind::NotFound => HostCrashDeleteResult::Deleted,
            Err(_) => HostCrashDeleteResult::Unavailable,
        }
    }
}

pub fn is_fatal_exit_signal(value: &str) -> bool {
    HOST_CRASH_EXIT_SIGNALS.contains(&value)
}

pub fn parse_host_crash_marker(raw: &str) -> Option<HostCrashMarker> {
    let marker: HostCrashMarker = serde_json::from_str(raw).ok()?;
    if marker.schema_version != 1 {
        return None;
    }

    match marker.error_class {
        HostCrashErrorClass::SignalExit => {
            if !is_fatal_exit_signal(&marker.exit_signal)
                || marker.started_at_ms.is_none()
                || marker.uptime_ms.is_none()
            {
                return None;
            }
        }
        HostCrashErrorClass::NonzeroExit | HostCrashErrorClass::UnexpectedCleanExit => {
            if marker.exit_signal != "none"
                || marker.started_at_ms.is_none()
                || marker.uptime_ms.is_none()
            {
                return None;
            }
        }
        HostCrashErrorClass::UnobservedExit => {
            if marker.exit_signal != "unknown" {
                return None;
            }
        }
    }

    Some(marker)
}

pub fn host_crash_marker_metadata(marker: &HostCrashMarker) -> Vec<(String, String)> {
    let mut metadata = vec![
        (
            "error_class".into(),
            match marker.error_class {
                HostCrashErrorClass::SignalExit => "signal_exit",
                HostCrashErrorClass::NonzeroExit => "nonzero_exit",
                HostCrashErrorClass::UnexpectedCleanExit => "unexpected_clean_exit",
                HostCrashErrorClass::UnobservedExit => "unobserved_exit",
            }
            .into(),
        ),
        ("exit_signal".into(), marker.exit_signal.clone()),
        ("crashed_at_ms".into(), marker.crashed_at_ms.to_string()),
    ];
    if let Some(value) = marker.started_at_ms {
        metadata.push(("started_at_ms".into(), value.to_string()));
    }
    if let Some(value) = marker.uptime_ms {
        metadata.push(("uptime_ms".into(), value.to_string()));
    }
    metadata
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteIfUnchanged {
    Failed,
    Deleted,
    Changed,
}

pub fn delete_if_unchanged(
    store: &impl HostCrashMarkerStore,
    expected_raw: &str,
) -> DeleteIfUnchanged {
    match store.read() {
        HostCrashMarkerRead::Unavailable => DeleteIfUnchanged::Failed,
        HostCrashMarkerRead::Absent => DeleteIfUnchanged::Deleted,
        HostCrashMarkerRead::Present(current) if current != expected_raw => DeleteIfUnchanged::Changed,
        HostCrashMarkerRead::Present(_) => match store.delete() {
            HostCrashDeleteResult::Deleted => DeleteIfUnchanged::Deleted,
            HostCrashDeleteResult::Unavailable => DeleteIfUnchanged::Failed,
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostCrashForwardOutcome {
    Deferred,
    Absent,
    DeleteDeferred,
    Pending,
    Delivered,
    ParseError,
}

pub fn forward_host_crash_marker_with(
    store: &impl HostCrashMarkerStore,
    mut emit: impl FnMut(&HostCrashMarker) -> bool,
) -> HostCrashForwardOutcome {
    let raw = match store.read() {
        HostCrashMarkerRead::Unavailable => return HostCrashForwardOutcome::Deferred,
        HostCrashMarkerRead::Absent => return HostCrashForwardOutcome::Absent,
        HostCrashMarkerRead::Present(raw) => raw,
    };

    let marker = match parse_host_crash_marker(&raw) {
        Some(marker) => marker,
        None => {
            return match delete_if_unchanged(store, &raw) {
                DeleteIfUnchanged::Failed => HostCrashForwardOutcome::DeleteDeferred,
                DeleteIfUnchanged::Changed => HostCrashForwardOutcome::Pending,
                DeleteIfUnchanged::Deleted => HostCrashForwardOutcome::ParseError,
            }
        }
    };

    if !emit(&marker) {
        return HostCrashForwardOutcome::Deferred;
    }

    match delete_if_unchanged(store, &raw) {
        DeleteIfUnchanged::Failed => HostCrashForwardOutcome::DeleteDeferred,
        DeleteIfUnchanged::Changed => HostCrashForwardOutcome::Pending,
        DeleteIfUnchanged::Deleted => HostCrashForwardOutcome::Delivered,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct MemoryStore {
        raw: Arc<Mutex<Option<String>>>,
        available: bool,
    }

    impl MemoryStore {
        fn new(raw: Option<String>) -> Self {
            Self {
                raw: Arc::new(Mutex::new(raw)),
                available: true,
            }
        }
    }

    impl HostCrashMarkerStore for MemoryStore {
        fn read(&self) -> HostCrashMarkerRead {
            if !self.available {
                return HostCrashMarkerRead::Unavailable;
            }
            self.raw
                .lock()
                .unwrap()
                .clone()
                .map(HostCrashMarkerRead::Present)
                .unwrap_or(HostCrashMarkerRead::Absent)
        }

        fn delete(&self) -> HostCrashDeleteResult {
            if !self.available {
                return HostCrashDeleteResult::Unavailable;
            }
            *self.raw.lock().unwrap() = None;
            HostCrashDeleteResult::Deleted
        }
    }

    fn valid_signal_marker() -> String {
        serde_json::json!({
            "schemaVersion": 1,
            "errorClass": "signal_exit",
            "exitSignal": "SIGSEGV",
            "startedAtMs": 100,
            "crashedAtMs": 150,
            "uptimeMs": 50
        })
        .to_string()
    }

    #[test]
    fn parser_accepts_only_valid_error_class_signal_combinations() {
        let parsed = parse_host_crash_marker(&valid_signal_marker()).unwrap();
        assert_eq!(parsed.error_class, HostCrashErrorClass::SignalExit);

        let invalid = serde_json::json!({
            "schemaVersion": 1,
            "errorClass": "signal_exit",
            "exitSignal": "none",
            "startedAtMs": 100,
            "crashedAtMs": 150,
            "uptimeMs": 50
        })
        .to_string();
        assert!(parse_host_crash_marker(&invalid).is_none());

        let unobserved = serde_json::json!({
            "schemaVersion": 1,
            "errorClass": "unobserved_exit",
            "exitSignal": "unknown",
            "crashedAtMs": 150
        })
        .to_string();
        assert!(parse_host_crash_marker(&unobserved).is_some());
    }

    #[test]
    fn successful_forward_deletes_only_the_unchanged_marker() {
        let store = MemoryStore::new(Some(valid_signal_marker()));
        assert_eq!(
            forward_host_crash_marker_with(&store, |_| true),
            HostCrashForwardOutcome::Delivered
        );
        assert!(matches!(store.read(), HostCrashMarkerRead::Absent));
    }

    #[test]
    fn newer_crash_marker_is_never_deleted_by_older_forward() {
        let original = valid_signal_marker();
        let store = MemoryStore::new(Some(original.clone()));
        let writer = store.clone();
        let outcome = forward_host_crash_marker_with(&store, |_| {
            *writer.raw.lock().unwrap() = Some(
                serde_json::json!({
                    "schemaVersion": 1,
                    "errorClass": "unobserved_exit",
                    "exitSignal": "unknown",
                    "crashedAtMs": 999
                })
                .to_string(),
            );
            true
        });
        assert_eq!(outcome, HostCrashForwardOutcome::Pending);
        assert!(matches!(store.read(), HostCrashMarkerRead::Present(_)));
    }

    #[test]
    fn invalid_marker_is_removed_without_being_emitted() {
        let store = MemoryStore::new(Some("{invalid".into()));
        let mut emitted = false;
        assert_eq!(
            forward_host_crash_marker_with(&store, |_| {
                emitted = true;
                true
            }),
            HostCrashForwardOutcome::ParseError
        );
        assert!(!emitted);
    }
}
