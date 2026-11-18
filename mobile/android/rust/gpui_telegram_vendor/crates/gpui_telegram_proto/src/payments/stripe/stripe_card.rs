//! Telegram Desktop 1:1 Parity Submodule: `StripeCardStripe`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Upstream Sources: `Telegram/SourceFiles/payments/stripe/stripe_card.cpp`, `Telegram/SourceFiles/payments/stripe/stripe_card.h`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CardBrand {
    Visa,
    Amex,
    MasterCard,
    Discover,
    JCB,
    DinersClub,
    UnionPay,
    #[default]
    Unknown,
}

pub fn brand_from_string(brand: &str) -> CardBrand {
    match brand.to_ascii_lowercase().as_str() {
        "visa" => CardBrand::Visa,
        "american express" | "amex" => CardBrand::Amex,
        "mastercard" => CardBrand::MasterCard,
        "discover" => CardBrand::Discover,
        "jcb" => CardBrand::JCB,
        "diners club" | "diners" => CardBrand::DinersClub,
        "unionpay" => CardBrand::UnionPay,
        _ => CardBrand::Unknown,
    }
}

pub fn card_brand_to_string(brand: CardBrand) -> &'static str {
    match brand {
        CardBrand::Amex => "American Express",
        CardBrand::DinersClub => "Diners Club",
        CardBrand::Discover => "Discover",
        CardBrand::JCB => "JCB",
        CardBrand::MasterCard => "MasterCard",
        CardBrand::UnionPay => "UnionPay",
        CardBrand::Visa => "Visa",
        CardBrand::Unknown => "Unknown",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CardFundingType {
    Debit,
    Credit,
    Prepaid,
    #[default]
    Other,
}

pub fn funding_from_string(funding: &str) -> CardFundingType {
    match funding.to_ascii_lowercase().as_str() {
        "credit" => CardFundingType::Credit,
        "debit" => CardFundingType::Debit,
        "prepaid" => CardFundingType::Prepaid,
        _ => CardFundingType::Other,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Card {
    pub card_id: String,
    pub name: String,
    pub last4: String,
    pub dynamic_last4: String,
    pub brand: CardBrand,
    pub funding: CardFundingType,
    pub fingerprint: String,
    pub country: String,
    pub currency: String,
    pub exp_month: u32,
    pub exp_year: u32,
    pub address_line1: String,
    pub address_line2: String,
    pub address_city: String,
    pub address_state: String,
    pub address_zip: String,
    pub address_country: String,
}

impl Card {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.card_id.is_empty()
    }

    pub fn decoded_object_from_api_response(val: &serde_json::Value) -> Self {
        let card_id = val.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if card_id.is_empty() {
            return Self::empty();
        }
        let last4 = val.get("last4").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let brand_str = val.get("brand").and_then(|v| v.as_str()).unwrap_or("");
        let brand = brand_from_string(brand_str);
        let funding_str = val.get("funding").and_then(|v| v.as_str()).unwrap_or("");
        let funding = funding_from_string(funding_str);
        let exp_month = val.get("exp_month").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let exp_year = val.get("exp_year").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

        Self {
            card_id,
            name: val.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            last4,
            dynamic_last4: val.get("dynamic_last4").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            brand,
            funding,
            fingerprint: val.get("fingerprint").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            country: val.get("country").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            currency: val.get("currency").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            exp_month,
            exp_year,
            address_line1: val.get("address_line1").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            address_line2: val.get("address_line2").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            address_city: val.get("address_city").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            address_state: val.get("address_state").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            address_zip: val.get("address_zip").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            address_country: val.get("address_country").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StripeCardStripe {
    pub is_enabled: bool,
    pub card: Card,
    pub updated_at: i64,
}

impl Default for StripeCardStripe {
    fn default() -> Self {
        Self {
            is_enabled: true,
            card: Card::empty(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl StripeCardStripe {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_card(card: Card) -> Self {
        Self {
            card,
            ..Self::default()
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled && !self.card.is_empty()
    }

    pub fn brand_name(&self) -> &'static str {
        card_brand_to_string(self.card.brand)
    }
}
