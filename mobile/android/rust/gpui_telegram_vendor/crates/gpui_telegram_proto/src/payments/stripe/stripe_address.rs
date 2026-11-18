//! Telegram Desktop 1:1 Parity Submodule: `StripeAddressStripe`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Upstream Sources: `Telegram/SourceFiles/payments/stripe/stripe_address.h`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BillingAddressFields {
    #[default]
    None,
    Zip,
    Full,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StripeAddress {
    pub country: String,
    pub state: String,
    pub city: String,
    pub line1: String,
    pub line2: String,
    pub postal_code: String,
}

impl StripeAddress {
    pub fn is_valid_for(&self, requirement: BillingAddressFields) -> bool {
        match requirement {
            BillingAddressFields::None => true,
            BillingAddressFields::Zip => !self.postal_code.trim().is_empty(),
            BillingAddressFields::Full => {
                !self.line1.trim().is_empty()
                    && !self.city.trim().is_empty()
                    && !self.country.trim().is_empty()
                    && !self.postal_code.trim().is_empty()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StripeAddressStripe {
    pub is_enabled: bool,
    pub address: StripeAddress,
    pub requirement: BillingAddressFields,
    pub updated_at: i64,
}

impl Default for StripeAddressStripe {
    fn default() -> Self {
        Self {
            is_enabled: true,
            address: StripeAddress::default(),
            requirement: BillingAddressFields::None,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl StripeAddressStripe {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_address(address: StripeAddress, requirement: BillingAddressFields) -> Self {
        Self {
            address,
            requirement,
            ..Self::default()
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled && self.address.is_valid_for(self.requirement)
    }

    pub fn is_valid(&self) -> bool {
        self.address.is_valid_for(self.requirement)
    }
}
