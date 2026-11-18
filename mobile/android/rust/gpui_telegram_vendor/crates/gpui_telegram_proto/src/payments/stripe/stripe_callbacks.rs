//! Telegram Desktop 1:1 Parity Submodule: `StripeCallbacksStripe`
//! Subsystem: `payments` | Feature ID: `TCG-N24`
//! Upstream Sources: `Telegram/SourceFiles/payments/stripe/stripe_callbacks.h`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StripeTokenData {
    pub token_id: String,
    pub card_id: String,
    pub card_last4: String,
    pub card_brand: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StripeErrorData {
    pub code: String,
    pub message: String,
    pub param: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StripeCallbacksStripe {
    pub is_enabled: bool,
    pub token: Option<StripeTokenData>,
    pub error: Option<StripeErrorData>,
    pub is_completed: bool,
    pub updated_at: i64,
}

impl Default for StripeCallbacksStripe {
    fn default() -> Self {
        Self {
            is_enabled: true,
            token: None,
            error: None,
            is_completed: false,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl StripeCallbacksStripe {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled && !self.is_completed
    }

    pub fn on_success(&mut self, token: StripeTokenData) {
        self.token = Some(token);
        self.error = None;
        self.is_completed = true;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn on_error(&mut self, error: StripeErrorData) {
        self.error = Some(error);
        self.token = None;
        self.is_completed = true;
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
