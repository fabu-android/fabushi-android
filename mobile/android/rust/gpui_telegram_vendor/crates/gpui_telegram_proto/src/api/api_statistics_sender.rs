//! Telegram Desktop 1:1 Parity Submodule: `ApiStatisticsSender`
//! Subsystem: `api` | Feature ID: `TCG-N12`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_statistics_sender.cpp, Telegram/SourceFiles/api/api_statistics_sender.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiStatisticsSender {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub peer_id: u64,
    pub dc_id: i32,
    pub in_flight_requests: Vec<u64>,
    pub updated_at: i64,
}

impl Default for ApiStatisticsSender {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiStatisticsSender".to_string(),
            peer_id: 0,
            dc_id: 0,
            in_flight_requests: Vec::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiStatisticsSender {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_peer(peer_id: u64, dc_id: i32) -> Self {
        Self {
            peer_id,
            dc_id,
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

    pub fn register_request(&mut self, request_id: u64) {
        self.in_flight_requests.push(request_id);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn finish_request(&mut self, request_id: u64) {
        self.in_flight_requests.retain(|id| *id != request_id);
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
