//! Telegram Desktop 1:1 Parity Submodule: `PaymentsPanelUi`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Disposition: `inherited_rust_reused`
//! Upstream Sources: Telegram/SourceFiles/payments/ui/payments_panel.cpp, Telegram/SourceFiles/payments/ui/payments_panel.h
//! Inherited Paths: inherited/mahayana-messaging/src/payment.rs

use super::payments_edit_card::PaymentsEditCardUi;
use super::payments_edit_information::PaymentsEditInformationUi;
use super::payments_form_summary::PaymentsFormSummaryUi;
use super::payments_panel_data::{Invoice, Receipt};
use serde::{Deserialize, Serialize};

pub const PROGRESS_DURATION_MS: u64 = 200;
pub const PROGRESS_OPACITY: f32 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PanelStep {
    #[default]
    Summary,
    EditInformation,
    EditCard,
    Processing,
    Receipt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PaymentsPanelUi {
    pub is_enabled: bool,
    pub step: PanelStep,
    pub is_progress_active: bool,
    pub is_closed: bool,
    pub summary: Option<PaymentsFormSummaryUi>,
    pub edit_info: Option<PaymentsEditInformationUi>,
    pub edit_card: Option<PaymentsEditCardUi>,
    pub receipt: Option<Receipt>,
}

impl PaymentsPanelUi {
    pub fn new(invoice: Invoice) -> Self {
        let summary = PaymentsFormSummaryUi::new(invoice);
        Self {
            is_enabled: true,
            step: PanelStep::Summary,
            is_progress_active: false,
            is_closed: false,
            summary: Some(summary),
            edit_info: None,
            edit_card: None,
            receipt: None,
        }
    }

    pub fn show_edit_information(&mut self) {
        if self.edit_info.is_none() {
            self.edit_info = Some(PaymentsEditInformationUi::new());
        }
        self.step = PanelStep::EditInformation;
    }

    pub fn show_edit_card(&mut self) {
        if self.edit_card.is_none() {
            self.edit_card = Some(PaymentsEditCardUi::new());
        }
        self.step = PanelStep::EditCard;
    }

    pub fn return_to_summary(&mut self) {
        self.step = PanelStep::Summary;
    }

    pub fn toggle_progress(&mut self, active: bool) {
        self.is_progress_active = active;
        if active {
            self.step = PanelStep::Processing;
        } else if self.step == PanelStep::Processing {
            self.step = PanelStep::Summary;
        }
    }

    pub fn complete_payment(&mut self, receipt: Receipt) {
        self.is_progress_active = false;
        self.receipt = Some(receipt);
        self.step = PanelStep::Receipt;
    }

    pub fn close(&mut self) {
        self.is_closed = true;
    }
}
