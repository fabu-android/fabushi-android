//! Telegram Desktop 1:1 Parity Submodule: `ApiPremiumOption`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_premium_option.cpp, Telegram/SourceFiles/api/api_premium_option.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PremiumSubscriptionOption {
    pub months: i32,
    pub monthly_amount: f64,
    pub amount: i64,
    pub currency: String,
    pub bot_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiPremiumOption {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub options: Vec<PremiumSubscriptionOption>,
    pub updated_at: i64,
}

impl Default for ApiPremiumOption {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiPremiumOption".to_string(),
            options: vec![
                PremiumSubscriptionOption {
                    months: 1,
                    monthly_amount: 4.99,
                    amount: 499,
                    currency: "USD".to_string(),
                    bot_url: None,
                },
                PremiumSubscriptionOption {
                    months: 12,
                    monthly_amount: 2.99,
                    amount: 3588,
                    currency: "USD".to_string(),
                    bot_url: None,
                },
            ],
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiPremiumOption {
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

    pub fn add_option(&mut self, option: PremiumSubscriptionOption) {
        self.options.push(option);
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
