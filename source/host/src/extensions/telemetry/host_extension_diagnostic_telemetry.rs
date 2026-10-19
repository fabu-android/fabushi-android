use std::collections::BTreeMap;

use super::HostTelemetryProjection;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostExtensionDiagnostic {
    pub extension: String,
    pub kind: Option<String>,
    pub error_class: Option<String>,
    pub operation: Option<String>,
    pub agent_id: Option<String>,
    pub error_type: Option<String>,
    pub error_code: Option<String>,
    pub has_active: Option<bool>,
    pub leg: Option<String>,
}

fn metadata(pairs: &[(&str, Option<String>)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .filter_map(|(key, value)| value.clone().map(|value| ((*key).into(), value)))
        .collect()
}

pub fn host_extension_diagnostic_telemetry(
    diagnostic: &HostExtensionDiagnostic,
) -> Option<HostTelemetryProjection> {
    let projection = match diagnostic.extension.as_str() {
        "box_store" => HostTelemetryProjection {
            level: Some("warn"),
            event: Some("sand.box_store.diagnostic"),
            metadata: metadata(&[
                ("kind", diagnostic.kind.clone()),
                ("error_class", diagnostic.error_class.clone()),
            ]),
        },
        "automation_cloud_sync" => HostTelemetryProjection {
            level: Some("error"),
            event: Some("sand.automation.cloud_sync"),
            metadata: metadata(&[
                ("operation", diagnostic.operation.clone()),
                ("agent_id", diagnostic.agent_id.clone()),
                ("error_type", diagnostic.error_type.clone()),
                ("error_code", diagnostic.error_code.clone()),
            ]),
        },
        "managed_setup" => HostTelemetryProjection {
            level: Some(
                if diagnostic.kind.as_deref() == Some("managed_skills") {
                    "error"
                } else {
                    "warn"
                },
            ),
            event: Some("sand.managed_setup.load_failed"),
            metadata: metadata(&[
                ("kind", diagnostic.kind.clone()),
                ("error_class", diagnostic.error_class.clone()),
            ]),
        },
        "attachments" => HostTelemetryProjection {
            level: Some("warn"),
            event: Some("sand.attachment.read_miss"),
            metadata: metadata(&[
                ("kind", diagnostic.kind.clone()),
                (
                    "has_active",
                    Some(
                        diagnostic
                            .has_active
                            .map_or_else(|| "undefined".into(), |value| value.to_string()),
                    ),
                ),
            ]),
        },
        "action_audit" => HostTelemetryProjection {
            level: Some("error"),
            event: Some("sand.action_audit.drop"),
            metadata: metadata(&[("error_class", diagnostic.error_class.clone())]),
        },
        "mcp" => HostTelemetryProjection {
            level: Some("info"),
            event: Some("sand.mcp.host_edge_failed"),
            metadata: metadata(&[
                ("leg", diagnostic.leg.clone()),
                ("error_class", diagnostic.error_class.clone()),
            ]),
        },
        _ => return None,
    };
    Some(projection)
}
