//! Telegram Desktop 1:1 Parity Submodule: `PaymentsNonPanelProcess`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Upstream Sources: `Telegram/SourceFiles/payments/payments_non_panel_process.cpp`, `Telegram/SourceFiles/payments/payments_non_panel_process.h`

use serde::{Deserialize, Serialize};

pub const K_CREDITS_CURRENCY: &str = "XTR";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CheckoutResult {
    #[default]
    Paid,
    Pending,
    Cancelled,
    Failed,
    Free,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CreditsFormData {
    pub form_id: u64,
    pub bot_id: u64,
    pub title: String,
    pub description: String,
    pub amount: u64,
    pub currency: String,
    pub star_gift_limited_count: Option<i32>,
    pub star_gift_per_user_limit: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CreditsReceiptData {
    pub date: i64,
    pub currency: String,
    pub amount: u64,
    pub peer_id: i64,
    pub bot_id: u64,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NonPanelPaymentForm {
    CreditsForm(CreditsFormData),
    Receipt(CreditsReceiptData),
    RealFormPresented,
}

impl Default for NonPanelPaymentForm {
    fn default() -> Self {
        Self::RealFormPresented
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SmallBalanceResult {
    Success,
    Already,
    Blocked,
    Cancelled,
}

pub fn is_credits_invoice(currency: &str) -> bool {
    currency == K_CREDITS_CURRENCY || currency == "STARS"
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaymentsNonPanelProcess {
    pub is_enabled: bool,
    pub current_form: Option<NonPanelPaymentForm>,
    pub result: Option<CheckoutResult>,
    pub show_fireworks: bool,
    pub error_toast: Option<String>,
    pub updated_at: i64,
}

impl Default for PaymentsNonPanelProcess {
    fn default() -> Self {
        Self {
            is_enabled: true,
            current_form: None,
            result: None,
            show_fireworks: false,
            error_toast: None,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl PaymentsNonPanelProcess {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn process_credits_payment(&mut self, form: CreditsFormData) -> Result<(), &'static str> {
        if !is_credits_invoice(&form.currency) {
            return Err("Not a credits invoice");
        }
        self.current_form = Some(NonPanelPaymentForm::CreditsForm(form));
        self.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    pub fn process_credits_receipt(&mut self, receipt: CreditsReceiptData) {
        self.current_form = Some(NonPanelPaymentForm::Receipt(receipt));
        self.result = Some(CheckoutResult::Paid);
        self.show_fireworks = true;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn handle_small_balance_result(&mut self, result: SmallBalanceResult) -> Option<CheckoutResult> {
        let outcome = match result {
            SmallBalanceResult::Success | SmallBalanceResult::Already => {
                self.show_fireworks = true;
                CheckoutResult::Paid
            }
            SmallBalanceResult::Blocked => CheckoutResult::Failed,
            SmallBalanceResult::Cancelled => CheckoutResult::Cancelled,
        };
        self.result = Some(outcome);
        self.updated_at = chrono::Utc::now().timestamp();
        Some(outcome)
    }

    pub fn handle_stargift_error(&mut self, error_code: &str, limited_count: Option<i32>) {
        if error_code == "STARGIFT_USAGE_LIMITED" {
            self.error_toast = Some(format!(
                "Gift is sold out (limit: {})",
                limited_count.unwrap_or(0)
            ));
        } else if error_code == "STARGIFT_USER_USAGE_LIMITED" {
            self.error_toast = Some("You have reached your purchase limit for this gift".to_string());
        } else {
            self.error_toast = Some(error_code.to_string());
        }
        self.result = Some(CheckoutResult::Failed);
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
