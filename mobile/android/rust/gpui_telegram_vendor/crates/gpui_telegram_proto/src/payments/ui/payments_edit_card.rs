//! Telegram Desktop 1:1 Parity Submodule: `PaymentsEditCardUi`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Disposition: `inherited_rust_reused`
//! Upstream Sources: Telegram/SourceFiles/payments/ui/payments_edit_card.cpp, Telegram/SourceFiles/payments/ui/payments_edit_card.h
//! Inherited Paths: inherited/mahayana-messaging/src/payment.rs

use super::payments_panel_data::UncheckedCardDetails;
use crate::payments::stripe::stripe_card_validator::{
    card_number_format, validate_card, validate_cvc, validate_expire_date, CardBrand, ValidationState,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PaymentsEditCardUi {
    pub is_enabled: bool,
    pub card_number: String,
    pub expire_date: String,
    pub cvc: String,
    pub cardholder_name: String,
    pub country: String,
    pub zip: String,
    pub save_card: bool,
    pub detected_brand: CardBrand,
    pub errors: Vec<String>,
}

impl PaymentsEditCardUi {
    pub fn new() -> Self {
        Self {
            is_enabled: true,
            save_card: true,
            detected_brand: CardBrand::Unknown,
            ..Self::default()
        }
    }

    pub fn set_card_number(&mut self, number: &str) {
        self.card_number = number.to_string();
        let res = validate_card(number);
        self.detected_brand = res.brand;
    }

    pub fn formatting_groups(&self) -> Vec<usize> {
        card_number_format(&self.card_number)
    }

    pub fn validate(&mut self, need_cardholder: bool, need_country: bool, need_zip: bool) -> bool {
        self.errors.clear();

        let card_res = validate_card(&self.card_number);
        if card_res.state != ValidationState::Valid {
            self.errors.push("Invalid card number".into());
        }

        let exp_res = validate_expire_date(&self.expire_date, None);
        if exp_res.state != ValidationState::Valid {
            self.errors.push("Invalid expiration date".into());
        }

        let cvc_res = validate_cvc(&self.card_number, &self.cvc);
        if cvc_res.state != ValidationState::Valid {
            self.errors.push("Invalid CVC code".into());
        }

        if need_cardholder && self.cardholder_name.trim().is_empty() {
            self.errors.push("Cardholder name is required".into());
        }

        if need_country && self.country.trim().is_empty() {
            self.errors.push("Billing country is required".into());
        }

        if need_zip && self.zip.trim().is_empty() {
            self.errors.push("Billing postal code is required".into());
        }

        self.errors.is_empty()
    }

    pub fn to_unchecked_details(&self) -> UncheckedCardDetails {
        let (month, year) = self.parse_expire_month_year();
        UncheckedCardDetails {
            number: self.card_number.chars().filter(|c| !c.is_whitespace()).collect(),
            cvc: self.cvc.clone(),
            expire_year: year,
            expire_month: month,
            cardholder_name: self.cardholder_name.clone(),
            address_country: self.country.clone(),
            address_zip: self.zip.clone(),
        }
    }

    fn parse_expire_month_year(&self) -> (u32, u32) {
        let cleaned = self.expire_date.replace('/', "");
        if cleaned.len() == 4 {
            let month = cleaned[..2].parse().unwrap_or(0);
            let year = 2000 + cleaned[2..4].parse::<u32>().unwrap_or(0);
            (month, year)
        } else {
            (0, 0)
        }
    }
}
