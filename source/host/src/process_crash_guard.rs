use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessCrashKind {
    UncaughtException,
    UnhandledRejection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessCrashRecord {
    pub scope: String,
    pub kind: ProcessCrashKind,
    pub message: String,
    pub kept_alive: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct DurableSessionMarker {
    started_at_ms: u64,
    last_alive_at_ms: u64,
    crash_seen: bool,
    #[serde(default = "unknown_session_version")]
    app_version: String,
}

pub const ANDROID_SESSION_MARKER_ORPHAN_BOUND_MS: u64 = 30 * 24 * 60 * 60 * 1_000;
const ANDROID_SESSION_UPTIME_BUCKET_CEILINGS_MS: [u64; 6] = [
    60_000,
    300_000,
    1_800_000,
    21_600_000,
    86_400_000,
    ANDROID_SESSION_MARKER_ORPHAN_BOUND_MS,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AndroidUncleanExitKind {
    UncleanExit,
    MarkerOrphaned,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AndroidUncleanExitRecord {
    pub kind: AndroidUncleanExitKind,
    pub started_at_ms: u64,
    pub last_alive_at_ms: u64,
    pub previous_uptime_bucket_ms: u64,
    pub previous_version: String,
    pub same_version: bool,
    pub crash_seen: bool,
}

pub struct AndroidUncleanExitMarker {
    path: PathBuf,
    marker: DurableSessionMarker,
    armed: bool,
}

impl AndroidUncleanExitMarker {
    pub fn open(
        path: impl Into<PathBuf>,
        now_ms: u64,
    ) -> io::Result<(Self, Option<AndroidUncleanExitRecord>)> {
        Self::open_with_version(path, now_ms, "unknown")
    }

    pub fn open_with_version(
        path: impl Into<PathBuf>,
        now_ms: u64,
        app_version: &str,
    ) -> io::Result<(Self, Option<AndroidUncleanExitRecord>)> {
        if now_ms == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "session marker time must be positive",
            ));
        }
        let current_version = normalize_session_version(app_version);
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let prior = read_marker(&path).ok().map(|marker| {
            let last_alive_at_ms = marker.last_alive_at_ms.max(marker.started_at_ms);
            let age_ms = now_ms.saturating_sub(last_alive_at_ms);
            let previous_version = normalize_session_version(&marker.app_version);
            AndroidUncleanExitRecord {
                kind: if age_ms > ANDROID_SESSION_MARKER_ORPHAN_BOUND_MS {
                    AndroidUncleanExitKind::MarkerOrphaned
                } else {
                    AndroidUncleanExitKind::UncleanExit
                },
                started_at_ms: marker.started_at_ms,
                last_alive_at_ms,
                previous_uptime_bucket_ms: session_uptime_bucket_ms(
                    last_alive_at_ms.saturating_sub(marker.started_at_ms),
                ),
                same_version: previous_version == current_version,
                previous_version,
                crash_seen: marker.crash_seen,
            }
        });
        let marker = DurableSessionMarker {
            started_at_ms: now_ms,
            last_alive_at_ms: now_ms,
            crash_seen: false,
            app_version: current_version,
        };
        persist_marker(&path, &marker)?;
        Ok((Self { path, marker, armed: true }, prior))
    }

    pub fn heartbeat(&mut self, now_ms: u64) -> io::Result<()> {
        if now_ms <= self.marker.last_alive_at_ms {
            return Ok(());
        }
        self.marker.last_alive_at_ms = now_ms;
        persist_marker(&self.path, &self.marker)
    }

    pub fn mark_crash_seen(&mut self, now_ms: u64) -> io::Result<()> {
        self.marker.crash_seen = true;
        self.marker.last_alive_at_ms = self.marker.last_alive_at_ms.max(now_ms);
        persist_marker(&self.path, &self.marker)
    }

    pub fn settle_clean(&mut self) -> io::Result<()> {
        if !self.armed {
            return Ok(());
        }
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        self.armed = false;
        Ok(())
    }
}

impl Drop for AndroidUncleanExitMarker {
    fn drop(&mut self) {
        // A panic/unwind is an unclean termination signal, not a clean Host shutdown.
        // Preserve the durable marker so the next process can reconcile the crash instead of
        // silently deleting the only process-death evidence while unwinding.
        if !std::thread::panicking() {
            let _ = self.settle_clean();
        }
    }
}

pub fn to_error_message(value: impl ToString) -> String {
    value.to_string()
}

pub fn handle_process_crash(
    scope: impl Into<String>,
    kind: ProcessCrashKind,
    value: impl ToString,
) -> ProcessCrashRecord {
    ProcessCrashRecord {
        scope: scope.into(),
        kind,
        message: to_error_message(value),
        kept_alive: true,
    }
}

fn unknown_session_version() -> String {
    "unknown".into()
}

fn normalize_session_version(value: &str) -> String {
    let value = value.trim();
    if value.len() <= 64
        && !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-'))
    {
        value.to_string()
    } else {
        unknown_session_version()
    }
}

fn session_uptime_bucket_ms(uptime_ms: u64) -> u64 {
    ANDROID_SESSION_UPTIME_BUCKET_CEILINGS_MS
        .iter()
        .copied()
        .find(|ceiling| uptime_ms < *ceiling)
        .unwrap_or(ANDROID_SESSION_MARKER_ORPHAN_BOUND_MS)
}

fn read_marker(path: &Path) -> io::Result<DurableSessionMarker> {
    let raw = fs::read_to_string(path)?;
    serde_json::from_str(&raw)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn persist_marker(path: &Path, marker: &DurableSessionMarker) -> io::Result<()> {
    let raw = serde_json::to_vec(marker)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(&raw)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-process-crash-{label}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn crash_guard_records_both_runtime_failure_classes_without_forcing_exit() {
        let exception = handle_process_crash("host", ProcessCrashKind::UncaughtException, "boom");
        let rejection = handle_process_crash("host", ProcessCrashKind::UnhandledRejection, "reject");
        assert!(exception.kept_alive);
        assert_eq!(exception.message, "boom");
        assert_eq!(rejection.kind, ProcessCrashKind::UnhandledRejection);
    }

    #[test]
    fn process_death_reopen_reports_prior_session_without_sensitive_payload() {
        let dir = marker_root("reopen");
        let path = dir.join("session.json");
        let _ = fs::remove_dir_all(&dir);
        let (mut first, prior) = AndroidUncleanExitMarker::open(&path, 100).unwrap();
        assert!(prior.is_none());
        first.heartbeat(160).unwrap();
        first.mark_crash_seen(170).unwrap();
        std::mem::forget(first);

        let (mut reopened, prior) = AndroidUncleanExitMarker::open(&path, 200).unwrap();
        let prior = prior.unwrap();
        assert_eq!(prior.kind, AndroidUncleanExitKind::UncleanExit);
        assert_eq!(prior.started_at_ms, 100);
        assert_eq!(prior.last_alive_at_ms, 170);
        assert_eq!(prior.previous_uptime_bucket_ms, 60_000);
        assert_eq!(prior.previous_version, "unknown");
        assert!(prior.same_version);
        assert!(prior.crash_seen);
        reopened.settle_clean().unwrap();
        assert!(!path.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn reopen_preserves_previous_version_and_compares_current_version() {
        let dir = marker_root("version");
        let path = dir.join("session.json");
        let _ = fs::remove_dir_all(&dir);
        let (first, prior) =
            AndroidUncleanExitMarker::open_with_version(&path, 100, "1.2.3").unwrap();
        assert!(prior.is_none());
        std::mem::forget(first);

        let (second, prior) =
            AndroidUncleanExitMarker::open_with_version(&path, 200, "1.2.4").unwrap();
        let prior = prior.expect("prior version should survive process death");
        assert_eq!(prior.previous_version, "1.2.3");
        assert!(!prior.same_version);
        drop(second);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn clean_drop_removes_marker_and_does_not_report_unclean_next_start() {
        let dir = marker_root("clean");
        let path = dir.join("session.json");
        let _ = fs::remove_dir_all(&dir);
        {
            let (_marker, prior) = AndroidUncleanExitMarker::open(&path, 100).unwrap();
            assert!(prior.is_none());
        }
        let (marker, prior) = AndroidUncleanExitMarker::open(&path, 200).unwrap();
        assert!(prior.is_none());
        drop(marker);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn panic_unwind_preserves_marker_for_next_process_reconciliation() {
        let dir = marker_root("panic");
        let path = dir.join("session.json");
        let _ = fs::remove_dir_all(&dir);

        let result = std::panic::catch_unwind({
            let path = path.clone();
            move || {
                let (mut marker, prior) = AndroidUncleanExitMarker::open(&path, 100).unwrap();
                assert!(prior.is_none());
                marker.mark_crash_seen(125).unwrap();
                panic!("simulated host panic");
            }
        });
        assert!(result.is_err());
        assert!(
            path.exists(),
            "panic unwinding must preserve the session marker for cold-start reconciliation"
        );

        let (marker, prior) = AndroidUncleanExitMarker::open(&path, 200).unwrap();
        let prior = prior.expect("next process should observe the panicked Host session");
        assert!(prior.crash_seen);
        assert_eq!(prior.last_alive_at_ms, 125);
        drop(marker);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_prior_marker_is_classified_as_orphaned_and_uptime_is_bucketed() {
        let dir = marker_root("orphan");
        let path = dir.join("session.json");
        let _ = fs::remove_dir_all(&dir);
        let (mut first, prior) = AndroidUncleanExitMarker::open(&path, 100).unwrap();
        assert!(prior.is_none());
        first.heartbeat(86_500).unwrap();
        std::mem::forget(first);

        let observed_at = 86_500 + ANDROID_SESSION_MARKER_ORPHAN_BOUND_MS + 1;
        let (marker, prior) = AndroidUncleanExitMarker::open(&path, observed_at).unwrap();
        let prior = prior.expect("stale marker should be reconciled on reopen");
        assert_eq!(prior.kind, AndroidUncleanExitKind::MarkerOrphaned);
        assert_eq!(prior.previous_uptime_bucket_ms, 86_400_000);
        drop(marker);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_heartbeat_never_moves_last_alive_backwards() {
        let dir = marker_root("stale");
        let path = dir.join("session.json");
        let _ = fs::remove_dir_all(&dir);
        let (mut marker, _) = AndroidUncleanExitMarker::open(&path, 100).unwrap();
        marker.heartbeat(150).unwrap();
        marker.heartbeat(120).unwrap();
        let stored = read_marker(&path).unwrap();
        assert_eq!(stored.last_alive_at_ms, 150);
        drop(marker);
        let _ = fs::remove_dir_all(dir);
    }
}
