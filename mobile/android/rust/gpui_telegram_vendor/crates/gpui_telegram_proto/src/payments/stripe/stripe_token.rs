//! Telegram Desktop 1:1 Parity Submodule: `StripeTokenStripe`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Disposition: `inherited_rust_reused`
//! Upstream Sources: Telegram/SourceFiles/payments/stripe/stripe_token.cpp, Telegram/SourceFiles/payments/stripe/stripe_token.h
//! Inherited Paths: inherited/mahayana-messaging/src/payment.rs

use serde::{Deserialize, Serialize};

/// Represents a tokenized payment credential returned by Stripe API.
/// Mirrors C++ `Stripe::Token`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Token {
    pub token_id: String,
    pub livemode: bool,
    pub created: i64,
    pub card_id: Option<String>,
    pub card_last4: Option<String>,
    pub card_brand: Option<String>,
}

impl Token {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.token_id.is_empty()
    }

    pub fn decoded_object_from_api_response(object: &serde_json::Value) -> Self {
        let Some(obj) = object.as_object() else {
            return Self::empty();
        };

        let token_id = obj
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if token_id.is_empty() {
            return Self::empty();
        }

        let livemode = obj
            .get("livemode")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let created = obj
            .get("created")
            .and_then(|v| v.as_i64())
            .unwrap_or_else(|| chrono::Utc::now().timestamp());

        let mut card_id = None;
        let mut card_last4 = None;
        let mut card_brand = None;

        if let Some(card_obj) = obj.get("card").and_then(|v| v.as_object()) {
            card_id = card_obj.get("id").and_then(|v| v.as_str()).map(String::from);
            card_last4 = card_obj.get("last4").and_then(|v| v.as_str()).map(String::from);
            card_brand = card_obj.get("brand").and_then(|v| v.as_str()).map(String::from);
        }

        Self {
            token_id,
            livemode,
            created,
            card_id,
            card_last4,
            card_brand,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StripeTokenStripe {
    pub is_enabled: bool,
    pub last_token: Option<Token>,
}

impl StripeTokenStripe {
    pub fn new() -> Self {
        Self {
            is_enabled: true,
            last_token: None,
        }
    }

    pub fn decode(&mut self, object: &serde_json::Value) -> Token {
        let tok = Token::decoded_object_from_api_response(object);
        if !tok.is_empty() {
            self.last_token = Some(tok.clone());
        }
        tok
    }

    pub fn empty(&self) -> Token {
        Token::empty()
    }
}
