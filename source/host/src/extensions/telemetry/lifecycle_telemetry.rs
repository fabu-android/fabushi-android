use std::collections::BTreeMap;

use super::HostTelemetryProjection;
use super::host_lifecycle_progress::{HostLifecycleError, HostLifecycleReport};
use super::sand_error_tags::{SandErrorValue, sand_error_tags};

pub const HOST_STARTUP_EVENT: &str = "sand.host.startup";
pub const HOST_LIFECYCLE_EVENT: &str = "sand.host.lifecycle";
pub const DAEMON_PING_EVENT: &str = "sand.box.daemon_ping";
pub const BOX_IMAGE_CHECK_EVENT: &str = "sand.box.image_check";
pub const BOX_BOOT_STAGE_EVENT: &str = "sand.box.boot_stage";
pub const BOX_BOOT_FAILURE_EVENT: &str = "sand.box.boot_failure";
pub const EGRESS_TUNNEL_EVENT: &str = "sand.box.egress_tunnel";
pub const HOST_BOOT_FETCH_EVENT: &str = "sand.box.host_boot_fetch";
pub const EXEC_DAEMON_RESTART_EVENT: &str = "sand.box.exec_daemon_restart";
pub const SUPERVISOR_RESTART_EVENT: &str = "sand.box.supervisor_restart";
pub const COOKIE_PERSIST_EVENT: &str = "sand.box.cookie_persist";
pub const BOX_PROCESS_CRASH_EVENT: &str = "sand.box.process_crash";

fn projection(
    level: &'static str,
    event: &'static str,
    metadata: BTreeMap<String, String>,
) -> HostTelemetryProjection {
    HostTelemetryProjection {
        level: Some(level),
        event: Some(event),
        metadata,
    }
}

pub fn host_startup_telemetry(
    mut metadata: BTreeMap<String, String>,
    host_built_at_ms: &str,
) -> HostTelemetryProjection {
    // Frozen JS spreads caller metadata after host_built_at_ms, so preserve a
    // caller-provided key rather than overwriting it here.
    metadata
        .entry("host_built_at_ms".into())
        .or_insert_with(|| host_built_at_ms.to_string());
    projection("info", HOST_STARTUP_EVENT, metadata)
}

fn lifecycle_error(error: &HostLifecycleError) -> SandErrorValue {
    SandErrorValue::new(match error {
        HostLifecycleError::Failed => "SAND-E0303",
        HostLifecycleError::Stalled => "SAND-E0302",
    })
}

pub fn host_lifecycle_telemetry(report: &HostLifecycleReport) -> HostTelemetryProjection {
    let (level, phase, outcome, duration_ms, plugin_count, entry_count, error) = match report {
        HostLifecycleReport::Completed {
            phase,
            plugin_count,
            entry_count,
            duration_ms,
        } => (
            "info",
            phase.as_str(),
            "completed",
            *duration_ms,
            *plugin_count,
            *entry_count,
            None,
        ),
        HostLifecycleReport::Failed {
            phase,
            duration_ms,
            error,
        } => (
            "error",
            phase.as_str(),
            "failed",
            *duration_ms,
            None,
            None,
            Some(error),
        ),
        HostLifecycleReport::Stuck {
            phase,
            duration_ms,
            error,
        } => (
            "warn",
            phase.as_str(),
            "stuck",
            *duration_ms,
            None,
            None,
            Some(error),
        ),
    };
    let mut metadata = BTreeMap::from([
        ("phase".into(), phase.to_string()),
        ("outcome".into(), outcome.to_string()),
        ("duration_ms".into(), duration_ms.to_string()),
    ]);
    if outcome == "completed" && phase == "plugin_graph" {
        if let Some(value) = plugin_count {
            metadata.insert("plugin_count".into(), value.to_string());
        }
    }
    if outcome == "completed" && phase == "transcript_read" {
        if let Some(value) = entry_count {
            metadata.insert("entry_count".into(), value.to_string());
        }
    }
    if let Some(error) = error {
        metadata.extend(sand_error_tags(&lifecycle_error(error)));
    }
    projection(level, HOST_LIFECYCLE_EVENT, metadata)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonPingReport {
    pub outcome: String,
    pub attempts: u64,
    pub duration_ms: u64,
    pub unready_duration_ms: u64,
    pub readiness_state: String,
    pub target: String,
    pub cause_summary: Option<String>,
}

pub fn daemon_ping_telemetry(report: &DaemonPingReport) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("outcome".into(), report.outcome.clone()),
        ("attempts".into(), report.attempts.to_string()),
        ("duration_ms".into(), report.duration_ms.to_string()),
        (
            "unready_duration_ms".into(),
            report.unready_duration_ms.to_string(),
        ),
        ("readiness_state".into(), report.readiness_state.clone()),
        ("target".into(), report.target.clone()),
    ]);
    if report.outcome != "ok" {
        if let Some(cause) = &report.cause_summary {
            metadata.insert("cause".into(), cause.clone());
        }
    }
    projection(
        if report.outcome == "ok" { "warn" } else { "error" },
        DAEMON_PING_EVENT,
        metadata,
    )
}

#[derive(Debug, Clone, PartialEq)]
pub enum BoxImageCheckReport {
    Skipped {
        trigger: String,
        duration_ms: u64,
        skip_reason: String,
    },
    Answered {
        trigger: String,
        duration_ms: u64,
    },
    Unanswered {
        trigger: String,
        duration_ms: u64,
    },
    Timeout {
        trigger: String,
        duration_ms: u64,
        error: SandErrorValue,
    },
    Failed {
        trigger: String,
        duration_ms: u64,
        error: SandErrorValue,
    },
}

pub fn box_image_check_telemetry(report: &BoxImageCheckReport) -> HostTelemetryProjection {
    let (level, trigger, outcome, duration_ms, skip_reason, error) = match report {
        BoxImageCheckReport::Skipped {
            trigger,
            duration_ms,
            skip_reason,
        } => (
            "info",
            trigger,
            "skipped",
            *duration_ms,
            Some(skip_reason),
            None,
        ),
        BoxImageCheckReport::Answered { trigger, duration_ms } => {
            ("info", trigger, "answered", *duration_ms, None, None)
        }
        BoxImageCheckReport::Unanswered { trigger, duration_ms } => {
            ("info", trigger, "unanswered", *duration_ms, None, None)
        }
        BoxImageCheckReport::Timeout {
            trigger,
            duration_ms,
            error,
        } => ("warn", trigger, "timeout", *duration_ms, None, Some(error)),
        BoxImageCheckReport::Failed {
            trigger,
            duration_ms,
            error,
        } => ("warn", trigger, "failed", *duration_ms, None, Some(error)),
    };
    let mut metadata = BTreeMap::from([
        ("trigger".into(), trigger.clone()),
        ("outcome".into(), outcome.to_string()),
        ("duration_ms".into(), duration_ms.to_string()),
    ]);
    if let Some(reason) = skip_reason {
        metadata.insert("skip_reason".into(), reason.clone());
    }
    if let Some(error) = error {
        metadata.extend(sand_error_tags(error));
    }
    projection(level, BOX_IMAGE_CHECK_EVENT, metadata)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoxInfrastructureEvent {
    BootStage {
        stage: String,
        duration_ms: u64,
    },
    BootFailure {
        stage: String,
        reason: String,
        duration_ms: u64,
    },
    EgressTunnel {
        outcome: String,
        attempt: u64,
        exit_status: Option<i64>,
        runtime_s: Option<u64>,
    },
    HostBootFetch {
        outcome: String,
        reason: Option<String>,
        duration_ms: u64,
        swap_ms: Option<u64>,
        from_version: Option<String>,
        to_version: Option<String>,
    },
    ExecDaemonRestart {
        restart_attempt: u64,
        runtime_s: u64,
        cause: String,
        exit_status: i64,
    },
    SupervisorRestart {
        restart_attempt: u64,
        runtime_s: u64,
        cause: String,
        exit_status: i64,
    },
    CookiePersist {
        phase: String,
        outcome: String,
        seed_cookies: u64,
        injected: Option<u64>,
        missing_after: Option<u64>,
        attempts: Option<u64>,
    },
    ProcessCrash {
        binary: String,
        signal: String,
        count: u64,
    },
}

fn insert_opt<T: ToString>(metadata: &mut BTreeMap<String, String>, key: &str, value: &Option<T>) {
    if let Some(value) = value {
        metadata.insert(key.into(), value.to_string());
    }
}

pub fn box_infrastructure_telemetry(event: &BoxInfrastructureEvent) -> HostTelemetryProjection {
    match event {
        BoxInfrastructureEvent::BootStage { stage, duration_ms } => projection(
            if stage == "ready" { "warn" } else { "info" },
            BOX_BOOT_STAGE_EVENT,
            BTreeMap::from([
                ("stage".into(), stage.clone()),
                ("duration_ms".into(), duration_ms.to_string()),
            ]),
        ),
        BoxInfrastructureEvent::BootFailure {
            stage,
            reason,
            duration_ms,
        } => projection(
            "error",
            BOX_BOOT_FAILURE_EVENT,
            BTreeMap::from([
                ("stage".into(), stage.clone()),
                ("reason".into(), reason.clone()),
                ("duration_ms".into(), duration_ms.to_string()),
            ]),
        ),
        BoxInfrastructureEvent::EgressTunnel {
            outcome,
            attempt,
            exit_status,
            runtime_s,
        } => {
            let mut metadata = BTreeMap::from([
                ("outcome".into(), outcome.clone()),
                ("attempt".into(), attempt.to_string()),
            ]);
            insert_opt(&mut metadata, "exit_status", exit_status);
            insert_opt(&mut metadata, "runtime_s", runtime_s);
            projection(
                if outcome == "ready" { "info" } else { "warn" },
                EGRESS_TUNNEL_EVENT,
                metadata,
            )
        }
        BoxInfrastructureEvent::HostBootFetch {
            outcome,
            reason,
            duration_ms,
            swap_ms,
            from_version,
            to_version,
        } => {
            let level = if outcome == "restore_failed" {
                "error"
            } else if outcome == "fallback" {
                "warn"
            } else {
                "info"
            };
            let mut metadata = BTreeMap::from([
                ("outcome".into(), outcome.clone()),
                ("duration_ms".into(), duration_ms.to_string()),
            ]);
            insert_opt(&mut metadata, "reason", reason);
            insert_opt(&mut metadata, "swap_ms", swap_ms);
            insert_opt(&mut metadata, "from_version", from_version);
            insert_opt(&mut metadata, "to_version", to_version);
            projection(level, HOST_BOOT_FETCH_EVENT, metadata)
        }
        BoxInfrastructureEvent::ExecDaemonRestart {
            restart_attempt,
            runtime_s,
            cause,
            exit_status,
        } => projection(
            "error",
            EXEC_DAEMON_RESTART_EVENT,
            BTreeMap::from([
                ("restart_attempt".into(), restart_attempt.to_string()),
                ("runtime_s".into(), runtime_s.to_string()),
                ("cause".into(), cause.clone()),
                ("exit_status".into(), exit_status.to_string()),
            ]),
        ),
        BoxInfrastructureEvent::SupervisorRestart {
            restart_attempt,
            runtime_s,
            cause,
            exit_status,
        } => projection(
            "error",
            SUPERVISOR_RESTART_EVENT,
            BTreeMap::from([
                ("restart_attempt".into(), restart_attempt.to_string()),
                ("runtime_s".into(), runtime_s.to_string()),
                ("cause".into(), cause.clone()),
                ("exit_status".into(), exit_status.to_string()),
            ]),
        ),
        BoxInfrastructureEvent::CookiePersist {
            phase,
            outcome,
            seed_cookies,
            injected,
            missing_after,
            attempts,
        } => {
            let mut metadata = BTreeMap::from([
                ("phase".into(), phase.clone()),
                ("outcome".into(), outcome.clone()),
                ("seed_cookies".into(), seed_cookies.to_string()),
            ]);
            insert_opt(&mut metadata, "injected", injected);
            insert_opt(&mut metadata, "missing_after", missing_after);
            insert_opt(&mut metadata, "attempts", attempts);
            projection(
                if outcome == "failed" { "error" } else { "warn" },
                COOKIE_PERSIST_EVENT,
                metadata,
            )
        }
        BoxInfrastructureEvent::ProcessCrash {
            binary,
            signal,
            count,
        } => projection(
            "warn",
            BOX_PROCESS_CRASH_EVENT,
            BTreeMap::from([
                ("binary".into(), binary.clone()),
                ("signal".into(), signal.clone()),
                ("count".into(), count.to_string()),
            ]),
        ),
    }
}
