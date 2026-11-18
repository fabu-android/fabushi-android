//! Telegram Desktop 1:1 Parity Submodule: `PaymentsForm`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Upstream Sources: `Telegram/SourceFiles/payments/payments_form.cpp`, `Telegram/SourceFiles/payments/payments_form.h`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FormDetails {
    pub form_id: u64,
    pub url: String,
    pub native_provider: String,
    pub terms_bot_username: String,
    pub native_params_json: Vec<u8>,
    pub bot_id: u64,
    pub provider_id: u64,
    pub can_save_credentials: bool,
    pub password_missing: bool,
    pub terms_accepted: bool,
}

impl FormDetails {
    pub fn is_valid(&self) -> bool {
        !self.url.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SavedCredentials {
    pub id: String,
    pub title: String,
}

impl SavedCredentials {
    pub fn is_valid(&self) -> bool {
        !self.id.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NewCredentials {
    pub title: String,
    pub data: Vec<u8>,
    pub save_on_server: bool,
}

impl NewCredentials {
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StripePaymentMethod {
    pub publishable_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SmartGlocalPaymentMethod {
    pub public_token: String,
    pub tokenize_url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NativePaymentMethod {
    None,
    Stripe(StripePaymentMethod),
    SmartGlocal(SmartGlocalPaymentMethod),
}

impl Default for NativePaymentMethod {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PaymentMethod {
    pub native: NativePaymentMethod,
    pub saved_credentials: Vec<SavedCredentials>,
    pub saved_credentials_index: usize,
    pub new_credentials: Option<NewCredentials>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InvoiceMessage {
    pub peer_id: i64,
    pub item_id: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InvoiceSlug {
    pub slug: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InvoiceCredits {
    pub random_id: u64,
    pub credits: u64,
    pub amount: u64,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InvoiceStarGift {
    pub gift_id: u64,
    pub random_id: u64,
    pub recipient_id: i64,
    pub limited_count: i32,
    pub per_user_limit: i32,
    pub anonymous: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InvoiceId {
    Message(InvoiceMessage),
    Slug(InvoiceSlug),
    Credits(InvoiceCredits),
    StarGift(InvoiceStarGift),
}

impl Default for InvoiceId {
    fn default() -> Self {
        Self::Slug(InvoiceSlug::default())
    }
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ThumbnailLoadProcess {
    pub is_loading: bool,
    pub has_blurred: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaymentsForm {
    pub details: FormDetails,
    pub payment_method: PaymentMethod,
    pub selected_shipping_id: Option<String>,
    pub tip_amount: i64,
    pub trust_bot: bool,
    pub has_password: bool,
    pub is_submitting: bool,
    pub updated_at: i64,
}

impl Default for PaymentsForm {
    fn default() -> Self {
        Self {
            details: FormDetails::default(),
            payment_method: PaymentMethod::default(),
            selected_shipping_id: None,
            tip_amount: 0,
            trust_bot: false,
            has_password: true,
            is_submitting: false,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl PaymentsForm {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn validate_information(&self) -> bool {
        self.details.is_valid()
    }

    pub fn validate_card(&self) -> bool {
        if let Some(ref cred) = self.payment_method.new_credentials {
            !cred.is_empty()
        } else if !self.payment_method.saved_credentials.is_empty() {
            let idx = self.payment_method.saved_credentials_index;
            idx < self.payment_method.saved_credentials.len() && self.payment_method.saved_credentials[idx].is_valid()
        } else {
            false
        }
    }

    pub fn set_tips(&mut self, tips: i64) {
        self.tip_amount = tips;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn set_shipping_option(&mut self, option_id: impl Into<String>) {
        self.selected_shipping_id = Some(option_id.into());
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn accept_terms(&mut self) {
        self.details.terms_accepted = true;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn trust_bot(&mut self, trust: bool) {
        self.trust_bot = trust;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn submit(&mut self) -> Result<(), &'static str> {
        if !self.details.is_valid() {
            return Err("Form details invalid");
        }
        self.is_submitting = true;
        self.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }
}
