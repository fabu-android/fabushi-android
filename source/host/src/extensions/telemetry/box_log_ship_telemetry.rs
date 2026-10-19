use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const BOX_LOG_SHIP_EVENT: &str = "sand.box.log_ship";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoxLogShipReport {
    Progress {
        bytes_written: i64,
        bytes_delivered: i64,
        pending_window_count: i64,
        oldest_pending_window_age_ms: i64,
    },
    SaveFailed {
        error_class: String,
        failure_count: i64,
    },
    SaveRecovered {
        error_class: String,
        failure_count: i64,
    },
}

pub fn box_log_ship_telemetry(report: &BoxLogShipReport) -> HostTelemetryProjection {
    match report {
        BoxLogShipReport::Progress {
            bytes_written,
            bytes_delivered,
            pending_window_count,
            oldest_pending_window_age_ms,
        } => HostTelemetryProjection {
            level: Some("info"),
            event: Some(BOX_LOG_SHIP_EVENT),
            metadata: BTreeMap::from([
                ("kind".into(), "progress".into()),
                ("bytes_written".into(), bytes_written.to_string()),
                ("bytes_delivered".into(), bytes_delivered.to_string()),
                (
                    "pending_window_count".into(),
                    pending_window_count.to_string(),
                ),
                (
                    "oldest_pending_window_age_ms".into(),
                    oldest_pending_window_age_ms.to_string(),
                ),
            ]),
        },
        BoxLogShipReport::SaveFailed {
            error_class,
            failure_count,
        }
        | BoxLogShipReport::SaveRecovered {
            error_class,
            failure_count,
        } => {
            let failed = matches!(report, BoxLogShipReport::SaveFailed { .. });
            HostTelemetryProjection {
                level: Some(if failed { "warn" } else { "info" }),
                event: Some(BOX_LOG_SHIP_EVENT),
                metadata: BTreeMap::from([
                    (
                        "kind".into(),
                        if failed { "save_failed" } else { "save_recovered" }.into(),
                    ),
                    ("error_class".into(), error_class.clone()),
                    ("failure_count".into(), failure_count.to_string()),
                ]),
            }
        }
    }
}
