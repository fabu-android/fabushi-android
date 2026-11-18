//! Telegram Desktop 1:1 Parity Submodule: `StripePchStripe`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Disposition: `inherited_rust_reused`
//! Upstream Sources: Telegram/SourceFiles/payments/stripe/stripe_pch.h
//! Inherited Paths: inherited/mahayana-messaging/src/payment.rs

pub use chrono::{DateTime, Utc};
pub use serde::{Deserialize, Serialize};
pub use serde_json::{Map as JsonMap, Value as JsonValue};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StripePchStripe {
    pub is_enabled: bool,
    pub initialized_at: i64,
}

impl StripePchStripe {
    pub fn new() -> Self {
        Self {
            is_enabled: true,
            initialized_at: Utc::now().timestamp(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled
    }
}
