use serde::{Deserialize, Serialize};
use crate::message::MessageEntity;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CreateChatRequest {
    pub title: String,
    #[serde(default)]
    pub chat_type: Option<serde_json::Value>,
    #[serde(default)]
    pub peer_user_id: Option<u64>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub about: Option<String>,
    #[serde(default)]
    pub is_forum: bool,
    #[serde(default)]
    pub member_ids: Vec<u64>,
    #[serde(default)]
    pub member_usernames: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatDetailDto {
    pub chat_id: i64,
    pub title: String,
    pub kind: String,
    pub chat_type: u8,
    pub creator_id: u64,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub about: Option<String>,
    pub is_group: bool,
    pub is_channel: bool,
    pub is_bot: bool,
    pub is_forum: bool,
    pub is_archived: bool,
    pub is_pinned: bool,
    pub is_muted: bool,
    #[serde(default)]
    pub mute_until_ms: Option<i64>,
    pub pts: u64,
    pub last_message_id: i64,
    pub last_message_date_ms: i64,
    pub unread_count: u32,
    pub read_inbox_max_id: i64,
    pub read_outbox_max_id: i64,
    pub member_count: u32,
    #[serde(default)]
    pub pinned_message_id: Option<i64>,
    #[serde(default)]
    pub draft: Option<DraftDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberDto {
    pub user_id: u64,
    #[serde(default)]
    pub username: Option<String>,
    pub first_name: String,
    #[serde(default)]
    pub last_name: Option<String>,
    pub role: String,
    pub admin_rights: u32,
    pub banned_rights: u32,
    #[serde(default)]
    pub admin_title: Option<String>,
    #[serde(default)]
    pub until_date_ms: Option<i64>,
    pub joined_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InviteLinkDto {
    pub invite_id: String,
    pub chat_id: i64,
    pub creator_id: u64,
    pub link: String,
    pub code: String,
    #[serde(default)]
    pub title: Option<String>,
    pub usage_limit: Option<u32>,
    pub usage_count: u32,
    pub expire_date_ms: Option<i64>,
    pub request_needed: bool,
    pub is_revoked: bool,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForumTopicDto {
    pub chat_id: i64,
    pub topic_id: i64,
    pub title: String,
    pub icon_color: u32,
    #[serde(default)]
    pub icon_emoji_id: Option<i64>,
    pub creator_id: u64,
    pub is_pinned: bool,
    pub is_closed: bool,
    pub is_hidden: bool,
    #[serde(default)]
    pub last_message_id: Option<i64>,
    pub unread_count: u32,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ChatFolderDto {
    #[serde(default, alias = "folder_id")]
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub emoticon: Option<String>,
    #[serde(default, alias = "included_chat_ids")]
    pub include_chats: Vec<i64>,
    #[serde(default, alias = "excluded_chat_ids")]
    pub exclude_chats: Vec<i64>,
    #[serde(default, alias = "pinned_chat_ids")]
    pub pinned_chats: Vec<i64>,
    #[serde(default)]
    pub include_contacts: bool,
    #[serde(default)]
    pub include_non_contacts: bool,
    #[serde(default)]
    pub include_groups: bool,
    #[serde(default)]
    pub include_channels: bool,
    #[serde(default)]
    pub include_bots: bool,
    #[serde(default)]
    pub exclude_muted: bool,
    #[serde(default)]
    pub exclude_read: bool,
    #[serde(default)]
    pub exclude_archived: bool,
    #[serde(default)]
    pub sort_order: i32,
}

pub type FolderDto = ChatFolderDto;
pub type UpsertFolderRequest = ChatFolderDto;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DraftDto {
    pub chat_id: i64,
    pub text: String,
    #[serde(default)]
    pub entities: Vec<MessageEntity>,
    #[serde(default)]
    pub reply_to_msg_id: Option<i64>,
    #[serde(default)]
    pub topic_id: Option<i64>,
    pub date_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedMessageDto {
    pub chat_id: i64,
    pub message_id: i64,
    pub pinned_by: u64,
    pub pinned_at_ms: i64,
    #[serde(default)]
    pub preview_text: String,
}
