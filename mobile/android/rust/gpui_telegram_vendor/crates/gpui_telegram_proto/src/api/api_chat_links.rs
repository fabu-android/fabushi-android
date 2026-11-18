//! Telegram Desktop 1:1 Parity Submodule: `ApiChatLinks`
//! Subsystem: `api` | Feature ID: `TCG-N11`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_chat_links.cpp, Telegram/SourceFiles/api/api_chat_links.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChatLink {
    pub link: String,
    pub title: String,
    pub message: String,
    pub clicks: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatLinkUpdate {
    pub was: String,
    pub now: Option<ChatLink>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiChatLinks {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub links: Vec<ChatLink>,
    pub is_loaded: bool,
    pub updated_at: i64,
}

impl Default for ApiChatLinks {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiChatLinks".to_string(),
            links: Vec::new(),
            is_loaded: true,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiChatLinks {
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

    pub fn create(&mut self, title: impl Into<String>, message: impl Into<String>) -> ChatLink {
        let title = title.into();
        let message = message.into();
        let random_slug = uuid::Uuid::new_v4().as_fields().0;
        let link = format!("https://t.me/+link_{:x}", random_slug);
        let chat_link = ChatLink {
            link,
            title,
            message,
            clicks: 0,
        };
        self.links.push(chat_link.clone());
        self.updated_at = chrono::Utc::now().timestamp();
        chat_link
    }

    pub fn edit(&mut self, link_url: &str, title: impl Into<String>, message: impl Into<String>) -> Option<ChatLink> {
        if let Some(link) = self.links.iter_mut().find(|l| l.link == link_url) {
            link.title = title.into();
            link.message = message.into();
            self.updated_at = chrono::Utc::now().timestamp();
            Some(link.clone())
        } else {
            None
        }
    }

    pub fn destroy(&mut self, link_url: &str) -> bool {
        let initial_len = self.links.len();
        self.links.retain(|l| l.link != link_url);
        if self.links.len() < initial_len {
            self.updated_at = chrono::Utc::now().timestamp();
            true
        } else {
            false
        }
    }
}
