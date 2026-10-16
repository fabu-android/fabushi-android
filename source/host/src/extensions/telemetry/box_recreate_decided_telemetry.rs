use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const BOX_RECREATE_DECIDED_EVENT: &str = "sand.box.recreate_decided";

pub fn box_recreate_decided_telemetry(
    metadata: BTreeMap<String, String>,
) -> HostTelemetryProjection {
    HostTelemetryProjection {
        level: Some("info"),
        event: Some(BOX_RECREATE_DECIDED_EVENT),
        metadata,
    }
}
