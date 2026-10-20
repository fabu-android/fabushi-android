use std::collections::BTreeMap;

use crate::host_invariant::SandInvariantReport;

use super::HostTelemetryProjection;

pub const HOST_INVARIANT_VIOLATION_EVENT: &str = "sand.host.invariant_violation";

pub fn invariant_violation_telemetry(report: &SandInvariantReport) -> HostTelemetryProjection {
    HostTelemetryProjection {
        level: Some("error"),
        event: Some(HOST_INVARIANT_VIOLATION_EVENT),
        metadata: BTreeMap::from([("name".into(), report.name.clone())]),
    }
}
