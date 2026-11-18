//! Telegram Desktop 1:1 Parity Submodule: `ApiBot`
//! Subsystem: `api` | Feature ID: `TCG-N27`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_bot.cpp, Telegram/SourceFiles/api/api_bot.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use crate::message::InlineKeyboardButton;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BotButtonType {
    Callback(Vec<u8>),
    Url(String),
    WebApp(String),
    SwitchInline(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BotButton {
    pub text: String,
    pub button_type: BotButtonType,
}

impl BotButton {
    pub fn callback(text: impl Into<String>, data: impl Into<Vec<u8>>) -> Self {
        Self {
            text: text.into(),
            button_type: BotButtonType::Callback(data.into()),
        }
    }

    pub fn url(text: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            button_type: BotButtonType::Url(url.into()),
        }
    }

    pub fn web_app(text: impl Into<String>, web_app_url: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            button_type: BotButtonType::WebApp(web_app_url.into()),
        }
    }

    pub fn switch_inline(text: impl Into<String>, query: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            button_type: BotButtonType::SwitchInline(query.into()),
        }
    }

    pub fn to_inline_keyboard_button(&self) -> InlineKeyboardButton {
        match &self.button_type {
            BotButtonType::Callback(data) => InlineKeyboardButton {
                text: self.text.clone(),
                callback_data: Some(String::from_utf8_lossy(data).to_string()),
                url: None,
                web_app_url: None,
            },
            BotButtonType::Url(url) => InlineKeyboardButton {
                text: self.text.clone(),
                callback_data: None,
                url: Some(url.clone()),
                web_app_url: None,
            },
            BotButtonType::WebApp(web_app_url) => InlineKeyboardButton {
                text: self.text.clone(),
                callback_data: None,
                url: None,
                web_app_url: Some(web_app_url.clone()),
            },
            BotButtonType::SwitchInline(query) => InlineKeyboardButton {
                text: self.text.clone(),
                callback_data: Some(format!("switch_inline:{}", query)),
                url: None,
                web_app_url: None,
            },
        }
    }

    pub fn from_inline_keyboard_button(btn: &InlineKeyboardButton) -> Option<Self> {
        if let Some(ref web_app_url) = btn.web_app_url {
            return Some(Self::web_app(btn.text.clone(), web_app_url.clone()));
        }
        if let Some(ref url) = btn.url {
            return Some(Self::url(btn.text.clone(), url.clone()));
        }
        if let Some(ref data) = btn.callback_data {
            if let Some(query) = data.strip_prefix("switch_inline:") {
                return Some(Self::switch_inline(btn.text.clone(), query.to_string()));
            }
            return Some(Self::callback(btn.text.clone(), data.as_bytes().to_vec()));
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BotMenuButtonConfig {
    pub text: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiBot {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub clicked_callbacks: Vec<(u64, i64, Vec<u8>)>,
    #[serde(default)]
    pub bot_menu_buttons: BTreeMap<u64, BotMenuButtonConfig>,
    #[serde(default)]
    pub active_web_app_sessions: Vec<String>,
    #[serde(default)]
    pub attach_menu_bot_ids: Vec<u64>,
    pub updated_at: i64,
}

impl Default for ApiBot {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiBot".to_string(),
            clicked_callbacks: Vec::new(),
            bot_menu_buttons: BTreeMap::new(),
            active_web_app_sessions: Vec::new(),
            attach_menu_bot_ids: Vec::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiBot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_label(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.is_enabled = enabled;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn send_callback_data(&mut self, peer_id: u64, msg_id: i64, data: Vec<u8>) {
        self.clicked_callbacks.push((peer_id, msg_id, data));
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn set_bot_menu_button(
        &mut self,
        bot_user_id: u64,
        text: impl Into<String>,
        url: impl Into<String>,
    ) {
        self.bot_menu_buttons.insert(
            bot_user_id,
            BotMenuButtonConfig {
                text: text.into(),
                url: url.into(),
            },
        );
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn menu_button_for(&self, bot_user_id: u64) -> Option<&BotMenuButtonConfig> {
        self.bot_menu_buttons.get(&bot_user_id)
    }

    pub fn register_web_app_session(&mut self, query_id: impl Into<String>) {
        let qid = query_id.into();
        if !self.active_web_app_sessions.contains(&qid) {
            self.active_web_app_sessions.push(qid);
            self.updated_at = chrono::Utc::now().timestamp();
        }
    }

    pub fn close_web_app_session(&mut self, query_id: &str) -> bool {
        let before = self.active_web_app_sessions.len();
        self.active_web_app_sessions.retain(|q| q != query_id);
        let removed = self.active_web_app_sessions.len() < before;
        if removed {
            self.updated_at = chrono::Utc::now().timestamp();
        }
        removed
    }

    pub fn record_attach_menu_bot(&mut self, bot_user_id: u64) {
        if !self.attach_menu_bot_ids.contains(&bot_user_id) {
            self.attach_menu_bot_ids.push(bot_user_id);
            self.updated_at = chrono::Utc::now().timestamp();
        }
    }

    pub fn remove_attach_menu_bot(&mut self, bot_user_id: u64) -> bool {
        let before = self.attach_menu_bot_ids.len();
        self.attach_menu_bot_ids.retain(|&id| id != bot_user_id);
        let removed = self.attach_menu_bot_ids.len() < before;
        if removed {
            self.updated_at = chrono::Utc::now().timestamp();
        }
        removed
    }
}
