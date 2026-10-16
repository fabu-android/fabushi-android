use std::collections::BTreeMap;

use crate::extensions::memory::memory_synthesis_service::{SynthesisReport, SynthesisReportOutcome};

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


pub fn memory_synthesis_service_telemetry(report: &SynthesisReport) -> HostTelemetryProjection {
    let report = match report.outcome {
        SynthesisReportOutcome::Committed | SynthesisReportOutcome::NoWork => MemorySynthesisReport::Ok {
            duration_ms: report.duration_ms as f64,
            item_count: report.change_count as i64,
        },
        SynthesisReportOutcome::Dropped => MemorySynthesisReport::Shed {
            cause: SandErrorValue::new("SAND-E0412"),
            item_count: report.evidence_count as i64,
        },
        SynthesisReportOutcome::InvalidOutput => MemorySynthesisReport::Failed {
            cause: SandErrorValue::new("SAND-E0409"), duration_ms: report.duration_ms as f64,
            item_count: report.change_count as i64,
        },
        SynthesisReportOutcome::Rejected => MemorySynthesisReport::Failed {
            cause: SandErrorValue::new("SAND-E0410"), duration_ms: report.duration_ms as f64,
            item_count: report.change_count as i64,
        },
        SynthesisReportOutcome::Stale => MemorySynthesisReport::Failed {
            cause: SandErrorValue::new("SAND-E0411"), duration_ms: report.duration_ms as f64,
            item_count: report.change_count as i64,
        },
        SynthesisReportOutcome::Failed => MemorySynthesisReport::Failed {
            cause: SandErrorValue::new("SAND-E0413"), duration_ms: report.duration_ms as f64,
            item_count: report.change_count as i64,
        },
    };
    memory_synthesis_telemetry(&report)
}

#[cfg(test)]
mod service_tests {
    use super::*;
    fn report(outcome: SynthesisReportOutcome) -> SynthesisReport {
        SynthesisReport { outcome, agent_id: Some("agent-a".into()), evidence_count: 3, input_memory_count: 2, change_count: 1, duration_ms: 12 }
    }
    #[test]
    fn service_failures_preserve_desktop_error_registry_codes() {
        for (outcome, code) in [
            (SynthesisReportOutcome::InvalidOutput,"SAND-E0409"),
            (SynthesisReportOutcome::Rejected,"SAND-E0410"),
            (SynthesisReportOutcome::Stale,"SAND-E0411"),
            (SynthesisReportOutcome::Dropped,"SAND-E0412"),
            (SynthesisReportOutcome::Failed,"SAND-E0413"),
        ] {
            let p=memory_synthesis_service_telemetry(&report(outcome));
            assert_eq!(p.event,Some(MEMORY_SYNTHESIS_EVENT));
            assert_eq!(p.metadata.get("error_code").map(String::as_str),Some(code));
        }
    }
    #[test]
    fn committed_and_no_work_project_ok() {
        for outcome in [SynthesisReportOutcome::Committed,SynthesisReportOutcome::NoWork] {
            let p=memory_synthesis_service_telemetry(&report(outcome));
            assert_eq!(p.level,Some("info"));
            assert_eq!(p.metadata.get("outcome").map(String::as_str),Some("ok"));
        }
    }
}
