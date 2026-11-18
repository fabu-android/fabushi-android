//! Telegram Desktop 1:1 Parity Submodule: `PaymentsPanelDataUi`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Disposition: `inherited_rust_reused`
//! Upstream Sources: Telegram/SourceFiles/payments/ui/payments_panel_data.h
//! Inherited Paths: inherited/mahayana-messaging/src/payment.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LabeledPrice {
    pub label: String,
    pub price: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Cover {
    pub title: String,
    pub description: String,
    pub seller: String,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Receipt {
    pub date: i64,
    pub total_amount: i64,
    pub currency: String,
    pub paid: bool,
}

impl Receipt {
    pub fn is_empty(&self) -> bool {
        !self.paid
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShippingOption {
    pub id: String,
    pub title: String,
    pub prices: Vec<LabeledPrice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShippingOptions {
    pub currency: String,
    pub list: Vec<ShippingOption>,
    pub selected_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Address {
    pub address1: String,
    pub address2: String,
    pub city: String,
    pub state: String,
    pub country_iso2: String,
    pub postcode: String,
}

impl Address {
    pub fn is_valid(&self) -> bool {
        !self.address1.is_empty() && !self.city.is_empty() && !self.country_iso2.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RequestedInformation {
    pub default_phone: String,
    pub default_country: String,
    pub save: bool,
    pub name: String,
    pub phone: String,
    pub email: String,
    pub shipping_address: Address,
}

impl RequestedInformation {
    pub fn is_empty(&self) -> bool {
        self.name.is_empty()
            && self.phone.is_empty()
            && self.email.is_empty()
            && !self.shipping_address.is_valid()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InformationField {
    ShippingStreet,
    ShippingCity,
    ShippingState,
    ShippingCountry,
    ShippingPostcode,
    Name,
    Email,
    Phone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NativeMethodDetails {
    pub default_country: String,
    pub supported: bool,
    pub need_country: bool,
    pub need_zip: bool,
    pub need_cardholder_name: bool,
    pub can_save_information: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PaymentMethodSaved {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PaymentMethodAdditional {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PaymentMethodDetails {
    pub native: NativeMethodDetails,
    pub saved_methods: Vec<PaymentMethodSaved>,
    pub additional_methods: Vec<PaymentMethodAdditional>,
    pub url: String,
    pub provider: String,
    pub saved_method_index: usize,
    pub can_save_information: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardField {
    Number,
    Cvc,
    ExpireDate,
    Name,
    AddressCountry,
    AddressZip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UncheckedCardDetails {
    pub number: String,
    pub cvc: String,
    pub expire_year: u32,
    pub expire_month: u32,
    pub cardholder_name: String,
    pub address_country: String,
    pub address_zip: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Invoice {
    pub cover: Cover,
    pub prices: Vec<LabeledPrice>,
    pub suggested_tips: Vec<i64>,
    pub tips_max: i64,
    pub tips_selected: i64,
    pub currency: String,
    pub receipt: Receipt,
    pub is_name_requested: bool,
    pub is_phone_requested: bool,
    pub is_email_requested: bool,
    pub is_shipping_address_requested: bool,
    pub is_recurring: bool,
    pub is_flexible: bool,
    pub is_test: bool,
    pub provider: String,
    pub terms_url: String,
    pub phone_sent_to_provider: bool,
    pub email_sent_to_provider: bool,
}

impl Invoice {
    pub fn is_valid(&self) -> bool {
        !self.currency.is_empty() && (!self.prices.is_empty() || self.tips_max > 0)
    }

    pub fn total_amount(&self) -> i64 {
        let base_prices: i64 = self.prices.iter().map(|p| p.price).sum();
        base_prices.saturating_add(self.tips_selected)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PaymentsPanelDataUi {
    pub is_enabled: bool,
    pub invoice: Invoice,
    pub info: RequestedInformation,
    pub shipping: ShippingOptions,
}

impl PaymentsPanelDataUi {
    pub fn new() -> Self {
        Self {
            is_enabled: true,
            invoice: Invoice::default(),
            info: RequestedInformation::default(),
            shipping: ShippingOptions::default(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.invoice.is_valid()
    }
}
