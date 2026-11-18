//! Telegram Desktop 1:1 Parity Submodule: `StripeErrorStripe`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Disposition: `inherited_rust_reused`
//! Upstream Sources: Telegram/SourceFiles/payments/stripe/stripe_error.cpp, Telegram/SourceFiles/payments/stripe/stripe_error.h
//! Inherited Paths: inherited/mahayana-messaging/src/payment.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(i32)]
pub enum Code {
    #[default]
    None = 0,
    JsonParse = -1,
    JsonFormat = -2,
    Network = -3,
    Unknown = 8,
    Connection = 40,
    InvalidRequest = 50,
    API = 60,
    Card = 70,
    Cancellation = 80,
    CheckoutUnknown = 5000,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Error {
    pub code: Code,
    pub description: String,
    pub message: String,
    pub parameter: String,
}

impl Error {
    pub fn none() -> Self {
        Self {
            code: Code::None,
            description: String::new(),
            message: String::new(),
            parameter: String::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.code == Code::None
    }

    pub fn decoded_object_from_response(object: &serde_json::Value) -> Self {
        let Some(error_obj) = object.get("error").and_then(|v| v.as_object()) else {
            return Self::none();
        };

        let type_str = error_obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let message_str = error_obj.get("message").and_then(|v| v.as_str()).unwrap_or("");
        let param_str = error_obj.get("param").and_then(|v| v.as_str()).unwrap_or("");

        if message_str.is_empty() || type_str.is_empty() {
            return Self {
                code: Code::API,
                description: "GenericError".into(),
                message: "Could not interpret the error response that was returned from Stripe.".into(),
                parameter: String::new(),
            };
        }

        // Convert snake_case parameter to camelCase
        let mut words = param_str.split('_').filter(|w| !w.is_empty());
        let parameter = if let Some(first) = words.next() {
            let mut res = first.to_lowercase();
            for word in words {
                let mut chars = word.chars();
                if let Some(c) = chars.next() {
                    res.push(c.to_ascii_uppercase());
                    res.push_str(&chars.as_str().to_lowercase());
                }
            }
            res
        } else {
            String::new()
        };

        if type_str == "api_error" {
            return Self {
                code: Code::API,
                description: "GenericError".into(),
                message: message_str.into(),
                parameter,
            };
        } else if type_str == "invalid_request_error" {
            return Self {
                code: Code::InvalidRequest,
                description: "GenericError".into(),
                message: message_str.into(),
                parameter,
            };
        } else if type_str != "card_error" {
            return Self {
                code: Code::Unknown,
                description: type_str.into(),
                message: message_str.into(),
                parameter,
            };
        }

        let code_str = error_obj.get("code").and_then(|v| v.as_str()).unwrap_or("");
        let description = match code_str {
            "incorrect_number" => "IncorrectNumber",
            "invalid_number" => "InvalidNumber",
            "invalid_expiry_month" => "InvalidExpiryMonth",
            "invalid_expiry_year" => "InvalidExpiryYear",
            "invalid_cvc" => "InvalidCVC",
            "expired_card" => "ExpiredCard",
            "incorrect_cvc" => "IncorrectCVC",
            "card_declined" => "CardDeclined",
            "processing_error" => "ProcessingError",
            other => other,
        };

        Self {
            code: Code::Card,
            description: description.into(),
            message: message_str.into(),
            parameter,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StripeErrorStripe {
    pub is_enabled: bool,
    pub last_error: Option<Error>,
}

impl StripeErrorStripe {
    pub fn new() -> Self {
        Self {
            is_enabled: true,
            last_error: None,
        }
    }

    pub fn decode(&mut self, object: &serde_json::Value) -> Error {
        let err = Error::decoded_object_from_response(object);
        self.last_error = Some(err.clone());
        err
    }

    pub fn none(&self) -> Error {
        Error::none()
    }
}
