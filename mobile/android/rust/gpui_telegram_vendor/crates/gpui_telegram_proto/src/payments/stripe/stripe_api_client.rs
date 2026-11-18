//! Telegram Desktop 1:1 Parity Submodule: `StripeApiClientStripe`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Upstream Sources: `Telegram/SourceFiles/payments/stripe/stripe_api_client.cpp`, `Telegram/SourceFiles/payments/stripe/stripe_api_client.h`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const API_URL_BASE: &str = "api.stripe.com/v1";
pub const TOKEN_ENDPOINT: &str = "tokens";
pub const STRIPE_API_VERSION: &str = "2015-10-12";
pub const SDK_VERSION: &str = "9.1.0";

pub fn stripe_user_agent_details() -> String {
    r#"{"lang":"objective-c","bindings_version":"9.1.0"}"#.to_string()
}

pub fn token_url() -> String {
    format!("https://{}/{}", API_URL_BASE, TOKEN_ENDPOINT)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PaymentConfiguration {
    pub publishable_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StripeApiClientStripe {
    pub is_enabled: bool,
    pub api_url: String,
    pub publishable_key: String,
    pub additional_headers: HashMap<String, String>,
    pub updated_at: i64,
}

impl Default for StripeApiClientStripe {
    fn default() -> Self {
        Self::new("")
    }
}

impl StripeApiClientStripe {
    pub fn new(publishable_key: impl Into<String>) -> Self {
        let key = publishable_key.into();
        let mut headers = HashMap::new();
        headers.insert("X-Stripe-User-Agent".to_string(), stripe_user_agent_details());
        headers.insert("Stripe-Version".to_string(), STRIPE_API_VERSION.to_string());
        headers.insert("Authorization".to_string(), format!("Bearer {}", key));
        headers.insert("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string());

        Self {
            is_enabled: true,
            api_url: token_url(),
            publishable_key: key,
            additional_headers: headers,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled && !self.publishable_key.is_empty()
    }

    pub fn build_token_request(&mut self, form_encoded_card: &str) -> (String, String, HashMap<String, String>) {
        self.updated_at = chrono::Utc::now().timestamp();
        (self.api_url.clone(), form_encoded_card.to_string(), self.additional_headers.clone())
    }
}
