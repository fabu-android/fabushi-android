use std::collections::BTreeMap;

pub mod host_crash_marker;
pub mod webauthn_proxy_telemetry;
pub mod product_events;
pub mod recovery_events;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostTelemetryProjection {
    pub level: Option<&'static str>,
    pub event: Option<&'static str>,
    pub metadata: BTreeMap<String, String>,
}
pub mod agent_open_telemetry;
pub mod auto_review_approval_telemetry;
pub mod automation_fire_telemetry;
