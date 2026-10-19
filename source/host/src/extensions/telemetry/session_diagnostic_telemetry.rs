use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const SESSION_WARN_KINDS: &[&str] = &[
    "open_io_retry",
    "wal_unavailable",
    "quarantine_copied",
    "stat_failed",
    "path_stat_failed",
    "connector_secrets_unreadable",
    "fallback_adopt_failed",
    "placeholder_check_failed",
    "degraded",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionDiagnosticFamily {
    StoreDb,
    Maintenance,
    Materialize,
    SummaryBuild,
}

impl SessionDiagnosticFamily {
    pub fn event(self) -> &'static str {
        match self {
            Self::StoreDb => "sand.session.store_db",
            Self::Maintenance => "sand.session.maintenance",
            Self::Materialize => "sand.session.materialize",
            Self::SummaryBuild => "sand.session.summary_build",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTelemetryDiagnostic {
    pub family: SessionDiagnosticFamily,
    pub kind: String,
    pub agent_id: Option<String>,
    pub error_class: Option<String>,
    pub outcome: Option<String>,
    pub quarantine: Option<String>,
    pub salvaged_kv: Option<i64>,
    pub salvaged_blobs: Option<i64>,
    pub salvaged_transcript: Option<i64>,
}

pub fn session_diagnostic_telemetry(
    report: &SessionTelemetryDiagnostic,
) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([("kind".into(), report.kind.clone())]);
    if let Some(value) = &report.agent_id {
        metadata.insert("agent_id".into(), value.clone());
    }
    if let Some(value) = &report.error_class {
        metadata.insert("error_class".into(), value.clone());
    }
    if report.family == SessionDiagnosticFamily::StoreDb {
        if let Some(value) = &report.outcome {
            metadata.insert("outcome".into(), value.clone());
        }
        if let Some(value) = &report.quarantine {
            metadata.insert("quarantine".into(), value.clone());
        }
        for (key, value) in [
            ("salvaged_kv", report.salvaged_kv),
            ("salvaged_blobs", report.salvaged_blobs),
            ("salvaged_transcript", report.salvaged_transcript),
        ] {
            if let Some(value) = value {
                metadata.insert(key.into(), value.to_string());
            }
        }
    }
    HostTelemetryProjection {
        level: Some(if SESSION_WARN_KINDS.contains(&report.kind.as_str()) {
            "warn"
        } else {
            "error"
        }),
        event: Some(report.family.event()),
        metadata,
    }
}
