//! Telegram Desktop 1:1 Parity Submodule: `ApiCommon`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_common.cpp, Telegram/SourceFiles/api/api_common.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

pub const K_SCHEDULED_UNTIL_ONLINE_TIMESTAMP: i64 = 0x7FFF_FFFE;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SendType {
    #[default]
    Normal,
    Scheduled,
    ScheduledToUser,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SendOptions {
    pub price: u64,
    pub send_as: Option<u64>,
    pub scheduled: i64,
    pub schedule_repeat_period: i64,
    pub shortcut_id: u64,
    pub effect_id: u64,
    pub stake_nano_ton: i64,
    pub stars_approved: i32,
    pub silent: bool,
    pub handle_support_switch: bool,
    pub invert_caption: bool,
    pub hide_via_bot: bool,
    pub media_spoiler: bool,
    pub welcome_template: bool,
    pub ttl_seconds: i64,
    pub suggest: Option<String>,
}

impl SendOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_scheduled(scheduled: i64) -> Self {
        Self {
            scheduled,
            ..Self::default()
        }
    }

    pub fn is_scheduled(&self) -> bool {
        self.scheduled != 0
    }

    pub fn is_scheduled_until_online(&self) -> bool {
        self.scheduled == K_SCHEDULED_UNTIL_ONLINE_TIMESTAMP
    }
}

pub fn default_send_when_online_options() -> SendOptions {
    SendOptions {
        scheduled: K_SCHEDULED_UNTIL_ONLINE_TIMESTAMP,
        ..SendOptions::default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendAction {
    pub peer_id: u64,
    pub thread_id: Option<i64>,
    pub reply_to_msg_id: Option<i64>,
    pub options: SendOptions,
    pub clear_draft: bool,
    pub generate_local: bool,
    pub replace_media_of: Option<i64>,
}

impl Default for SendAction {
    fn default() -> Self {
        Self {
            peer_id: 0,
            thread_id: None,
            reply_to_msg_id: None,
            options: SendOptions::default(),
            clear_draft: true,
            generate_local: true,
            replace_media_of: None,
        }
    }
}

impl SendAction {
    pub fn new(peer_id: u64, options: SendOptions) -> Self {
        Self {
            peer_id,
            options,
            ..Self::default()
        }
    }

    pub fn with_reply(peer_id: u64, reply_to_msg_id: i64) -> Self {
        Self {
            peer_id,
            reply_to_msg_id: Some(reply_to_msg_id),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageToSend {
    pub action: SendAction,
    pub text: String,
    pub entities: Vec<crate::message::MessageEntity>,
    pub web_page_url: Option<String>,
}

impl MessageToSend {
    pub fn new(action: SendAction, text: impl Into<String>) -> Self {
        Self {
            action,
            text: text.into(),
            entities: Vec::new(),
            web_page_url: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RemoteFileInfo {
    pub file_id: u64,
    pub access_hash: u64,
    pub size: u64,
    pub mime_type: String,
    pub thumb_bytes: Option<Vec<u8>>,
    pub attached_stickers: Vec<u64>,
    pub force_file: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct VideoCoverEdit {
    pub photo_id: Option<u64>,
    pub cleared: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RateLimiter {
    pub capacity: f64,
    pub tokens: f64,
    pub refill_rate_per_sec: f64,
    pub last_refill_timestamp: i64,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self {
            capacity: 30.0,
            tokens: 30.0,
            refill_rate_per_sec: 1.0,
            last_refill_timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

impl RateLimiter {
    pub fn new(capacity: f64, refill_rate_per_sec: f64) -> Self {
        Self {
            capacity,
            tokens: capacity,
            refill_rate_per_sec,
            last_refill_timestamp: chrono::Utc::now().timestamp(),
        }
    }

    pub fn try_acquire(&mut self) -> bool {
        let now = chrono::Utc::now().timestamp();
        let elapsed = (now - self.last_refill_timestamp).max(0) as f64;
        self.tokens = (self.tokens + elapsed * self.refill_rate_per_sec).min(self.capacity);
        self.last_refill_timestamp = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

static NEXT_REQ_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiCommon {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub rate_limiter: RateLimiter,
    pub default_options: SendOptions,
    pub updated_at: i64,
}

impl Default for ApiCommon {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiCommon".to_string(),
            rate_limiter: RateLimiter::default(),
            default_options: SendOptions::default(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiCommon {
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

    pub fn next_req_id() -> u64 {
        NEXT_REQ_ID.fetch_add(1, Ordering::Relaxed)
    }

    pub fn reply_to_for_mtp(&self, reply_to_msg_id: i64) -> Option<i64> {
        if reply_to_msg_id > 0 {
            Some(reply_to_msg_id)
        } else {
            None
        }
    }

    pub fn default_send_when_online_options(&self) -> SendOptions {
        default_send_when_online_options()
    }

    pub fn send_action(&self, peer_id: u64, reply_to: Option<i64>) -> SendAction {
        SendAction {
            peer_id,
            reply_to_msg_id: reply_to,
            options: self.default_options.clone(),
            ..SendAction::default()
        }
    }
}
