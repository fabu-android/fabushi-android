//! Telegram Passport Panel Edit Contact (Phone & Email)
//! Upstream: `Telegram/SourceFiles/passport/passport_panel_edit_contact.h`, `passport_panel_edit_contact.cpp`
//! Subsystem: `passport`, Feature ID: `TCG-N02`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditContactValueType {
    Phone,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditContactScheme {
    pub value_type: EditContactValueType,
    pub about_existing: String,
    pub new_header: String,
    pub placeholder: String,
    pub about_new: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyContactState {
    pub destination: String,
    pub code_length: usize,
    pub pending_code: String,
    pub resend_timeout_sec: u32,
    pub error_message: Option<String>,
}

/// View model and state controller for editing phone numbers and email addresses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassportPanelEditContact {
    pub scheme: EditContactScheme,
    pub current_value: String,
    pub existing_value: Option<String>,
    pub verification: Option<VerifyContactState>,
    pub is_saved: bool,
}

impl PassportPanelEditContact {
    pub fn new_phone(current: impl Into<String>, existing: Option<String>) -> Self {
        Self {
            scheme: EditContactScheme {
                value_type: EditContactValueType::Phone,
                about_existing: "Existing phone number from Telegram account".to_string(),
                new_header: "Enter new phone number".to_string(),
                placeholder: "+1 234 567-8900".to_string(),
                about_new: "A verification code will be sent to confirm this number.".to_string(),
            },
            current_value: current.into(),
            existing_value: existing,
            verification: None,
            is_saved: false,
        }
    }

    pub fn new_email(current: impl Into<String>, existing: Option<String>) -> Self {
        Self {
            scheme: EditContactScheme {
                value_type: EditContactValueType::Text,
                about_existing: "Existing email address from Telegram account".to_string(),
                new_header: "Enter new email address".to_string(),
                placeholder: "user@example.com".to_string(),
                about_new: "A confirmation link or code will be sent to this email.".to_string(),
            },
            current_value: current.into(),
            existing_value: existing,
            verification: None,
            is_saved: false,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match self.scheme.value_type {
            EditContactValueType::Phone => {
                let digits: String = self.current_value.chars().filter(|c| c.is_ascii_digit()).collect();
                if digits.len() < 7 || digits.len() > 15 {
                    return Err("Phone number must contain between 7 and 15 digits".to_string());
                }
            }
            EditContactValueType::Text => {
                if !self.current_value.contains('@') || !self.current_value.contains('.') {
                    return Err("Invalid email address format".to_string());
                }
            }
        }
        Ok(())
    }

    pub fn format_phone_digits(raw: &str) -> String {
        let mut digits: Vec<char> = raw.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            return String::new();
        }
        let mut formatted = String::from("+");
        if digits.len() <= 3 {
            formatted.extend(digits);
        } else {
            let country_len = if digits.first() == Some(&'1') {
                1
            } else if digits.len() > 10 {
                2
            } else {
                1
            };
            let country: String = digits.drain(..country_len).collect();
            formatted.push_str(&country);
            formatted.push(' ');

            let area_len = 3.min(digits.len());
            let area: String = digits.drain(..area_len).collect();
            formatted.push_str(&format!("({area}) "));

            let mid_len = 3.min(digits.len());
            let mid: String = digits.drain(..mid_len).collect();
            formatted.push_str(&mid);

            if !digits.is_empty() {
                formatted.push('-');
                let rest: String = digits.into_iter().collect();
                formatted.push_str(&rest);
            }
        }
        formatted
    }

    pub fn start_verification(&mut self, code_length: usize, timeout: u32) {
        self.verification = Some(VerifyContactState {
            destination: self.current_value.clone(),
            code_length,
            pending_code: String::new(),
            resend_timeout_sec: timeout,
            error_message: None,
        });
    }

    pub fn submit_code(&mut self, code: &str) -> Result<(), String> {
        if let Some(v) = &mut self.verification {
            if code.len() != v.code_length {
                let err = format!("Code must be exactly {} digits", v.code_length);
                v.error_message = Some(err.clone());
                return Err(err);
            }
            v.pending_code = code.to_string();
            self.is_saved = true;
            Ok(())
        } else {
            Err("No verification session active".to_string())
        }
    }
}
