//! Telegram Desktop 1:1 Parity Submodule: `StripeFormEncoderStripe`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Disposition: `inherited_rust_reused`
//! Upstream Sources: Telegram/SourceFiles/payments/stripe/stripe_form_encoder.cpp, Telegram/SourceFiles/payments/stripe/stripe_form_encoder.h
//! Inherited Paths: inherited/mahayana-messaging/src/payment.rs

use super::stripe_form_encodable::FormEncodable;
use serde::{Deserialize, Serialize};

/// Standard URL percent-encoding for form fields.
/// Mirrors Qt's `QUrl::toPercentEncoding(value)`.
pub fn to_percent_encoding(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len() * 3);
    for b in input.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}

pub struct FormEncoder;

impl FormEncoder {
    /// Form-encodes an object into an application/x-www-form-urlencoded payload:
    /// `root[key1]=val1&root[key2]=val2` with keys sorted alphabetically.
    /// Mirrors C++ `Stripe::FormEncoder::formEncodedDataForObject`.
    pub fn form_encoded_data_for_object(object: &dyn FormEncodable) -> Vec<u8> {
        let root = object.root_object_name();
        let values = object.form_field_values();

        let mut sorted_keys: Vec<&String> = values
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, _)| k)
            .collect();
        sorted_keys.sort();

        let mut parts = Vec::new();
        for key in sorted_keys {
            if let Some(val) = values.get(key) {
                let encoded_root = to_percent_encoding(root);
                let encoded_key = to_percent_encoding(key);
                let encoded_val = to_percent_encoding(val);
                parts.push(format!("{}[{}]={}", encoded_root, encoded_key, encoded_val));
            }
        }

        parts.join("&").into_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StripeFormEncoderStripe {
    pub is_enabled: bool,
    pub encodes_count: u64,
}

impl StripeFormEncoderStripe {
    pub fn new() -> Self {
        Self {
            is_enabled: true,
            encodes_count: 0,
        }
    }

    pub fn encode(&mut self, object: &dyn FormEncodable) -> Vec<u8> {
        self.encodes_count += 1;
        FormEncoder::form_encoded_data_for_object(object)
    }

    pub fn to_percent_encoding(&self, input: &str) -> String {
        to_percent_encoding(input)
    }
}
