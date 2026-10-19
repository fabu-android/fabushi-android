use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const BOT_BLOCK_EVENT: &str = "sand.bot_block";
pub const BOT_BLOCK_DETAIL_EVENT: &str = "sand.bot_block_detail";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotBlockReport {
    pub conversation_id: String,
    pub family: String,
    pub confidence: String,
    pub blocked_host: String,
    pub blocked_url: String,
}

pub fn bot_block_telemetry(report: &BotBlockReport) -> HostTelemetryProjection {
    HostTelemetryProjection {
        level: Some("warn"),
        event: Some(BOT_BLOCK_EVENT),
        metadata: BTreeMap::from([
            ("conversation_id".into(), report.conversation_id.clone()),
            ("family".into(), report.family.clone()),
            ("confidence".into(), report.confidence.clone()),
        ]),
    }
}

pub fn bot_block_detail_telemetry(report: &BotBlockReport) -> HostTelemetryProjection {
    HostTelemetryProjection {
        level: Some("warn"),
        event: Some(BOT_BLOCK_DETAIL_EVENT),
        metadata: BTreeMap::from([
            ("conversation_id".into(), report.conversation_id.clone()),
            ("family".into(), report.family.clone()),
            ("blocked_host".into(), report.blocked_host.clone()),
            ("blocked_url".into(), report.blocked_url.clone()),
        ]),
    }
}
