//! Telegram Passport Cloud 2FA Password Screen (Ask / Setup)
//! Upstream: `Telegram/SourceFiles/passport/passport_panel_password.h`, `passport_panel_password.cpp`
//! Subsystem: `passport`, Feature ID: `TCG-N02`

use serde::{Deserialize, Serialize};

/// Controller and view model for 2FA cloud password authentication and recovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassportPanelPassword {
    pub has_password: bool,
    pub password_input: String,
    pub password_hint: Option<String>,
    pub error_banner: Option<String>,
    pub is_submitting: bool,
    pub recovery_requested: bool,
}

impl Default for PassportPanelPassword {
    fn default() -> Self {
        Self {
            has_password: true,
            password_input: String::new(),
            password_hint: None,
            error_banner: None,
            is_submitting: false,
            recovery_requested: false,
        }
    }
}

impl PassportPanelPassword {
    pub fn new(has_password: bool) -> Self {
        Self {
            has_password,
            password_input: String::new(),
            password_hint: None,
            error_banner: None,
            is_submitting: false,
            recovery_requested: false,
        }
    }

    pub fn set_password_input(&mut self, text: impl Into<String>) {
        self.password_input = text.into();
        self.error_banner = None;
    }

    pub fn show_error(&mut self, error: impl Into<String>) {
        self.error_banner = Some(error.into());
        self.is_submitting = false;
    }

    pub fn hide_error(&mut self) {
        self.error_banner = None;
    }

    pub fn submit(&mut self) -> Result<String, String> {
        if self.password_input.is_empty() {
            let err = "Password cannot be empty".to_string();
            self.show_error(&err);
            return Err(err);
        }
        self.is_submitting = true;
        self.error_banner = None;
        Ok(self.password_input.clone())
    }

    pub fn recover(&mut self) {
        self.recovery_requested = true;
        self.error_banner = None;
    }
}
