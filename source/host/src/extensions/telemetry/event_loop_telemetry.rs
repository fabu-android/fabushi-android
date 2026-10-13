use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use super::HostTelemetryProjection;

pub const EVENT_LOOP_RESOLUTION_MS: u64 = 20;
pub const WINDOW_MS: u64 = 60_000;
pub const PRESSURE_P95_MS: f64 = 50.0;
pub const HEARTBEAT_EVERY_N_WINDOWS: u64 = 5;
pub const EVENT_LOOP_EVENT: &str = "sand.host.event_loop";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowEmitArgs {
    pub window_index: u64,
    pub p95_ms: f64,
    pub pressure_p95_ms: Option<f64>,
    pub heartbeat_every_n_windows: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventLoopTrigger {
    Pressure,
    Heartbeat,
}

impl EventLoopTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pressure => "pressure",
            Self::Heartbeat => "heartbeat",
        }
    }
}

pub fn resolve_window_emit(args: WindowEmitArgs) -> Option<EventLoopTrigger> {
    let threshold = args.pressure_p95_ms.unwrap_or(PRESSURE_P95_MS);
    let heartbeat = args
        .heartbeat_every_n_windows
        .unwrap_or(HEARTBEAT_EVERY_N_WINDOWS);

    if args.p95_ms >= threshold {
        return Some(EventLoopTrigger::Pressure);
    }
    if heartbeat != 0 && args.window_index % heartbeat == 0 {
        return Some(EventLoopTrigger::Heartbeat);
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EventLoopWindowReport {
    pub trigger: EventLoopTrigger,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
    pub utilization: f64,
    pub window_ms: u64,
}

fn rounded(value: f64) -> String {
    format!("{:.0}", value.round())
}

pub fn event_loop_window_telemetry(report: EventLoopWindowReport) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::new();
    metadata.insert("trigger".into(), report.trigger.as_str().into());
    metadata.insert("p50_ms".into(), rounded(report.p50_ms));
    metadata.insert("p95_ms".into(), rounded(report.p95_ms));
    metadata.insert("max_ms".into(), rounded(report.max_ms));
    metadata.insert("utilization".into(), format!("{:.3}", report.utilization));
    metadata.insert("window_ms".into(), report.window_ms.to_string());

    HostTelemetryProjection {
        level: Some(match report.trigger {
            EventLoopTrigger::Pressure => "warn",
            EventLoopTrigger::Heartbeat => "info",
        }),
        event: Some(EVENT_LOOP_EVENT),
        metadata,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EventLoopTelemetryConfig {
    pub resolution_ms: u64,
    pub window_ms: u64,
    pub pressure_p95_ms: f64,
    pub heartbeat_every_n_windows: u64,
}

impl Default for EventLoopTelemetryConfig {
    fn default() -> Self {
        Self {
            resolution_ms: EVENT_LOOP_RESOLUTION_MS,
            window_ms: WINDOW_MS,
            pressure_p95_ms: PRESSURE_P95_MS,
            heartbeat_every_n_windows: HEARTBEAT_EVERY_N_WINDOWS,
        }
    }
}

pub type EventLoopTelemetryReporter =
    Arc<dyn Fn(EventLoopWindowReport) + Send + Sync + 'static>;

/// Rust production equivalent of Grok's Node event-loop telemetry owner.
///
/// The frozen Node implementation samples the single JavaScript event loop with
/// perf_hooks. The Mahayana Host has no Node event loop, so this owner samples
/// scheduler overrun on a dedicated low-frequency probe and combines it with
/// normalized process CPU occupancy. The observable window/pressure/heartbeat
/// contract stays identical while the implementation matches the Rust runtime.
pub struct EventLoopTelemetryRuntime {
    stop: mpsc::Sender<()>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

impl EventLoopTelemetryRuntime {
    pub fn start(reporter: EventLoopTelemetryReporter) -> Self {
        Self::start_with_config(EventLoopTelemetryConfig::default(), reporter)
    }

    pub fn start_with_config(
        config: EventLoopTelemetryConfig,
        reporter: EventLoopTelemetryReporter,
    ) -> Self {
        let (stop_tx, stop_rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("mahayana-host-event-loop-telemetry".into())
            .spawn(move || run_sampler(config, reporter, stop_rx))
            .ok();

        Self {
            stop: stop_tx,
            worker: Mutex::new(worker),
        }
    }
}

impl Drop for EventLoopTelemetryRuntime {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Ok(mut worker) = self.worker.lock() {
            if let Some(handle) = worker.take() {
                let _ = handle.join();
            }
        }
    }
}

fn run_sampler(
    config: EventLoopTelemetryConfig,
    reporter: EventLoopTelemetryReporter,
    stop: mpsc::Receiver<()>,
) {
    let resolution_ms = config.resolution_ms.max(1);
    let window_ms = config.window_ms.max(resolution_ms);
    let resolution = Duration::from_millis(resolution_ms);
    let window = Duration::from_millis(window_ms);

    let mut window_started_at = Instant::now();
    let mut next_tick = window_started_at + resolution;
    let mut last_cpu_seconds = process_cpu_seconds();
    let mut delays_ms = Vec::<f64>::new();
    let mut window_index = 0_u64;

    loop {
        let wait = next_tick.saturating_duration_since(Instant::now());
        match stop.recv_timeout(wait) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }

        let now = Instant::now();
        delays_ms.push(now.saturating_duration_since(next_tick).as_secs_f64() * 1_000.0);
        next_tick += resolution;
        if next_tick <= now {
            next_tick = now + resolution;
        }

        let elapsed = now.saturating_duration_since(window_started_at);
        if elapsed < window {
            continue;
        }

        window_index = window_index.saturating_add(1);
        let p50_ms = percentile(&mut delays_ms, 0.50);
        let p95_ms = percentile(&mut delays_ms, 0.95);
        let max_ms = delays_ms.iter().copied().fold(0.0_f64, f64::max);
        let current_cpu_seconds = process_cpu_seconds();
        let utilization = normalized_cpu_utilization(
            last_cpu_seconds,
            current_cpu_seconds,
            elapsed.as_secs_f64(),
        );
        last_cpu_seconds = current_cpu_seconds;

        let trigger = resolve_window_emit(WindowEmitArgs {
            window_index,
            p95_ms,
            pressure_p95_ms: Some(config.pressure_p95_ms),
            heartbeat_every_n_windows: Some(config.heartbeat_every_n_windows),
        });
        if let Some(trigger) = trigger {
            reporter(EventLoopWindowReport {
                trigger,
                p50_ms,
                p95_ms,
                max_ms,
                utilization,
                window_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,
            });
        }

        delays_ms.clear();
        window_started_at = now;
    }
}

fn percentile(samples: &mut [f64], quantile: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.sort_by(|left, right| left.total_cmp(right));
    let rank = ((samples.len() - 1) as f64 * quantile.clamp(0.0, 1.0)).ceil() as usize;
    samples[rank.min(samples.len() - 1)]
}

fn normalized_cpu_utilization(
    before: Option<f64>,
    after: Option<f64>,
    elapsed_seconds: f64,
) -> f64 {
    if elapsed_seconds <= 0.0 || !elapsed_seconds.is_finite() {
        return 0.0;
    }
    let Some(delta) = before
        .zip(after)
        .map(|(before, after)| (after - before).max(0.0))
    else {
        return 0.0;
    };
    (delta / elapsed_seconds).clamp(0.0, 1.0)
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
