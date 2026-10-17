use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const HOST_DIAGNOSTIC_EVENT: &str = "sand.host.diagnostic";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostDiagnostic {
    pub kind: String,
    pub stage: Option<String>,
    pub agent_id: Option<String>,
    pub reason: Option<String>,
    pub error_class: Option<String>,
}

pub fn host_diagnostic_telemetry(diagnostic: &HostDiagnostic) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([("kind".into(), diagnostic.kind.clone())]);
    if let Some(value) = &diagnostic.stage {
        metadata.insert("stage".into(), value.clone());
    }
    if let Some(value) = &diagnostic.agent_id {
        metadata.insert("agent_id".into(), value.clone());
    }
    if let Some(value) = &diagnostic.reason {
        metadata.insert("reason".into(), value.clone());
    }
    if let Some(value) = &diagnostic.error_class {
        metadata.insert("error_class".into(), value.clone());
    }
    HostTelemetryProjection {
        level: Some(if diagnostic.kind == "send_ledger_degraded" {
            "error"
        } else {
            "warn"
        }),
        event: Some(HOST_DIAGNOSTIC_EVENT),
        metadata,
    }
}
