//! Telegram Desktop 1:1 Parity Submodule: `ApiStatistics`
//! Subsystem: `api` | Feature ID: `TCG-N12`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_statistics.cpp, Telegram/SourceFiles/api/api_statistics.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::api::api_statistics_data_deserialize::StatisticalGraphData;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChannelStatisticsData {
    pub period_start_date: i64,
    pub period_end_date: i64,
    pub members_count: i32,
    pub messages_count: i32,
    pub views_per_post: f64,
    pub shares_per_post: f64,
    pub reactions_per_post: f64,
    pub graphs: Vec<StatisticalGraphData>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiStatistics {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub channel_stats: HashMap<u64, ChannelStatisticsData>,
    pub updated_at: i64,
}

impl Default for ApiStatistics {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiStatistics".to_string(),
            channel_stats: HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiStatistics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_label(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.is_enabled = enabled;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn set_channel_stats(&mut self, channel_id: u64, stats: ChannelStatisticsData) {
        self.channel_stats.insert(channel_id, stats);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn get_channel_stats(&self, channel_id: u64) -> Option<&ChannelStatisticsData> {
        self.channel_stats.get(&channel_id)
    }
}
