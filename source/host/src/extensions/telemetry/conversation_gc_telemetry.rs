use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const CONVERSATION_GC_EVENT: &str = "sand.conversation.gc";
pub const NUMBER_MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

#[derive(Debug, Clone, PartialEq)]
pub enum ConversationGcReport {
    Failed {
        trigger: String,
        agent_id: String,
    },
    Skipped {
        trigger: String,
        agent_id: String,
        skip_reason: String,
        unresolved_proto_refs: Option<f64>,
    },
    Collected {
        trigger: String,
        agent_id: String,
        still_over_cap: bool,
        deleted_rows: f64,
        deleted_bytes: f64,
        live_rows: f64,
        live_bytes: f64,
        vacuumed: bool,
    },
}

pub fn capped_count(value: f64) -> String {
    value
        .round()
        .max(0.0)
        .min(NUMBER_MAX_SAFE_INTEGER)
        .to_string()
}

pub fn conversation_gc_level(report: &ConversationGcReport) -> &'static str {
    match report {
        ConversationGcReport::Failed { .. } => "warn",
        ConversationGcReport::Skipped { skip_reason, .. } => {
            if skip_reason == "unresolved-refs" { "warn" } else { "info" }
        }
        ConversationGcReport::Collected { still_over_cap, .. } => {
            if *still_over_cap { "warn" } else { "info" }
        }
    }
}

pub fn conversation_gc_telemetry(report: &ConversationGcReport) -> HostTelemetryProjection {
    let (trigger, agent_id, outcome) = match report {
        ConversationGcReport::Failed { trigger, agent_id } => (trigger, agent_id, "failed"),
        ConversationGcReport::Skipped { trigger, agent_id, .. } => (trigger, agent_id, "skipped"),
        ConversationGcReport::Collected { trigger, agent_id, .. } => (trigger, agent_id, "collected"),
    };
    let mut metadata = BTreeMap::from([
        ("trigger".into(), trigger.clone()),
        ("outcome".into(), outcome.into()),
        ("agent_id".into(), agent_id.clone()),
    ]);
    match report {
        ConversationGcReport::Skipped {
            skip_reason,
            unresolved_proto_refs,
            ..
        } => {
            metadata.insert("skip_reason".into(), skip_reason.clone());
            if let Some(value) = unresolved_proto_refs {
                metadata.insert("unresolved_proto_refs".into(), capped_count(*value));
            }
        }
        ConversationGcReport::Collected {
            deleted_rows,
            deleted_bytes,
            live_rows,
            live_bytes,
            vacuumed,
            ..
        } => {
            metadata.insert("deleted_rows".into(), capped_count(*deleted_rows));
            metadata.insert("deleted_bytes".into(), capped_count(*deleted_bytes));
            metadata.insert("live_rows".into(), capped_count(*live_rows));
            metadata.insert("live_bytes".into(), capped_count(*live_bytes));
            metadata.insert("vacuumed".into(), vacuumed.to_string());
        }
        ConversationGcReport::Failed { .. } => {}
    }
    HostTelemetryProjection {
        level: Some(conversation_gc_level(report)),
        event: Some(CONVERSATION_GC_EVENT),
        metadata,
    }
}
