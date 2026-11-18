//! Telegram Desktop 1:1 Parity Module: `ClickHandlerTypes`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/click_handler_types.h, Telegram/SourceFiles/core/click_handler_types.cpp

use serde::{Deserialize, Serialize};

pub const PEER_LINK_PEER_ID_PROPERTY: u32 = 0x01;
pub const PHOTO_LINK_MEDIA_PROPERTY: u32 = 0x02;
pub const DOCUMENT_LINK_MEDIA_PROPERTY: u32 = 0x03;
pub const SEND_REACTION_EMOJI_PROPERTY: u32 = 0x04;
pub const REACTIONS_COUNT_EMOJI_PROPERTY: u32 = 0x05;
pub const DOCUMENT_FILENAME_TOOLTIP_PROPERTY: u32 = 0x06;
pub const PHONE_NUMBER_LINK_PROPERTY: u32 = 0x07;
pub const TODO_LIST_ITEM_ID_PROPERTY: u32 = 0x08;
pub const POLL_OPTION_PROPERTY: u32 = 0x09;
pub const FAST_SHARE_PROPERTY: u32 = 0x0a;

/// Context passed along during link or entity click dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClickHandlerContext {
    pub item_id: u64,
    pub peer_id: Option<u64>,
    pub may_show_confirmation: bool,
    pub skip_bot_auto_login: bool,
    pub keep_web_auth_tokens: bool,
    pub bot_start_auto_submit: bool,
    pub ignore_iv: bool,
    pub force_external_url_confirmation: bool,
    pub dark_mode: bool,
}

impl Default for ClickHandlerContext {
    fn default() -> Self {
        Self {
            item_id: 0,
            peer_id: None,
            may_show_confirmation: true,
            skip_bot_auto_login: false,
            keep_web_auth_tokens: false,
            bot_start_auto_submit: false,
            ignore_iv: false,
            force_external_url_confirmation: false,
            dark_mode: false,
        }
    }
}

/// Checks whether navigating to `url` requires an explicit security confirmation dialog.
pub fn url_requires_confirmation(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("tg://") || lower.starts_with("tonsite://") {
        return false;
    }

    let host = if let Some(idx) = lower.find("://") {
        let rest = &lower[idx + 3..];
        rest.split('/').next().unwrap_or("")
    } else {
        lower.split('/').next().unwrap_or("")
    };

    let domain = host.split(':').next().unwrap_or("");

    let trusted_domains = [
        "telegram.org",
        "t.me",
        "telesco.pe",
        "fragment.com",
        "ton.org",
    ];

    for trusted in trusted_domains {
        if domain == trusted || domain.ends_with(&format!(".{}", trusted)) {
            return false;
        }
    }

    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiddenUrlClickHandler {
    pub url: String,
    pub display_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MentionClickHandler {
    pub username: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MentionNameClickHandler {
    pub user_id: u64,
    pub access_hash: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashtagClickHandler {
    pub hashtag: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashtagClickHandler {
    pub cashtag: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotCommandClickHandler {
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonospaceClickHandler {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TonAddressClickHandler {
    pub address: String,
}

/// Container struct for ClickHandler registry and classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ClickHandlerTypes {
    pub custom_whitelist: Vec<String>,
}

impl ClickHandlerTypes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_trusted_url(&self, url: &str) -> bool {
        if !url_requires_confirmation(url) {
            return true;
        }
        self.custom_whitelist.iter().any(|w| url.contains(w))
    }

    pub fn add_trusted_domain(&mut self, domain: impl Into<String>) {
        self.custom_whitelist.push(domain.into());
    }
}
