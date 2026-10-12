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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AndroidUncleanExitRecord {
    pub started_at_ms: u64,
    pub last_alive_at_ms: u64,
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
        if now_ms == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "session marker time must be positive",
            ));
        }
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let prior = read_marker(&path).ok().map(|marker| AndroidUncleanExitRecord {
            started_at_ms: marker.started_at_ms,
            last_alive_at_ms: marker.last_alive_at_ms.max(marker.started_at_ms),
            crash_seen: marker.crash_seen,
        });
        let marker = DurableSessionMarker {
            started_at_ms: now_ms,
            last_alive_at_ms: now_ms,
            crash_seen: false,
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
        let _ = self.settle_clean();
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
        assert_eq!(prior.started_at_ms, 100);
        assert_eq!(prior.last_alive_at_ms, 170);
        assert!(prior.crash_seen);
        reopened.settle_clean().unwrap();
        assert!(!path.exists());
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
