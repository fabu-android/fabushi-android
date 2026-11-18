//! Telegram Desktop 1:1 Parity Submodule: `SmartglocalTokenSmartglocal`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Upstream Sources: `Telegram/SourceFiles/payments/smartglocal/smartglocal_token.cpp`, `Telegram/SourceFiles/payments/smartglocal/smartglocal_token.h`

use serde::{Deserialize, Serialize};
use super::smartglocal_card::Card;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Token {
    pub token_id: String,
    pub card: Option<Card>,
}

impl Token {
    pub fn empty() -> Self {
        Self {
            token_id: String::new(),
            card: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.token_id.is_empty()
    }

    pub fn decoded_object_from_api_response(val: &serde_json::Value) -> Self {
        let token_id = val.get("token").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if token_id.is_empty() {
            return Self::empty();
        }
        let card = val.get("info").map(Card::decoded_object_from_api_response);
        Self {
            token_id,
            card,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartglocalTokenSmartglocal {
    pub is_enabled: bool,
    pub token: Token,
    pub updated_at: i64,
}

impl Default for SmartglocalTokenSmartglocal {
    fn default() -> Self {
        Self {
            is_enabled: true,
            token: Token::empty(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl SmartglocalTokenSmartglocal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_token(token: Token) -> Self {
        Self {
            token,
            ..Self::default()
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled && !self.token.is_empty()
    }

    pub fn token_id(&self) -> &str {
        &self.token.token_id
    }

    pub fn card(&self) -> Option<&Card> {
        self.token.card.as_ref()
    }
}
