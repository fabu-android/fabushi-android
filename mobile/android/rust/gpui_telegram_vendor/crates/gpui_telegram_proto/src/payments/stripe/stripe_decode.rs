//! Telegram Desktop 1:1 Parity Submodule: `StripeDecodeStripe`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Disposition: `inherited_rust_reused`
//! Upstream Sources: Telegram/SourceFiles/payments/stripe/stripe_decode.cpp, Telegram/SourceFiles/payments/stripe/stripe_decode.h
//! Inherited Paths: inherited/mahayana-messaging/src/payment.rs

use serde::{Deserialize, Serialize};

/// Utility checking whether a JSON object contains all required keys.
/// Mirrors `Stripe::ContainsFields(const QJsonObject &object, std::vector<QStringView> keys)`.
pub fn contains_fields(object: &serde_json::Value, keys: &[&str]) -> bool {
    let Some(map) = object.as_object() else {
        return false;
    };
    for key in keys {
        if !map.contains_key(*key) || map.get(*key).is_some_and(|v| v.is_null()) {
            return false;
        }
    }
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StripeDecodeStripe {
    pub is_enabled: bool,
    pub decoded_count: u64,
}

impl Default for StripeDecodeStripe {
    fn default() -> Self {
        Self {
            is_enabled: true,
            decoded_count: 0,
        }
    }
}

impl StripeDecodeStripe {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn contains_fields(&mut self, object: &serde_json::Value, keys: &[&str]) -> bool {
        let res = contains_fields(object, keys);
        if res {
            self.decoded_count += 1;
        }
        res
    }
}
