use super::product_events::{StructuredTelemetryEvent, TelemetryLevel};
use std::collections::BTreeMap;

pub const REPLICA_RESYNC_WARN_DURATION_MS: f64 = 60_000.0;
pub const RECOVERY_FUTILITY_WARN_AT: u64 = 3;

fn finite_nonnegative(value: f64, label: &str) -> Result<f64, String> {
    if !value.is_finite() || value < 0.0 {
        return Err(format!("{label} must be finite and nonnegative"));
    }
    Ok(value)
}

fn rounded(value: f64) -> String {
    value.round().to_string()
}

pub fn rebuild_escalation_event(
    rebuild_kind: &str,
    elapsed_ms: f64,
    action: &str,
    escalation: &str,
) -> Result<StructuredTelemetryEvent, String> {
    if !matches!(rebuild_kind, "update" | "reset" | "recover" | "reconnecting") {
        return Err("rebuild kind is invalid".into());
    }
    finite_nonnegative(elapsed_ms, "elapsedMs")?;
    if !matches!(action, "shown" | "keep_waiting" | "retry") {
        return Err("rebuild escalation action is invalid".into());
    }
    if !matches!(escalation, "taking-longer" | "unreachable") {
        return Err("rebuild escalation is invalid".into());
    }
    Ok(StructuredTelemetryEvent {
        level: if action == "shown" { TelemetryLevel::Warn } else { TelemetryLevel::Info },
        metadata: BTreeMap::from([
            ("event".into(), "escalation".into()),
            ("action".into(), action.into()),
            ("escalation".into(), escalation.into()),
            ("kind".into(), rebuild_kind.into()),
            ("elapsed_ms".into(), rounded(elapsed_ms)),
        ]),
    })
}

pub fn rebuild_pending_stall_event(
    rebuild_kind: &str,
    elapsed_ms: f64,
    has_ack: bool,
    stream_attached: bool,
) -> Result<StructuredTelemetryEvent, String> {
    if !matches!(rebuild_kind, "update" | "reset" | "recover" | "reconnecting") {
        return Err("rebuild kind is invalid".into());
    }
    finite_nonnegative(elapsed_ms, "elapsedMs")?;
    Ok(StructuredTelemetryEvent {
        level: TelemetryLevel::Warn,
        metadata: BTreeMap::from([
            ("event".into(), "pending_stall".into()),
            ("kind".into(), rebuild_kind.into()),
            ("has_ack".into(), has_ack.to_string()),
            ("stream_attached".into(), stream_attached.to_string()),
            ("elapsed_ms".into(), rounded(elapsed_ms)),
        ]),
    })
}

pub fn replica_resync_event(
    replica: &str,
    outcome: &str,
    attempts: f64,
    duration_ms: f64,
    buffer_overflowed: bool,
    epoch_changed: bool,
) -> Result<StructuredTelemetryEvent, String> {
    if !matches!(replica, "roster" | "transcript") {
        return Err("replica resync replica is invalid".into());
    }
    if !matches!(outcome, "covered" | "superseded" | "abandoned") {
        return Err("replica resync outcome is invalid".into());
    }
    finite_nonnegative(attempts, "attempts")?;
    finite_nonnegative(duration_ms, "durationMs")?;
    Ok(StructuredTelemetryEvent {
        level: if outcome == "abandoned" || duration_ms > REPLICA_RESYNC_WARN_DURATION_MS {
            TelemetryLevel::Warn
        } else {
            TelemetryLevel::Info
        },
        metadata: BTreeMap::from([
            ("event".into(), "replica_resync".into()),
            ("replica".into(), replica.into()),
            ("outcome".into(), outcome.into()),
            ("attempts".into(), rounded(attempts)),
            ("duration_ms".into(), rounded(duration_ms)),
            ("buffer_overflowed".into(), buffer_overflowed.to_string()),
            ("epoch_changed".into(), epoch_changed.to_string()),
        ]),
    })
}

pub fn send_journal_restore_event(
    outcome: &str,
    phase: &str,
    age_ms: f64,
    record_count: f64,
) -> Result<StructuredTelemetryEvent, String> {
    if !matches!(outcome, "dispatched" | "retired" | "parked" | "failed") {
        return Err("send journal restore outcome is invalid".into());
    }
    if !matches!(phase, "queued" | "prepared" | "dispatching" | "accepted-awaiting-echo") {
        return Err("send journal restore phase is invalid".into());
    }
    finite_nonnegative(age_ms, "ageMs")?;
    finite_nonnegative(record_count, "recordCount")?;
    Ok(StructuredTelemetryEvent {
        level: if matches!(outcome, "parked" | "failed") {
            TelemetryLevel::Warn
        } else {
            TelemetryLevel::Info
        },
        metadata: BTreeMap::from([
            ("event".into(), "send_journal_restore".into()),
            ("outcome".into(), outcome.into()),
            ("phase".into(), phase.into()),
            ("age_ms".into(), rounded(age_ms)),
            ("record_count".into(), rounded(record_count)),
        ]),
    })
}

pub fn resync_completed_event(
    failed_steps: &[String],
    duration_ms: f64,
) -> Result<StructuredTelemetryEvent, String> {
    finite_nonnegative(duration_ms, "durationMs")?;
    if failed_steps.iter().any(|step| step.is_empty() || step.len() > 128 || step.chars().any(char::is_control)) {
        return Err("resync completed step name is invalid".into());
    }
    let mut metadata = BTreeMap::from([
        ("failed_steps".into(), failed_steps.len().to_string()),
        ("duration_ms".into(), rounded(duration_ms)),
    ]);
    if !failed_steps.is_empty() {
        metadata.insert("failed_step_names".into(), failed_steps.join(","));
    }
    Ok(StructuredTelemetryEvent {
        level: if failed_steps.is_empty() { TelemetryLevel::Info } else { TelemetryLevel::Warn },
        metadata,
    })
}

pub fn recovery_action_event(
    action: &str,
    failure_class: &str,
    consecutive_identical: u64,
) -> Result<StructuredTelemetryEvent, String> {
    if !matches!(action, "retry" | "recover" | "reset" | "relaunch") {
        return Err("recovery action is invalid".into());
    }
    if failure_class.is_empty()
        || failure_class.len() > 64
        || !failure_class.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err("recovery failure class is invalid".into());
    }
    if consecutive_identical == 0 {
        return Err("consecutiveIdentical must be positive".into());
    }
    Ok(StructuredTelemetryEvent {
        level: if consecutive_identical >= RECOVERY_FUTILITY_WARN_AT {
            TelemetryLevel::Warn
        } else {
            TelemetryLevel::Info
        },
        metadata: BTreeMap::from([
            ("action".into(), action.into()),
            ("failure_class".into(), failure_class.to_ascii_lowercase()),
            ("consecutive_identical".into(), consecutive_identical.to_string()),
        ]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_contract_preserves_escalation_and_stall_severity() {
        let shown = rebuild_escalation_event("recover", 12.4, "shown", "taking-longer").unwrap();
        assert_eq!(shown.level, TelemetryLevel::Warn);
        assert_eq!(shown.metadata.get("elapsed_ms").map(String::as_str), Some("12"));
        let retry = rebuild_escalation_event("reset", 20.0, "retry", "unreachable").unwrap();
        assert_eq!(retry.level, TelemetryLevel::Info);
        assert_eq!(
            rebuild_pending_stall_event("reconnecting", 30.0, true, false).unwrap().level,
            TelemetryLevel::Warn,
        );
        assert!(rebuild_escalation_event("other", 1.0, "shown", "taking-longer").is_err());
        assert!(rebuild_pending_stall_event("recover", -1.0, false, false).is_err());
    }

    #[test]
    fn reconciliation_contract_preserves_warning_thresholds() {
        assert_eq!(
            replica_resync_event("roster", "covered", 1.0, 60_000.0, false, false)
                .unwrap()
                .level,
            TelemetryLevel::Info,
        );
        assert_eq!(
            replica_resync_event("transcript", "covered", 2.0, 60_001.0, false, true)
                .unwrap()
                .level,
            TelemetryLevel::Warn,
        );
        assert_eq!(
            replica_resync_event("roster", "abandoned", 3.0, 1.0, true, false)
                .unwrap()
                .level,
            TelemetryLevel::Warn,
        );
        assert_eq!(
            send_journal_restore_event("parked", "queued", 5.0, 2.0).unwrap().level,
            TelemetryLevel::Warn,
        );
        assert!(send_journal_restore_event("dispatched", "bad", 1.0, 1.0).is_err());
    }

    #[test]
    fn resync_completion_and_recovery_futility_are_bounded() {
        let clean = resync_completed_event(&[], 10.2).unwrap();
        assert_eq!(clean.level, TelemetryLevel::Info);
        let failed = resync_completed_event(&["roster".into(), "transcript".into()], 20.0).unwrap();
        assert_eq!(failed.level, TelemetryLevel::Warn);
        assert_eq!(
            failed.metadata.get("failed_step_names").map(String::as_str),
            Some("roster,transcript"),
        );

        assert_eq!(recovery_action_event("retry", "network_error", 2).unwrap().level, TelemetryLevel::Info);
        let futile = recovery_action_event("retry", "NETWORK_ERROR", 3).unwrap();
        assert_eq!(futile.level, TelemetryLevel::Warn);
        assert_eq!(futile.metadata.get("failure_class").map(String::as_str), Some("network_error"));
        assert!(recovery_action_event("retry", "bad value", 1).is_err());
        assert!(recovery_action_event("retry", "network_error", 0).is_err());
    }
}
