//! Telegram Desktop 1:1 Parity Submodule: `StripeFormEncodableStripe`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Disposition: `inherited_rust_reused`
//! Upstream Sources: Telegram/SourceFiles/payments/stripe/stripe_form_encodable.h
//! Inherited Paths: inherited/mahayana-messaging/src/payment.rs

use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

/// Trait defining an object that can be serialized into Stripe form-urlencoded payload.
/// Mirrors C++ `Stripe::FormEncodable`.
pub trait FormEncodable: Send + Sync {
    fn root_object_name(&self) -> &str;
    fn form_field_values(&self) -> BTreeMap<String, String>;
}

/// Generic container implementing FormEncodable.
/// Mirrors C++ `Stripe::MakeEncodable<T>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MakeEncodable {
    pub root: String,
    pub fields: BTreeMap<String, String>,
}

impl MakeEncodable {
    pub fn new(root: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            fields: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.fields.insert(key.into(), value.into());
        self
    }
}

impl FormEncodable for MakeEncodable {
    fn root_object_name(&self) -> &str {
        &self.root
    }

    fn form_field_values(&self) -> BTreeMap<String, String> {
        self.fields.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StripeFormEncodableStripe {
    pub is_enabled: bool,
    pub registered_types: Vec<String>,
}

impl StripeFormEncodableStripe {
    pub fn new() -> Self {
        Self {
            is_enabled: true,
            registered_types: vec!["card".into(), "bank_account".into()],
        }
    }

    pub fn create_encodable(&self, root: &str) -> MakeEncodable {
        MakeEncodable::new(root)
    }
}
