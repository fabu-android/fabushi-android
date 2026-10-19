use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const EXPERIMENTS_DIAGNOSTIC_EVENT: &str = "sand.experiments.diagnostic";
pub const INFO_KINDS: &[&str] = &[
    "bootstrap_resolved",
    "bootstrap_anonymous",
    "bootstrap_discarded_auth_changed",
    "exposure_flush_failed",
    "shutdown_failed",
];
pub const WARN_KINDS: &[&str] =
    &["bootstrap_config_unparseable", "bootstrap_cache_read_failed"];

pub use crate::extensions::experiments::ExperimentDiagnostic as ExperimentsDiagnostic;
pub fn level_for(diagnostic: &ExperimentsDiagnostic) -> &'static str {
    if diagnostic.kind == "config_not_applied" {
        return if diagnostic.reason.as_deref() == Some("identity_unhydrated") {
            "info"
        } else {
            "warn"
        };
    }
    if INFO_KINDS.contains(&diagnostic.kind.as_str()) {
        return "info";
    }
    if WARN_KINDS.contains(&diagnostic.kind.as_str()) {
        "warn"
    } else {
        "error"
    }
}

pub fn experiments_diagnostic_telemetry(
    diagnostic: &ExperimentsDiagnostic,
) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([("kind".into(), diagnostic.kind.clone())]);
    for (key, value) in [
        ("stage", diagnostic.stage.as_ref()),
        ("reason", diagnostic.reason.as_ref()),
        ("error_class", diagnostic.error_class.as_ref()),
    ] {
        if let Some(value) = value {
            metadata.insert(key.into(), value.clone());
        }
    }
    if let Some(value) = diagnostic.gates_on_count {
        metadata.insert("gates_on_count".into(), value.to_string());
    }
    if let Some(value) = diagnostic.authenticated {
        metadata.insert("authenticated".into(), value.to_string());
    }
    HostTelemetryProjection {
        level: Some(level_for(diagnostic)),
        event: Some(EXPERIMENTS_DIAGNOSTIC_EVENT),
        metadata,
    }
}
