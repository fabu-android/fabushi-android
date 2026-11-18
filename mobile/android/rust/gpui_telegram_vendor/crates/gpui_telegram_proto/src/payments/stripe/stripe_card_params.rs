//! Telegram Desktop 1:1 Parity Submodule: `StripeCardParamsStripe`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Upstream Sources: `Telegram/SourceFiles/payments/stripe/stripe_card_params.cpp`, `Telegram/SourceFiles/payments/stripe/stripe_card_params.h`

use serde::{Deserialize, Serialize};

fn percent_encode(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len());
    for b in s.bytes() {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CardParams {
    pub number: String,
    pub exp_month: u32,
    pub exp_year: u32,
    pub cvc: String,
    pub name: String,
    pub address_line1: String,
    pub address_line2: String,
    pub address_city: String,
    pub address_state: String,
    pub address_zip: String,
    pub address_country: String,
    pub currency: String,
}

impl CardParams {
    pub fn root_object_name() -> &'static str {
        "card"
    }

    pub fn form_field_values(&self) -> Vec<(&'static str, String)> {
        vec![
            ("number", self.number.clone()),
            ("cvc", self.cvc.clone()),
            ("name", self.name.clone()),
            ("address_line1", self.address_line1.clone()),
            ("address_line2", self.address_line2.clone()),
            ("address_city", self.address_city.clone()),
            ("address_state", self.address_state.clone()),
            ("address_zip", self.address_zip.clone()),
            ("address_country", self.address_country.clone()),
            ("exp_month", self.exp_month.to_string()),
            ("exp_year", self.exp_year.to_string()),
            ("currency", self.currency.clone()),
        ]
    }

    pub fn to_form_encoded(&self) -> String {
        let prefix = Self::root_object_name();
        self.form_field_values()
            .into_iter()
            .filter(|(_, val)| !val.is_empty())
            .map(|(key, val)| {
                format!("{}[{}]={}", prefix, key, percent_encode(&val))
            })
            .collect::<Vec<_>>()
            .join("&")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StripeCardParamsStripe {
    pub is_enabled: bool,
    pub params: CardParams,
    pub updated_at: i64,
}

impl Default for StripeCardParamsStripe {
    fn default() -> Self {
        Self {
            is_enabled: true,
            params: CardParams::default(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl StripeCardParamsStripe {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_params(params: CardParams) -> Self {
        Self {
            params,
            ..Self::default()
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled && !self.params.number.is_empty()
    }

    pub fn to_form_encoded(&self) -> String {
        self.params.to_form_encoded()
    }
}
