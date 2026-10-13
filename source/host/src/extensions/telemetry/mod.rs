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
pub mod agent_error_telemetry;
pub mod auto_review_approval_telemetry;
pub mod automation_fire_telemetry;
pub mod automation_lifecycle_telemetry;
#[cfg(test)]
mod android_projection_tests;
pub mod disk_pressure_telemetry;
pub mod host_diagnostic_telemetry;
pub mod search_index_health_telemetry;
pub mod automation_shadow_prune_telemetry;
pub mod bot_block_telemetry;
pub mod conversation_gc_telemetry;
pub mod mcp_discovery_telemetry;
pub mod box_copy_in_telemetry;
pub mod box_recreate_decided_telemetry;
pub mod experiments_diagnostic_telemetry;
pub mod host_extension_diagnostic_telemetry;
pub mod turn_empty_delivery_telemetry;
pub mod sand_error_tags;
pub mod journal_outcome_telemetry;
pub mod connector_auth_telemetry;
