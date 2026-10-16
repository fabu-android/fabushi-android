use std::collections::BTreeMap;

use super::HostTelemetryProjection;

#[derive(Debug, Clone, PartialEq)]
pub struct DiskPressureReport {
    pub level: String,
    pub volume: String,
    pub trigger: String,
    pub total_bytes: f64,
    pub available_bytes: f64,
    pub used_percent: f64,
}

pub fn telemetry_level(level: &str) -> &'static str {
    match level {
        "hard" => "error",
        "soft" => "warn",
        _ => "info",
    }
}

pub fn disk_pressure_telemetry(report: &DiskPressureReport) -> HostTelemetryProjection {
    HostTelemetryProjection {
        level: Some(telemetry_level(&report.level)),
        event: None,
        metadata: BTreeMap::from([
            ("volume".into(), report.volume.clone()),
            ("pressure_level".into(), report.level.clone()),
            ("trigger".into(), report.trigger.clone()),
            ("total_bytes".into(), report.total_bytes.to_string()),
            ("available_bytes".into(), report.available_bytes.to_string()),
            ("used_percent".into(), format!("{:.1}", report.used_percent)),
        ]),
    }
}
