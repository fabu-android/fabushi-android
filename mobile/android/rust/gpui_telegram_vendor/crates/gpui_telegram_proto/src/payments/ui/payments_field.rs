//! Telegram Desktop 1:1 Parity Submodule: `PaymentsFieldUi`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Disposition: `inherited_rust_reused`
//! Upstream Sources: Telegram/SourceFiles/payments/ui/payments_field.cpp, Telegram/SourceFiles/payments/ui/payments_field.h
//! Inherited Paths: inherited/mahayana-messaging/src/payment.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldType {
    CardNumber,
    CardExpireDate,
    CardCvc,
    CardHolder,
    AddressStreet1,
    AddressStreet2,
    AddressCity,
    AddressState,
    AddressCountry,
    AddressPostcode,
    Name,
    Email,
    Phone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FieldValidateRequest {
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FieldValidateResult {
    pub is_valid: bool,
    pub error_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldConfig {
    pub field_type: FieldType,
    pub placeholder: String,
    pub max_length: usize,
    pub is_required: bool,
}

impl Default for FieldConfig {
    fn default() -> Self {
        Self {
            field_type: FieldType::Name,
            placeholder: String::new(),
            max_length: 64,
            is_required: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub config: FieldConfig,
    pub value: String,
    pub cursor_position: usize,
    pub error: Option<String>,
}

impl Field {
    pub fn new(config: FieldConfig) -> Self {
        Self {
            config,
            value: String::new(),
            cursor_position: 0,
            error: None,
        }
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        let val = text.into();
        if val.len() <= self.config.max_length {
            self.value = val;
            self.cursor_position = self.value.len();
            self.error = None;
        }
    }

    pub fn validate(&mut self) -> bool {
        if self.config.is_required && self.value.trim().is_empty() {
            self.error = Some("Field is required".into());
            return false;
        }
        self.error = None;
        true
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PaymentsFieldUi {
    pub is_enabled: bool,
    pub fields: Vec<Field>,
}

impl PaymentsFieldUi {
    pub fn new() -> Self {
        Self {
            is_enabled: true,
            fields: Vec::new(),
        }
    }

    pub fn add_field(&mut self, config: FieldConfig) -> usize {
        let idx = self.fields.len();
        self.fields.push(Field::new(config));
        idx
    }

    pub fn validate_all(&mut self) -> bool {
        let mut all_valid = true;
        for f in &mut self.fields {
            if !f.validate() {
                all_valid = false;
            }
        }
        all_valid
    }
}
