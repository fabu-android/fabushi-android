use std::collections::BTreeMap;

use super::HostTelemetryProjection;
use super::sand_error_tags::{SandErrorValue, sand_error_tags};

pub const MEMORY_SYNTHESIS_EVENT: &str = "sand.memory.synthesis";

#[derive(Debug, Clone, PartialEq)]
pub enum MemorySynthesisReport {
    SkippedGate,
    Shed {
        cause: SandErrorValue,
        item_count: i64,
    },
    Failed {
        cause: SandErrorValue,
        duration_ms: f64,
        item_count: i64,
    },
    Ok {
        duration_ms: f64,
        item_count: i64,
    },
}

pub fn memory_synthesis_telemetry(report: &MemorySynthesisReport) -> HostTelemetryProjection {
    match report {
        MemorySynthesisReport::SkippedGate => HostTelemetryProjection {
            level: Some("info"),
            event: Some(MEMORY_SYNTHESIS_EVENT),
            metadata: BTreeMap::from([("outcome".into(), "skipped_gate".into())]),
        },
        MemorySynthesisReport::Shed { cause, item_count } => {
            let mut metadata = BTreeMap::from([
                ("outcome".into(), "shed".into()),
                ("item_count".into(), item_count.to_string()),
            ]);
            metadata.extend(sand_error_tags(cause));
            HostTelemetryProjection {
                level: Some("warn"),
                event: Some(MEMORY_SYNTHESIS_EVENT),
                metadata,
            }
        }
        MemorySynthesisReport::Failed {
            cause,
            duration_ms,
            item_count,
        } => {
            let mut metadata = BTreeMap::from([
                ("outcome".into(), "failed".into()),
                ("duration_ms".into(), duration_ms.round().to_string()),
                ("item_count".into(), item_count.to_string()),
            ]);
            metadata.extend(sand_error_tags(cause));
            HostTelemetryProjection {
                level: Some("warn"),
                event: Some(MEMORY_SYNTHESIS_EVENT),
                metadata,
            }
        }
        MemorySynthesisReport::Ok {
            duration_ms,
            item_count,
        } => HostTelemetryProjection {
            level: Some("info"),
            event: Some(MEMORY_SYNTHESIS_EVENT),
            metadata: BTreeMap::from([
                ("outcome".into(), "ok".into()),
                ("duration_ms".into(), duration_ms.round().to_string()),
                ("item_count".into(), item_count.to_string()),
            ]),
        },
    }
}
