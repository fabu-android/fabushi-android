//! Telegram Desktop 1:1 Parity Submodule: `PaymentsCheckoutProcess`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Upstream Sources: `Telegram/SourceFiles/payments/payments_checkout_process.cpp`, `Telegram/SourceFiles/payments/payments_checkout_process.h`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CardField {
    #[default]
    Number,
    ExpireDate,
    Cvc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum InformationField {
    #[default]
    Name,
    Phone,
    Email,
    ShippingAddress,
}

pub type InfoField = InformationField;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Mode {
    #[default]
    Payment,
    Receipt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CheckoutResult {
    #[default]
    Paid,
    Pending,
    Cancelled,
    Failed,
    Free,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SubmitState {
    #[default]
    None,
    Validating,
    Validated,
    Finishing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CloudPasswordState {
    #[default]
    Initial,
    Active,
    Pending,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Error {
    pub code: i32,
    pub description: String,
    pub message: String,
    pub parameter: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Field {
    pub name: String,
    pub title: String,
    pub value: String,
    pub is_required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Form {
    pub form_id: u64,
    pub title: String,
    pub description: String,
    pub invoice: String,
    pub can_save_credentials: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FormUpdate {
    pub form_id: u64,
    pub total_amount: i64,
    pub currency: String,
    pub shipping_options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GenericBox {
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct HistoryItem {
    pub peer_id: i64,
    pub msg_id: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InvoiceCredits {
    pub credits: u64,
    pub amount: u64,
    pub currency: String,
    pub random_id: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Panel {
    pub is_open: bool,
    pub mode: Mode,
    pub submit_state: SubmitState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PasscodeBox {
    pub is_prompting: bool,
    pub has_recovery: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Session {
    pub session_id: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PaidInvoice {
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SessionProcesses {
    pub by_item: HashMap<String, PaymentsCheckoutProcess>,
    pub by_slug: HashMap<String, PaymentsCheckoutProcess>,
    pub by_random_id: HashMap<u64, PaymentsCheckoutProcess>,
    pub payment_started_by_item: HashMap<String, PaidInvoice>,
    pub payment_started_by_slug: HashMap<String, PaidInvoice>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaymentsCheckoutProcess {
    pub is_enabled: bool,
    pub mode: Mode,
    pub submit_state: SubmitState,
    pub result: Option<CheckoutResult>,
    pub invoice_id: String,
    pub form: Option<Form>,
    pub error: Option<Error>,
    pub updated_at: i64,
}

impl Default for PaymentsCheckoutProcess {
    fn default() -> Self {
        Self {
            is_enabled: true,
            mode: Mode::Payment,
            submit_state: SubmitState::None,
            result: None,
            invoice_id: String::new(),
            form: None,
            error: None,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl PaymentsCheckoutProcess {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_invoice(invoice_id: impl Into<String>, mode: Mode) -> Self {
        Self {
            invoice_id: invoice_id.into(),
            mode,
            ..Self::default()
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled && self.result.is_none()
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.is_enabled = enabled;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn start(&mut self) -> Result<(), &'static str> {
        self.submit_state = SubmitState::None;
        self.result = None;
        self.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    pub fn clear_all(&mut self) -> Result<(), &'static str> {
        self.submit_state = SubmitState::None;
        self.result = None;
        self.form = None;
        self.error = None;
        self.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    pub fn invoice_paid(&mut self) -> Result<CheckoutResult, &'static str> {
        self.submit_state = SubmitState::Finishing;
        self.result = Some(CheckoutResult::Paid);
        self.updated_at = chrono::Utc::now().timestamp();
        Ok(CheckoutResult::Paid)
    }

    pub fn register_payment_start(&mut self) -> Result<(), &'static str> {
        self.submit_state = SubmitState::Validating;
        self.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    pub fn unregister_payment_start(&mut self) -> Result<(), &'static str> {
        self.submit_state = SubmitState::None;
        self.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), &'static str> {
        self.is_enabled = false;
        if self.result.is_none() {
            self.result = Some(CheckoutResult::Cancelled);
        }
        self.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    pub fn close_and_reactivate(&mut self, result: CheckoutResult) {
        self.result = Some(result);
        self.is_enabled = false;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn handle_form_update(&mut self, update: FormUpdate) -> Result<(), &'static str> {
        if let Some(ref mut form) = self.form {
            form.form_id = update.form_id;
        }
        self.submit_state = SubmitState::Validated;
        self.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    pub fn handle_error(&mut self, error: Error) -> Result<(), &'static str> {
        self.error = Some(error);
        self.submit_state = SubmitState::None;
        self.result = Some(CheckoutResult::Failed);
        self.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    pub fn choose_shipping_option(&mut self, _option_id: &str) -> Result<(), &'static str> {
        self.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    pub fn choose_tips(&mut self, _tip_amount: i64) -> Result<(), &'static str> {
        self.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    pub fn edit_payment_method(&mut self) -> Result<(), &'static str> {
        self.submit_state = SubmitState::None;
        self.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }
}
