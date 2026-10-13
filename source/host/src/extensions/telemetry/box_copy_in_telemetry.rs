use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const BOX_COPY_IN_EVENT: &str = "sand.box_copy_in";

pub fn box_copy_in_telemetry(
    level: &'static str,
    metadata: BTreeMap<String, String>,
) -> HostTelemetryProjection {
    HostTelemetryProjection {
        level: Some(level),
        event: Some(BOX_COPY_IN_EVENT),
        metadata,
    }
}
