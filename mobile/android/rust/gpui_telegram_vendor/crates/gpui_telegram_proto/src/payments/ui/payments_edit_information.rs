//! Telegram Desktop 1:1 Parity Submodule: `PaymentsEditInformationUi`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Disposition: `inherited_rust_reused`
//! Upstream Sources: Telegram/SourceFiles/payments/ui/payments_edit_information.cpp, Telegram/SourceFiles/payments/ui/payments_edit_information.h
//! Inherited Paths: inherited/mahayana-messaging/src/payment.rs

use super::payments_panel_data::{Address, RequestedInformation};
use serde::{Deserialize, Serialize};

pub const MAX_STREET_SIZE: usize = 64;
pub const MAX_POSTCODE_SIZE: usize = 10;
pub const MAX_NAME_SIZE: usize = 64;
pub const MAX_EMAIL_SIZE: usize = 128;
pub const MAX_PHONE_SIZE: usize = 16;
pub const MIN_CITY_SIZE: usize = 2;
pub const MAX_CITY_SIZE: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PaymentsEditInformationUi {
    pub is_enabled: bool,
    pub name: String,
    pub phone: String,
    pub email: String,
    pub street1: String,
    pub street2: String,
    pub city: String,
    pub state: String,
    pub country_iso2: String,
    pub postcode: String,
    pub save_information: bool,
    pub errors: Vec<String>,
}

impl PaymentsEditInformationUi {
    pub fn new() -> Self {
        Self {
            is_enabled: true,
            save_information: true,
            ..Self::default()
        }
    }

    pub fn from_existing(info: &RequestedInformation) -> Self {
        Self {
            is_enabled: true,
            name: info.name.clone(),
            phone: info.phone.clone(),
            email: info.email.clone(),
            street1: info.shipping_address.address1.clone(),
            street2: info.shipping_address.address2.clone(),
            city: info.shipping_address.city.clone(),
            state: info.shipping_address.state.clone(),
            country_iso2: info.shipping_address.country_iso2.clone(),
            postcode: info.shipping_address.postcode.clone(),
            save_information: info.save,
            errors: Vec::new(),
        }
    }

    pub fn validate(&mut self, need_name: bool, need_phone: bool, need_email: bool, need_address: bool) -> bool {
        self.errors.clear();

        if need_name {
            if self.name.trim().is_empty() {
                self.errors.push("Name is required".into());
            } else if self.name.len() > MAX_NAME_SIZE {
                self.errors.push(format!("Name exceeds {} characters", MAX_NAME_SIZE));
            }
        }

        if need_phone {
            if self.phone.trim().is_empty() {
                self.errors.push("Phone number is required".into());
            } else if self.phone.len() > MAX_PHONE_SIZE {
                self.errors.push(format!("Phone number exceeds {} characters", MAX_PHONE_SIZE));
            }
        }

        if need_email {
            if self.email.trim().is_empty() {
                self.errors.push("Email is required".into());
            } else if self.email.len() > MAX_EMAIL_SIZE || !self.email.contains('@') {
                self.errors.push("Invalid email address".into());
            }
        }

        if need_address {
            if self.street1.trim().is_empty() || self.street1.len() > MAX_STREET_SIZE {
                self.errors.push("Invalid street address".into());
            }
            if self.city.len() < MIN_CITY_SIZE || self.city.len() > MAX_CITY_SIZE {
                self.errors.push("Invalid city".into());
            }
            if self.country_iso2.len() != 2 {
                self.errors.push("Invalid country code".into());
            }
            if self.postcode.len() > MAX_POSTCODE_SIZE {
                self.errors.push("Invalid postal code".into());
            }
        }

        self.errors.is_empty()
    }

    pub fn to_requested_information(&self) -> RequestedInformation {
        RequestedInformation {
            default_phone: String::new(),
            default_country: self.country_iso2.clone(),
            save: self.save_information,
            name: self.name.clone(),
            phone: self.phone.clone(),
            email: self.email.clone(),
            shipping_address: Address {
                address1: self.street1.clone(),
                address2: self.street2.clone(),
                city: self.city.clone(),
                state: self.state.clone(),
                country_iso2: self.country_iso2.clone(),
                postcode: self.postcode.clone(),
            },
        }
    }
}
