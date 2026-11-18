//! Telegram Desktop 1:1 Parity Submodule: `ApiEditing`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_editing.cpp, Telegram/SourceFiles/api/api_editing.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use crate::api::api_common::{RemoteFileInfo, SendOptions, VideoCoverEdit};
use crate::message::MessageEntity;

pub const DEFAULT_EDIT_MESSAGES_ERRORS: &[&str] = &[
    "MESSAGE_ID_INVALID",
    "CHAT_ADMIN_REQUIRED",
    "MESSAGE_EDIT_TIME_EXPIRED",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditMessageRequest {
    pub peer_id: u64,
    pub msg_id: i64,
    pub new_text: String,
    pub entities: Vec<MessageEntity>,
    pub options: SendOptions,
    pub spoilered: bool,
    pub remote_file: Option<RemoteFileInfo>,
    pub video_cover: Option<VideoCoverEdit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiEditing {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub pending_edits: Vec<EditMessageRequest>,
    pub total_edited: u64,
    pub updated_at: i64,
}

impl Default for ApiEditing {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiEditing".to_string(),
            pending_edits: Vec::new(),
            total_edited: 0,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiEditing {
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

    pub fn edit_text(
        &mut self,
        peer_id: u64,
        msg_id: i64,
        text: impl Into<String>,
        options: SendOptions,
    ) {
        self.pending_edits.push(EditMessageRequest {
            peer_id,
            msg_id,
            new_text: text.into(),
            entities: Vec::new(),
            options,
            spoilered: false,
            remote_file: None,
            video_cover: None,
        });
        self.total_edited += 1;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn edit_with_media(
        &mut self,
        peer_id: u64,
        msg_id: i64,
        caption: impl Into<String>,
        file: RemoteFileInfo,
        options: SendOptions,
    ) {
        self.pending_edits.push(EditMessageRequest {
            peer_id,
            msg_id,
            new_text: caption.into(),
            entities: Vec::new(),
            options,
            spoilered: false,
            remote_file: Some(file),
            video_cover: None,
        });
        self.total_edited += 1;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn reschedule(&mut self, peer_id: u64, msg_id: i64, new_schedule_time: i64) {
        if let Some(req) = self.pending_edits.iter_mut().find(|r| r.peer_id == peer_id && r.msg_id == msg_id) {
            req.options.scheduled = new_schedule_time;
            self.updated_at = chrono::Utc::now().timestamp();
        }
    }
}
