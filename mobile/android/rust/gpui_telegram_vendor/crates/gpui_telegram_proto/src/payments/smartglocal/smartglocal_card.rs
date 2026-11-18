//! Telegram Desktop 1:1 Parity Submodule: `SmartglocalCardSmartglocal`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Upstream Sources: `Telegram/SourceFiles/payments/smartglocal/smartglocal_card.cpp`, `Telegram/SourceFiles/payments/smartglocal/smartglocal_card.h`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Card {
    pub card_type: String,
    pub network: String,
    pub masked_number: String,
}

impl Card {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.card_type.is_empty() || self.masked_number.is_empty()
    }

    pub fn last4(&self) -> String {
        extract_last4(&self.masked_number)
    }

    pub fn decoded_object_from_api_response(val: &serde_json::Value) -> Self {
        let card_type = val.get("card_type").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let network = val.get("card_network").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let masked_number = val.get("masked_card_number").and_then(|v| v.as_str()).unwrap_or("").to_string();

        if card_type.is_empty() || masked_number.is_empty() {
            Self::empty()
        } else {
            Self {
                card_type,
                network,
                masked_number,
            }
        }
    }
}

pub fn extract_last4(masked: &str) -> String {
    let digits: String = masked.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 4 {
        digits[digits.len() - 4..].to_string()
    } else {
        String::new()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartglocalCardSmartglocal {
    pub is_enabled: bool,
    pub card: Card,
    pub updated_at: i64,
}

impl Default for SmartglocalCardSmartglocal {
    fn default() -> Self {
        Self {
            is_enabled: true,
            card: Card::empty(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl SmartglocalCardSmartglocal {
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

    pub fn last4(&self) -> String {
        self.card.last4()
    }
}
