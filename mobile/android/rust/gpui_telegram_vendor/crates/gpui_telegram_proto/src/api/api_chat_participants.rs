//! Telegram Desktop 1:1 Parity Submodule: `ApiChatParticipants`
//! Subsystem: `api` | Feature ID: `TCG-N11`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_chat_participants.cpp, Telegram/SourceFiles/api/api_chat_participants.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ParticipantType {
    Creator,
    Admin,
    #[default]
    Member,
    Restricted,
    Left,
    Banned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChatAdminRights {
    pub change_info: bool,
    pub post_messages: bool,
    pub edit_messages: bool,
    pub delete_messages: bool,
    pub ban_users: bool,
    pub invite_users: bool,
    pub pin_messages: bool,
    pub add_admins: bool,
    pub anonymous: bool,
    pub manage_call: bool,
    pub manage_topics: bool,
    pub post_stories: bool,
    pub edit_stories: bool,
    pub delete_stories: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChatBannedRights {
    pub view_messages: bool,
    pub send_messages: bool,
    pub send_media: bool,
    pub send_stickers: bool,
    pub send_gifs: bool,
    pub embed_links: bool,
    pub send_polls: bool,
    pub change_info: bool,
    pub invite_users: bool,
    pub pin_messages: bool,
    pub manage_topics: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatParticipant {
    pub peer_id: u64,
    pub invited_by: u64,
    pub participant_type: ParticipantType,
    pub admin_rights: Option<ChatAdminRights>,
    pub banned_rights: Option<ChatBannedRights>,
    pub rank: Option<String>,
    pub can_be_edited: bool,
    pub member_since: i64,
}

impl Default for ChatParticipant {
    fn default() -> Self {
        Self {
            peer_id: 0,
            invited_by: 0,
            participant_type: ParticipantType::Member,
            admin_rights: None,
            banned_rights: None,
            rank: None,
            can_be_edited: false,
            member_since: chrono::Utc::now().timestamp(),
        }
    }
}

impl ChatParticipant {
    pub fn is_creator_or_admin(&self) -> bool {
        matches!(self.participant_type, ParticipantType::Creator | ParticipantType::Admin)
    }

    pub fn is_banned(&self) -> bool {
        matches!(self.participant_type, ParticipantType::Banned)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiChatParticipants {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub channel_participants: HashMap<u64, Vec<ChatParticipant>>,
    pub updated_at: i64,
}

impl Default for ApiChatParticipants {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiChatParticipants".to_string(),
            channel_participants: HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiChatParticipants {
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

    pub fn set_participants(&mut self, channel_id: u64, participants: Vec<ChatParticipant>) {
        self.channel_participants.insert(channel_id, participants);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn promote_admin(&mut self, channel_id: u64, peer_id: u64, rights: ChatAdminRights, rank: Option<String>) {
        let list = self.channel_participants.entry(channel_id).or_default();
        if let Some(p) = list.iter_mut().find(|p| p.peer_id == peer_id) {
            p.participant_type = ParticipantType::Admin;
            p.admin_rights = Some(rights);
            p.rank = rank;
        } else {
            list.push(ChatParticipant {
                peer_id,
                invited_by: 0,
                participant_type: ParticipantType::Admin,
                admin_rights: Some(rights),
                banned_rights: None,
                rank,
                can_be_edited: true,
                member_since: chrono::Utc::now().timestamp(),
            });
        }
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn kick_participant(&mut self, channel_id: u64, peer_id: u64) {
        if let Some(list) = self.channel_participants.get_mut(&channel_id) {
            list.retain(|p| p.peer_id != peer_id);
            self.updated_at = chrono::Utc::now().timestamp();
        }
    }
}
