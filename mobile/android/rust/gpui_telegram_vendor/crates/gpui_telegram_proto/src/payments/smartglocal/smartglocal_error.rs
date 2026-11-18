//! Telegram Desktop 1:1 Parity Submodule: `SmartglocalErrorSmartglocal`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Upstream Sources: `Telegram/SourceFiles/payments/smartglocal/smartglocal_error.cpp`, `Telegram/SourceFiles/payments/smartglocal/smartglocal_error.h`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Code {
    #[default]
    None = 0,
    JsonParse = -1,
    JsonFormat = -2,
    Network = -3,
    Unknown = 8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Error {
    pub code: Code,
    pub description: String,
    pub message: String,
    pub parameter: Option<String>,
}

impl Error {
    pub fn none() -> Self {
        Self {
            code: Code::None,
            description: String::new(),
            message: String::new(),
            parameter: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.code == Code::None
    }

    pub fn decoded_object_from_response(val: &serde_json::Value) -> Self {
        if val.get("status").and_then(|v| v.as_str()) == Some("ok") {
            return Self::none();
        }
        if let Some(err_obj) = val.get("error").and_then(|v| v.as_object()) {
            let code_str = err_obj.get("code").and_then(|v| v.as_str()).unwrap_or("");
            let desc_str = err_obj.get("description").and_then(|v| v.as_str()).unwrap_or("");
            if code_str.is_empty() || desc_str.is_empty() {
                Self {
                    code: Code::Unknown,
                    description: "GenericError".to_string(),
                    message: "Could not interpret the error response from SmartGlocal.".to_string(),
                    parameter: None,
                }
            } else {
                Self {
                    code: Code::Unknown,
                    description: code_str.to_string(),
                    message: desc_str.to_string(),
                    parameter: None,
                }
            }
        } else {
            Self {
                code: Code::Unknown,
                description: "GenericError".to_string(),
                message: "Could not read the error response from SmartGlocal.".to_string(),
                parameter: None,
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartglocalErrorSmartglocal {
    pub is_enabled: bool,
    pub error: Error,
    pub updated_at: i64,
}

impl Default for SmartglocalErrorSmartglocal {
    fn default() -> Self {
        Self {
            is_enabled: true,
            error: Error::none(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl SmartglocalErrorSmartglocal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_error(error: Error) -> Self {
        Self {
            error,
            ..Self::default()
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled && !self.error.is_empty()
    }

    pub fn code(&self) -> Code {
        self.error.code
    }

    pub fn description(&self) -> &str {
        &self.error.description
    }

    pub fn message(&self) -> &str {
        &self.error.message
    }
}
