use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use crate::ports::telemetry::SAND_HOST_LIFECYCLE_PHASES;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostLifecycleError {
    Failed,
    Stalled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostLifecycleCompletion {
    pub phase: String,
    pub plugin_count: Option<u64>,
    pub entry_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostLifecycleReport {
    Completed {
        phase: String,
        plugin_count: Option<u64>,
        entry_count: Option<u64>,
        duration_ms: u64,
    },
    Failed {
        phase: String,
        duration_ms: u64,
        error: HostLifecycleError,
    },
    Stuck {
        phase: String,
        duration_ms: u64,
        error: HostLifecycleError,
    },
}

pub const HOST_LIFECYCLE_STUCK_MS: u64 = 5 * 60_000;

pub trait Disposable: Send {
    fn dispose(&mut self);
}

struct ProductionLifecycleWatchdog {
    stop: Option<mpsc::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Disposable for ProductionLifecycleWatchdog {
    fn dispose(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn production_host_lifecycle_watchdog() -> WatchdogArm {
    Arc::new(|listener| {
        let (stop_tx, stop_rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("sand-host-lifecycle-watchdog".into())
            .spawn(move || {
                if matches!(
                    stop_rx.recv_timeout(Duration::from_millis(HOST_LIFECYCLE_STUCK_MS)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    listener();
                }
            })
            .ok();
        Box::new(ProductionLifecycleWatchdog {
            stop: Some(stop_tx),
            worker,
        })
    })
}

pub type WatchdogArm = Arc<dyn Fn(Box<dyn Fn() + Send + Sync>) -> Box<dyn Disposable> + Send + Sync>;
pub type ReportFn = Arc<dyn Fn(HostLifecycleReport) + Send + Sync>;
pub type NowFn = Arc<dyn Fn() -> f64 + Send + Sync>;

pub struct HostLifecycleProgress {
    phase_index: usize,
    phase_started_at: f64,
    now: NowFn,
    report: ReportFn,
    watchdog: WatchdogArm,
    watchdog_handle: Option<Box<dyn Disposable>>,
}

impl HostLifecycleProgress {
    pub fn new(started_at: f64, now: NowFn, report: ReportFn, watchdog: WatchdogArm) -> Self {
        let mut this = Self {
            phase_index: 0,
            phase_started_at: started_at,
            now,
            report,
            watchdog,
            watchdog_handle: None,
        };
        this.arm_watchdog();
        this
    }

    pub fn complete(&mut self, completion: HostLifecycleCompletion) -> Result<(), String> {
        let phase = self.current_phase().map(str::to_string);
        if phase.as_deref() != Some(completion.phase.as_str()) {
            return Err(format!(
                "Host lifecycle phase {} completed while {} was active",
                completion.phase,
                phase.as_deref().unwrap_or("none")
            ));
        }
        self.dispose_watchdog();
        (self.report)(HostLifecycleReport::Completed {
            phase: completion.phase,
            plugin_count: completion.plugin_count,
            entry_count: completion.entry_count,
            duration_ms: self.elapsed_ms(),
        });
        self.phase_index = self.phase_index.saturating_add(1);
        self.phase_started_at = (self.now)();
        self.arm_watchdog();
        Ok(())
    }

    pub fn fail(&mut self) {
        let Some(phase) = self.current_phase().map(str::to_string) else { return };
        self.dispose_watchdog();
        (self.report)(HostLifecycleReport::Failed {
            phase,
            duration_ms: self.elapsed_ms(),
            error: HostLifecycleError::Failed,
        });
    }

    pub fn current_phase(&self) -> Option<&'static str> {
        SAND_HOST_LIFECYCLE_PHASES.get(self.phase_index).copied()
    }

    pub fn elapsed_ms(&self) -> u64 {
        let elapsed = ((self.now)() - self.phase_started_at).max(0.0).round();
        if elapsed.is_finite() && elapsed > 0.0 { elapsed as u64 } else { 0 }
    }

    fn arm_watchdog(&mut self) {
        let Some(phase) = self.current_phase().map(str::to_string) else { return };
        let report = Arc::clone(&self.report);
        let now = Arc::clone(&self.now);
        let started_at = self.phase_started_at;
        self.watchdog_handle = Some((self.watchdog)(Box::new(move || {
            let elapsed = (now() - started_at).max(0.0).round();
            report(HostLifecycleReport::Stuck {
                phase: phase.clone(),
                duration_ms: if elapsed.is_finite() && elapsed > 0.0 { elapsed as u64 } else { 0 },
                error: HostLifecycleError::Stalled,
            });
        })));
    }

    fn dispose_watchdog(&mut self) {
        if let Some(mut handle) = self.watchdog_handle.take() {
            handle.dispose();
        }
    }
}

impl Drop for HostLifecycleProgress {
    fn drop(&mut self) {
        self.dispose_watchdog();
    }
}
