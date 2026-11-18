//! Telegram Desktop 1:1 Parity Submodule: `ApiPremium`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_premium.cpp, Telegram/SourceFiles/api/api_premium.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GiftCode {
    pub from_peer_id: u64,
    pub to_peer_id: u64,
    pub date: i64,
    pub used_date: i64,
    pub days: i32,
    pub giveaway: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiPremium {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub is_premium: bool,
    pub monthly_amount: i64,
    pub monthly_currency: String,
    pub gift_codes: HashMap<String, GiftCode>,
    pub updated_at: i64,
}

impl Default for ApiPremium {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiPremium".to_string(),
            is_premium: false,
            monthly_amount: 499,
            monthly_currency: "USD".to_string(),
            gift_codes: HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiPremium {
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

    pub fn set_premium(&mut self, premium: bool) {
        self.is_premium = premium;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn check_gift_code(&mut self, slug: &str) -> Option<GiftCode> {
        self.gift_codes.get(slug).cloned()
    }

    pub fn register_gift_code(&mut self, slug: impl Into<String>, code: GiftCode) {
        self.gift_codes.insert(slug.into(), code);
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
