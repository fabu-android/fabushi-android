//! Telegram Desktop 1:1 Parity Submodule: `PaymentsFormSummaryUi`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Disposition: `inherited_rust_reused`
//! Upstream Sources: Telegram/SourceFiles/payments/ui/payments_form_summary.cpp, Telegram/SourceFiles/payments/ui/payments_form_summary.h
//! Inherited Paths: inherited/mahayana-messaging/src/payment.rs

use super::payments_panel_data::{Invoice, LabeledPrice, ShippingOption};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PaymentsFormSummaryUi {
    pub is_enabled: bool,
    pub invoice: Invoice,
    pub selected_shipping: Option<ShippingOption>,
    pub selected_tip: i64,
}

impl PaymentsFormSummaryUi {
    pub fn new(invoice: Invoice) -> Self {
        let initial_tip = invoice.tips_selected;
        Self {
            is_enabled: true,
            invoice,
            selected_shipping: None,
            selected_tip: initial_tip,
        }
    }

    pub fn compute_total_amount(&self) -> i64 {
        let items_sum: i64 = self.invoice.prices.iter().map(|p| p.price).sum();
        let shipping_sum: i64 = self
            .selected_shipping
            .as_ref()
            .map(|s| s.prices.iter().map(|p| p.price).sum())
            .unwrap_or(0);
        items_sum.saturating_add(shipping_sum).saturating_add(self.selected_tip)
    }

    pub fn format_amount(amount: i64, currency: &str) -> String {
        let dollars = amount / 100;
        let cents = (amount % 100).abs();
        format!("{}.{:02} {}", dollars, cents, currency.to_uppercase())
    }

    pub fn formatted_total(&self) -> String {
        Self::format_amount(self.compute_total_amount(), &self.invoice.currency)
    }

    pub fn suggested_tips_amounts(&self) -> Vec<i64> {
        if !self.invoice.suggested_tips.is_empty() {
            return self.invoice.suggested_tips.clone();
        }
        let items_sum: i64 = self.invoice.prices.iter().map(|p| p.price).sum();
        if items_sum <= 0 {
            return Vec::new();
        }
        // Presets: 5%, 10%, 15%, 20%
        vec![
            (items_sum * 5) / 100,
            (items_sum * 10) / 100,
            (items_sum * 15) / 100,
            (items_sum * 20) / 100,
        ]
    }

    pub fn select_tip(&mut self, amount: i64) {
        if self.invoice.tips_max <= 0 || amount <= self.invoice.tips_max {
            self.selected_tip = amount;
        }
    }

    pub fn set_shipping_option(&mut self, option: ShippingOption) {
        self.selected_shipping = Some(option);
    }

    pub fn line_items(&self) -> Vec<LabeledPrice> {
        let mut list = self.invoice.prices.clone();
        if let Some(shipping) = &self.selected_shipping {
            for p in &shipping.prices {
                list.push(p.clone());
            }
        }
        if self.selected_tip > 0 {
            list.push(LabeledPrice {
                label: "Tip".into(),
                price: self.selected_tip,
            });
        }
        list
    }
}
