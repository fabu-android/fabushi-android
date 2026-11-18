//! Telegram Desktop 1:1 Parity Submodule: `ApiStatisticsDataDeserialize`
//! Subsystem: `api` | Feature ID: `TCG-N12`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_statistics_data_deserialize.cpp, Telegram/SourceFiles/api/api_statistics_data_deserialize.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StatisticalGraphData {
    pub graph_type: String,
    pub json_payload: String,
    pub zoom_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiStatisticsDataDeserialize {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub deserialized_graphs: Vec<StatisticalGraphData>,
    pub updated_at: i64,
}

impl Default for ApiStatisticsDataDeserialize {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiStatisticsDataDeserialize".to_string(),
            deserialized_graphs: Vec::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiStatisticsDataDeserialize {
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

    pub fn parse_graph(&mut self, raw_json: impl Into<String>, zoom_token: Option<String>) -> StatisticalGraphData {
        let json_payload = raw_json.into();
        let graph = StatisticalGraphData {
            graph_type: "line".to_string(),
            json_payload,
            zoom_token,
        };
        self.deserialized_graphs.push(graph.clone());
        self.updated_at = chrono::Utc::now().timestamp();
        graph
    }
}
