//! Telegram Desktop 1:1 Parity Submodule: `ApiReadMetrics`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_read_metrics.cpp, Telegram/SourceFiles/api/api_read_metrics.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FinalizedReadMetric {
    pub msg_id: i64,
    pub view_id: u64,
    pub time_in_view_ms: i32,
    pub active_time_in_view_ms: i32,
    pub height_to_viewport_ratio_permille: i32,
    pub seen_range_ratio_permille: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiReadMetrics {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub pending: HashMap<u64, Vec<FinalizedReadMetric>>,
    pub total_recorded: u64,
    pub updated_at: i64,
}

impl Default for ApiReadMetrics {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiReadMetrics".to_string(),
            pending: HashMap::new(),
            total_recorded: 0,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiReadMetrics {
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

    pub fn add(&mut self, peer_id: u64, metric: FinalizedReadMetric) {
        self.pending.entry(peer_id).or_default().push(metric);
        self.total_recorded += 1;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn take_pending(&mut self, peer_id: u64) -> Vec<FinalizedReadMetric> {
        self.pending.remove(&peer_id).unwrap_or_default()
    }

    pub fn pending_count(&self, peer_id: u64) -> usize {
        self.pending.get(&peer_id).map(|v| v.len()).unwrap_or(0)
    }

    pub fn clear(&mut self) {
        self.pending.clear();
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
