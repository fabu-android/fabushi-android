use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    Draft,
    Pending,
    RequiresAction,
    Authorized,
    Paid,
    Refunded,
    Cancelled,
    Failed,
}

impl Default for PaymentStatus {
    fn default() -> Self {
        Self::Draft
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceLine {
    pub label: String,
    pub amount_cents: u64,
}

impl PriceLine {
    pub fn new(label: impl Into<String>, amount_cents: u64) -> Self {
        Self {
            label: label.into(),
            amount_cents,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceCard {
    pub id: String,
    #[serde(default)]
    pub invoice_id: String,
    pub title: String,
    pub description: String,
    pub currency: String,
    pub amount_cents: u64,
    #[serde(default)]
    pub total_amount: u64,
    pub status: PaymentStatus,
    #[serde(default)]
    pub is_paid: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub prices: Vec<PriceLine>,
}

impl InvoiceCard {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        currency: impl Into<String>,
        amount_cents: u64,
    ) -> Self {
        let id_str = id.into();
        Self {
            id: id_str.clone(),
            invoice_id: id_str,
            title: title.into(),
            description: description.into(),
            currency: currency.into(),
            amount_cents,
            total_amount: amount_cents,
            status: PaymentStatus::Pending,
            is_paid: false,
            payload: None,
            provider_id: None,
            prices: Vec::new(),
        }
    }

    pub fn with_prices(mut self, prices: Vec<PriceLine>) -> Self {
        let sum: u64 = prices.iter().map(|p| p.amount_cents).sum();
        if sum > 0 {
            self.amount_cents = sum;
            self.total_amount = sum;
        }
        self.prices = prices;
        self
    }

    pub fn total_from_prices(&self) -> u64 {
        if self.prices.is_empty() {
            self.amount_cents
        } else {
            self.prices.iter().map(|p| p.amount_cents).sum()
        }
    }

    pub fn mark_paid(&mut self) {
        self.status = PaymentStatus::Paid;
        self.is_paid = true;
    }

    pub fn mark_failed(&mut self) {
        self.status = PaymentStatus::Failed;
        self.is_paid = false;
    }

    pub fn mark_cancelled(&mut self) {
        self.status = PaymentStatus::Cancelled;
        self.is_paid = false;
    }
}
