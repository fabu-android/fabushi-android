//! Telegram Desktop 1:1 Parity Submodule: `ApiSending`
//! Subsystem: `api` | Feature ID: `TCG-N19`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_sending.cpp, Telegram/SourceFiles/api/api_sending.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use crate::api::api_common::{MessageToSend, SendAction, SendOptions};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MusicSelectionItem {
    pub document_id: u64,
    pub title: String,
    pub performer: String,
    pub duration_sec: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputVenue {
    pub lat: f64,
    pub lon: f64,
    pub title: String,
    pub address: String,
    pub provider: String,
    pub venue_id: String,
    pub venue_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundMessagePayload {
    pub peer_id: u64,
    pub reply_to_msg_id: Option<i64>,
    pub text: String,
    pub options: SendOptions,
    pub local_id: i64,
    pub random_id: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiSending {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub in_flight: Vec<OutboundMessagePayload>,
    pub total_sent: u64,
    pub updated_at: i64,
}

impl Default for ApiSending {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiSending".to_string(),
            in_flight: Vec::new(),
            total_sent: 0,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiSending {
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

    pub fn send_text(&mut self, peer_id: u64, text: impl Into<String>, reply_to: Option<i64>, options: SendOptions) -> u64 {
        let random_id = uuid::Uuid::new_v4().as_u128() as u64;
        let local_id = -((self.total_sent as i64) + 1);
        self.in_flight.push(OutboundMessagePayload {
            peer_id,
            reply_to_msg_id: reply_to,
            text: text.into(),
            options,
            local_id,
            random_id,
        });
        self.total_sent += 1;
        self.updated_at = chrono::Utc::now().timestamp();
        random_id
    }

    pub fn send_location(&mut self, action: SendAction, lat: f64, lon: f64) -> u64 {
        let random_id = uuid::Uuid::new_v4().as_u128() as u64;
        let local_id = -((self.total_sent as i64) + 1);
        self.in_flight.push(OutboundMessagePayload {
            peer_id: action.peer_id,
            reply_to_msg_id: action.reply_to_msg_id,
            text: format!("geo:{:.6},{:.6}", lat, lon),
            options: action.options,
            local_id,
            random_id,
        });
        self.total_sent += 1;
        self.updated_at = chrono::Utc::now().timestamp();
        random_id
    }

    pub fn send_dice(&mut self, message: &MessageToSend, emoji: &str) -> u64 {
        let random_id = uuid::Uuid::new_v4().as_u128() as u64;
        let local_id = -((self.total_sent as i64) + 1);
        self.in_flight.push(OutboundMessagePayload {
            peer_id: message.action.peer_id,
            reply_to_msg_id: message.action.reply_to_msg_id,
            text: emoji.to_string(),
            options: message.action.options.clone(),
            local_id,
            random_id,
        });
        self.total_sent += 1;
        self.updated_at = chrono::Utc::now().timestamp();
        random_id
    }

    pub fn complete_send(&mut self, random_id: u64) -> bool {
        if let Some(pos) = self.in_flight.iter().position(|m| m.random_id == random_id) {
            self.in_flight.remove(pos);
            self.updated_at = chrono::Utc::now().timestamp();
            true
        } else {
            false
        }
    }
}
