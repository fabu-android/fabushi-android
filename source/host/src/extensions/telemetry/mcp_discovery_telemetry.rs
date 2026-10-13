use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const MCP_DISCOVERY_FAILED_EVENT: &str = "sand.mcp.discovery_failed";

#[derive(Debug, Clone, PartialEq)]
pub struct McpDiscoveryFailedReport {
    pub error_class: String,
    pub elapsed_ms: f64,
    pub served_stale: bool,
}

pub fn mcp_discovery_failed_telemetry(
    report: &McpDiscoveryFailedReport,
) -> HostTelemetryProjection {
    HostTelemetryProjection {
        level: Some("warn"),
        event: Some(MCP_DISCOVERY_FAILED_EVENT),
        metadata: BTreeMap::from([
            ("error_class".into(), report.error_class.clone()),
            ("elapsed_ms".into(), report.elapsed_ms.round().to_string()),
            ("served_stale".into(), report.served_stale.to_string()),
        ]),
    }
}
