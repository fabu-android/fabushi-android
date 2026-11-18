//! Telegram Desktop 1:1 Parity Submodule: `StripePaymentConfigurationStripe`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Disposition: `inherited_rust_reused`
//! Upstream Sources: Telegram/SourceFiles/payments/stripe/stripe_payment_configuration.h
//! Inherited Paths: inherited/mahayana-messaging/src/payment.rs

use serde::{Deserialize, Serialize};

/// Client configuration for Stripe payment processing.
/// Mirrors C++ `Stripe::PaymentConfiguration`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PaymentConfiguration {
    pub publishable_key: String,
    pub company_name: String,
}

impl PaymentConfiguration {
    pub fn new(publishable_key: impl Into<String>, company_name: impl Into<String>) -> Self {
        Self {
            publishable_key: publishable_key.into(),
            company_name: company_name.into(),
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.publishable_key.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StripePaymentConfigurationStripe {
    pub is_enabled: bool,
    pub config: PaymentConfiguration,
}

impl StripePaymentConfigurationStripe {
    pub fn new() -> Self {
        Self {
            is_enabled: true,
            config: PaymentConfiguration::default(),
        }
    }

    pub fn with_config(config: PaymentConfiguration) -> Self {
        Self {
            is_enabled: true,
            config,
        }
    }

    pub fn publishable_key(&self) -> &str {
        &self.config.publishable_key
    }

    pub fn company_name(&self) -> &str {
        &self.config.company_name
    }
}
