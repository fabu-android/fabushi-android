//! Telegram Desktop 1:1 Parity Submodule: `PaymentsPanelDelegateUi`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Disposition: `inherited_rust_reused`
//! Upstream Sources: Telegram/SourceFiles/payments/ui/payments_panel_delegate.h
//! Inherited Paths: inherited/mahayana-messaging/src/payment.rs

use super::payments_panel_data::{RequestedInformation, UncheckedCardDetails};
use serde::{Deserialize, Serialize};

/// Trait defining the callback interface for user actions in checkout drawer panels.
/// Mirrors C++ `Payments::Ui::PanelDelegate`.
pub trait PanelDelegate: Send + Sync {
    fn panel_request_close(&mut self);
    fn panel_close_sure(&mut self);
    fn panel_submit(&mut self);
    fn panel_trust_and_submit(&mut self);
    fn panel_accept_terms_and_submit(&mut self);
    fn panel_cancel_edit(&mut self);
    fn panel_edit_payment_method(&mut self);
    fn panel_edit_shipping_information(&mut self);
    fn panel_edit_name(&mut self);
    fn panel_edit_email(&mut self);
    fn panel_edit_phone(&mut self);
    fn panel_choose_shipping_option(&mut self);
    fn panel_change_shipping_option(&mut self, id: &str);
    fn panel_choose_tips(&mut self);
    fn panel_change_tips(&mut self, value: i64);
    fn panel_validate_information(&mut self, data: RequestedInformation);
    fn panel_validate_card(&mut self, data: UncheckedCardDetails, save_information: bool);
}

/// Default implementation of PanelDelegate recording events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PaymentsPanelDelegateUi {
    pub is_enabled: bool,
    pub is_closed: bool,
    pub is_submitted: bool,
    pub selected_shipping_option_id: Option<String>,
    pub selected_tip_amount: i64,
    pub submitted_info: Option<RequestedInformation>,
    pub submitted_card: Option<UncheckedCardDetails>,
    pub save_card: bool,
}

impl PaymentsPanelDelegateUi {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PanelDelegate for PaymentsPanelDelegateUi {
    fn panel_request_close(&mut self) {
        self.is_closed = true;
    }

    fn panel_close_sure(&mut self) {
        self.is_closed = true;
    }

    fn panel_submit(&mut self) {
        self.is_submitted = true;
    }

    fn panel_trust_and_submit(&mut self) {
        self.is_submitted = true;
    }

    fn panel_accept_terms_and_submit(&mut self) {
        self.is_submitted = true;
    }

    fn panel_cancel_edit(&mut self) {}

    fn panel_edit_payment_method(&mut self) {}

    fn panel_edit_shipping_information(&mut self) {}

    fn panel_edit_name(&mut self) {}

    fn panel_edit_email(&mut self) {}

    fn panel_edit_phone(&mut self) {}

    fn panel_choose_shipping_option(&mut self) {}

    fn panel_change_shipping_option(&mut self, id: &str) {
        self.selected_shipping_option_id = Some(id.to_string());
    }

    fn panel_choose_tips(&mut self) {}

    fn panel_change_tips(&mut self, value: i64) {
        self.selected_tip_amount = value;
    }

    fn panel_validate_information(&mut self, data: RequestedInformation) {
        self.submitted_info = Some(data);
    }

    fn panel_validate_card(&mut self, data: UncheckedCardDetails, save_information: bool) {
        self.submitted_card = Some(data);
        self.save_card = save_information;
    }
}
