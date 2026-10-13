use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::extensions::experiments::HostExperimentsExtension;

pub const DEFAULT_PROFILE_DURATION_MS: i64 = 15_000;
pub const DEFAULT_MIN_INTERVAL_MS: i64 = 6 * 60 * 60_000;
pub const DEFAULT_MAX_RETAINED_PROFILES: usize = 3;
pub const DEFAULT_SUSTAINED_PRESSURE_WINDOW_MS: i64 = 150_000;
pub const ANDROID_PRESSURE_PROFILE_PREFIX: &str = "android-pressure-";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PressureCpuProfilerKnobs {
    pub sustained_pressure_window_ms: Option<i64>,
    pub profile_duration_ms: Option<i64>,
    pub min_interval_ms: Option<i64>,
    pub max_retained_profiles: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EffectiveKnobs {
    sustained_pressure_window_ms: i64,
    profile_duration_ms: i64,
    min_interval_ms: i64,
    max_retained_profiles: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ProcessSnapshot {
    cpu_seconds: Option<f64>,
    rss_kib: Option<u64>,
    thread_count: Option<u64>,
}

#[derive(Debug, Clone)]
struct ActiveCapture {
    started_at_ms: i64,
    deadline_ms: i64,
    start: ProcessSnapshot,
}

/// Android replacement for Desktop's OS-process pressure profiler.
///
/// Android production apps cannot safely spawn `perf`, WPR, or macOS `sample`. This owner
/// preserves the Desktop state-machine/product effect instead: sustained event-loop pressure
/// starts a bounded app-private diagnostic capture, the configured capture duration is honored,
/// repeated captures are cooldown-fenced, and only a bounded number of artifacts are retained.
///
/// The artifact intentionally contains process-level counters available inside the Android app
/// sandbox. It never shells out, requires root, or exposes arbitrary filesystem paths to UI.
pub struct AndroidPressureCpuProfiler {
    directory: PathBuf,
    experiments: Arc<HostExperimentsExtension>,
    previous_pressure_at_ms: Option<i64>,
    last_capture_started_at_ms: Option<i64>,
    active: Option<ActiveCapture>,
    disposed: bool,
    fixed_knobs: Option<PressureCpuProfilerKnobs>,
}

impl AndroidPressureCpuProfiler {
    pub fn new(
        directory: impl Into<PathBuf>,
        experiments: Arc<HostExperimentsExtension>,
    ) -> io::Result<Self> {
        let directory = directory.into();
        fs::create_dir_all(&directory)?;
        Ok(Self {
            directory,
            experiments,
            previous_pressure_at_ms: None,
            last_capture_started_at_ms: None,
            active: None,
            disposed: false,
            fixed_knobs: None,
        })
    }

    fn knobs(&self) -> EffectiveKnobs {
        let configured = self.fixed_knobs.unwrap_or_else(|| {
            pressure_knobs_from_dynamic_config(
                &self.experiments.dynamic_config("sand_pressure_cpu_profiler_config"),
            )
        });
        EffectiveKnobs {
            sustained_pressure_window_ms: configured
                .sustained_pressure_window_ms
                .unwrap_or(DEFAULT_SUSTAINED_PRESSURE_WINDOW_MS)
                .max(0),
            profile_duration_ms: configured
                .profile_duration_ms
                .unwrap_or(DEFAULT_PROFILE_DURATION_MS)
                .max(0),
            min_interval_ms: configured
                .min_interval_ms
                .unwrap_or(DEFAULT_MIN_INTERVAL_MS)
                .max(0),
            max_retained_profiles: configured
                .max_retained_profiles
                .unwrap_or(DEFAULT_MAX_RETAINED_PROFILES)
                .max(1),
        }
    }

    pub fn on_pressure(&mut self, now_ms: i64) {
        if self.disposed || self.active.is_some() {
            return;
        }
        let knobs = self.knobs();
        if self
            .last_capture_started_at_ms
            .is_some_and(|last| now_ms.saturating_sub(last) < knobs.min_interval_ms)
        {
            return;
        }
        let sustained = self.previous_pressure_at_ms.is_some_and(|previous| {
            now_ms.saturating_sub(previous) <= knobs.sustained_pressure_window_ms
        });
        if !sustained {
            self.previous_pressure_at_ms = Some(now_ms);
            return;
        }

        self.previous_pressure_at_ms = None;
        self.last_capture_started_at_ms = Some(now_ms);
        self.active = Some(ActiveCapture {
            started_at_ms: now_ms,
            deadline_ms: now_ms.saturating_add(knobs.profile_duration_ms),
            start: process_snapshot(),
        });
    }

    pub fn on_tick(&mut self, now_ms: i64) -> io::Result<Option<PathBuf>> {
        if self.disposed {
            return Ok(None);
        }
        let Some(active) = self.active.as_ref() else {
            return Ok(None);
        };
        if now_ms < active.deadline_ms {
            return Ok(None);
        }
        let active = self.active.take().expect("pressure capture was present");
        let knobs = self.knobs();
        let path = self.write_artifact(now_ms, &active)?;
        self.prune(knobs.max_retained_profiles)?;
        Ok(Some(path))
    }

    pub fn dispose(&mut self) {
        self.disposed = true;
        self.active = None;
        self.previous_pressure_at_ms = None;
    }

    fn write_artifact(&self, ended_at_ms: i64, active: &ActiveCapture) -> io::Result<PathBuf> {
        let path = self.directory.join(format!(
            "{ANDROID_PRESSURE_PROFILE_PREFIX}{}-{}.json",
            std::process::id(),
            active.started_at_ms.max(0)
        ));
        let payload = serde_json::json!({
            "formatVersion": 1,
            "kind": "android-process-pressure-snapshot",
            "startedAtMs": active.started_at_ms,
            "endedAtMs": ended_at_ms,
            "durationMs": ended_at_ms.saturating_sub(active.started_at_ms),
            "start": active.start,
            "end": process_snapshot(),
        });
        let bytes = serde_json::to_vec_pretty(&payload)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        atomic_write(&path, &bytes)?;
        Ok(path)
    }

    fn prune(&self, max_retained: usize) -> io::Result<()> {
        let mut profiles = fs::read_dir(&self.directory)?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name.starts_with(ANDROID_PRESSURE_PROFILE_PREFIX)
                    .then_some((name, entry.path()))
            })
            .collect::<Vec<_>>();
        profiles.sort_by(|left, right| left.0.cmp(&right.0));
        let remove = profiles.len().saturating_sub(max_retained.max(1));
        for (_, path) in profiles.into_iter().take(remove) {
            let _ = fs::remove_file(path);
        }
        Ok(())
    }
}

impl Drop for AndroidPressureCpuProfiler {
    fn drop(&mut self) {
        self.dispose();
    }
}

fn pressure_knobs_from_dynamic_config(
    config: &BTreeMap<String, Value>,
) -> PressureCpuProfilerKnobs {
    PressureCpuProfilerKnobs {
        sustained_pressure_window_ms: config_i64(config, "sustainedPressureWindowMs")
            .or_else(|| config_i64(config, "sustained_pressure_window_ms")),
        profile_duration_ms: config_i64(config, "profileDurationMs")
            .or_else(|| config_i64(config, "profile_duration_ms")),
        min_interval_ms: config_i64(config, "minIntervalMs")
            .or_else(|| config_i64(config, "min_interval_ms")),
        max_retained_profiles: config_i64(config, "maxRetainedProfiles")
            .or_else(|| config_i64(config, "max_retained_profiles"))
            .and_then(|value| usize::try_from(value.max(0)).ok()),
    }
}

fn config_i64(config: &BTreeMap<String, Value>, key: &str) -> Option<i64> {
    config.get(key).and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
            .or_else(|| value.as_f64().map(|value| value.round() as i64))
    })
}

fn process_snapshot() -> ProcessSnapshot {
    ProcessSnapshot {
        cpu_seconds: process_cpu_seconds(),
        rss_kib: process_rss_kib(),
        thread_count: process_thread_count(),
    }
}

#[cfg(unix)]
fn process_cpu_seconds() -> Option<f64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if result != 0 {
        return None;
    }
    let usage = unsafe { usage.assume_init() };
    let user = usage.ru_utime.tv_sec as f64 + usage.ru_utime.tv_usec as f64 / 1_000_000.0;
    let system = usage.ru_stime.tv_sec as f64 + usage.ru_stime.tv_usec as f64 / 1_000_000.0;
    Some(user + system)
}

#[cfg(not(unix))]
fn process_cpu_seconds() -> Option<f64> {
    None
}

fn process_rss_kib() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        let value = line.strip_prefix("VmRSS:")?.split_whitespace().next()?;
        value.parse::<u64>().ok()
    })
}

fn process_thread_count() -> Option<u64> {
    fs::read_dir("/proc/self/task")
        .ok()
        .map(|entries| entries.filter_map(Result::ok).count() as u64)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "profile path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
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

    fn runtime(
        root: &Path,
        knobs: PressureCpuProfilerKnobs,
    ) -> AndroidPressureCpuProfiler {
        let mut profiler =
            AndroidPressureCpuProfiler::new(root, Arc::new(HostExperimentsExtension::default()))
                .unwrap();
        profiler.fixed_knobs = Some(knobs);
        profiler
    }

    #[test]
    fn sustained_pressure_captures_after_duration_and_respects_cooldown() {
        let root = tempfile::tempdir().unwrap();
        let mut profiler = runtime(
            root.path(),
            PressureCpuProfilerKnobs {
                sustained_pressure_window_ms: Some(100),
                profile_duration_ms: Some(20),
                min_interval_ms: Some(1_000),
                ..PressureCpuProfilerKnobs::default()
            },
        );
        profiler.on_pressure(1_000);
        assert!(profiler.on_tick(1_200).unwrap().is_none());
        profiler.on_pressure(1_050);
        assert!(profiler.on_tick(1_069).unwrap().is_none());
        let artifact = profiler.on_tick(1_070).unwrap().unwrap();
        assert!(artifact.exists());

        profiler.on_pressure(1_100);
        profiler.on_pressure(1_150);
        assert!(profiler.on_tick(2_000).unwrap().is_none());
    }

    #[test]
    fn retention_is_bounded_and_artifact_is_process_snapshot() {
        let root = tempfile::tempdir().unwrap();
        let mut profiler = runtime(
            root.path(),
            PressureCpuProfilerKnobs {
                sustained_pressure_window_ms: Some(100),
                profile_duration_ms: Some(0),
                min_interval_ms: Some(0),
                max_retained_profiles: Some(2),
            },
        );
        for base in [1_000, 2_000, 3_000] {
            profiler.on_pressure(base);
            profiler.on_pressure(base + 1);
            let artifact = profiler.on_tick(base + 1).unwrap().unwrap();
            let payload: Value =
                serde_json::from_slice(&fs::read(&artifact).unwrap()).unwrap();
            assert_eq!(
                payload.get("kind").and_then(Value::as_str),
                Some("android-process-pressure-snapshot")
            );
        }
        let retained = fs::read_dir(root.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(ANDROID_PRESSURE_PROFILE_PREFIX)
            })
            .count();
        assert_eq!(retained, 2);
    }

    #[test]
    fn dispose_cancels_inflight_capture_without_artifact() {
        let root = tempfile::tempdir().unwrap();
        let mut profiler = runtime(
            root.path(),
            PressureCpuProfilerKnobs {
                sustained_pressure_window_ms: Some(100),
                profile_duration_ms: Some(20),
                min_interval_ms: Some(0),
                ..PressureCpuProfilerKnobs::default()
            },
        );
        profiler.on_pressure(10);
        profiler.on_pressure(11);
        profiler.dispose();
        assert!(profiler.on_tick(100).unwrap().is_none());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
