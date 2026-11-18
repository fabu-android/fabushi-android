//! Telegram Passport Main Authorization Panel Form
//! Upstream: `Telegram/SourceFiles/passport/passport_panel_form.h`, `passport_panel_form.cpp`
//! Subsystem: `passport`, Feature ID: `TCG-N02`

use serde::{Deserialize, Serialize};
use super::passport_form_view_controller::ScopeRow;

/// Top-level view model for the bot authorization overview screen in Telegram Passport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassportPanelForm {
    pub bot_name: String,
    pub bot_username: String,
    pub bot_avatar: Option<Vec<u8>>,
    pub header_text: String,
    pub policy_url: String,
    pub rows: Vec<ScopeRow>,
    pub can_authorize: bool,
    pub is_authorizing: bool,
}

impl Default for PassportPanelForm {
    fn default() -> Self {
        Self {
            bot_name: String::new(),
            bot_username: String::new(),
            bot_avatar: None,
            header_text: "requests the following documents to verify your identity:".to_string(),
            policy_url: String::new(),
            rows: Vec::new(),
            can_authorize: false,
            is_authorizing: false,
        }
    }
}

impl PassportPanelForm {
    pub fn new(bot_name: impl Into<String>, bot_username: impl Into<String>) -> Self {
        let name = bot_name.into();
        let user = bot_username.into();
        let header = format!("**{name}** requests the following documents to verify your identity:");
        Self {
            bot_name: name,
            bot_username: user,
            bot_avatar: None,
            header_text: header,
            policy_url: String::new(),
            rows: Vec::new(),
            can_authorize: false,
            is_authorizing: false,
        }
    }

    pub fn set_rows(&mut self, rows: Vec<ScopeRow>) {
        self.can_authorize = !rows.is_empty() && rows.iter().all(|r| r.is_complete);
        self.rows = rows;
    }

    pub fn update_authorization_state(&mut self) {
        self.can_authorize = !self.rows.is_empty() && self.rows.iter().all(|r| r.is_complete);
    }

    pub fn begin_authorization(&mut self) -> Result<(), &'static str> {
        if !self.can_authorize {
            return Err("Cannot authorize: some required documents or fields are missing");
        }
        self.is_authorizing = true;
        Ok(())
    }
}
