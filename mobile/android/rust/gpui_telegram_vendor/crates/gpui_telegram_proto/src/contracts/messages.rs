use serde::{Deserialize, Serialize};
use crate::cards::PollCard;
pub use crate::message::InlineKeyboardButton;
use crate::message::{MessageContent, MessageEntity, ReactionCount};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendMessageHttpRequest {
    pub chat_id: i64,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub content: Option<MessageContent>,
    #[serde(default)]
    pub entities: Vec<MessageEntity>,
    #[serde(default, alias = "reply_to_id")]
    pub reply_to: Option<i64>,
    #[serde(default)]
    pub quote_text: Option<String>,
    #[serde(default)]
    pub topic_id: Option<i64>,
    #[serde(default)]
    pub client_nonce: Option<u64>,
    #[serde(default)]
    pub forward_from_chat_id: Option<i64>,
    #[serde(default)]
    pub forward_from_msg_id: Option<i64>,
    #[serde(default)]
    pub silent: bool,
    #[serde(default)]
    pub scheduled_at_ms: Option<i64>,
    #[serde(default)]
    pub inline_keyboard: Option<Vec<Vec<InlineKeyboardButton>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_as_peer_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendMessageHttpResponse {
    pub message_id: i64,
    pub id: i64,
    pub pts: u64,
    pub pts_count: u32,
    pub duplicate: bool,
    pub date_ms: i64,
    pub date: String,
    pub sender_id: u64,
    pub chat_id: i64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_nonce: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<i64>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditMessageHttpRequest {
    #[serde(default)]
    pub chat_id: Option<i64>,
    #[serde(default)]
    pub message_id: Option<i64>,
    pub text: String,
    #[serde(default)]
    pub entities: Vec<MessageEntity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactionHttpRequest {
    pub emoji: String,
    #[serde(default)]
    pub action: Option<String>, // "toggle" | "add" | "remove"
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactionDetailDto {
    pub emoji: String,
    pub count: u32,
    pub chosen: bool,
    #[serde(default)]
    pub user_ids: Vec<u64>,
}

#[derive(Debug, Deserialize)]
struct ReactionHttpResponseWire {
    #[serde(default = "default_true")]
    success: bool,
    #[serde(default)]
    chat_id: i64,
    #[serde(default)]
    message_id: i64,
    #[serde(default)]
    pts: u64,
    #[serde(default)]
    reactions: Option<Vec<ReactionCount>>,
    #[serde(default)]
    details: Option<Vec<ReactionDetailDto>>,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "ReactionHttpResponseWire")]
pub struct ReactionHttpResponse {
    pub success: bool,
    pub chat_id: i64,
    pub message_id: i64,
    pub pts: u64,
    pub reactions: Vec<ReactionCount>,
    #[serde(default)]
    pub details: Vec<ReactionDetailDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip)]
    pub reactions_authoritative: bool,
}

impl ReactionHttpResponse {
    pub fn has_authoritative_reactions(&self) -> bool {
        self.reactions_authoritative
    }
}

impl From<ReactionHttpResponseWire> for ReactionHttpResponse {
    fn from(wire: ReactionHttpResponseWire) -> Self {
        let (reactions, details, reactions_authoritative) = match (wire.reactions, wire.details) {
            (Some(rx), det) => (rx, det.unwrap_or_default(), true),
            (None, Some(det)) => {
                let derived = det
                    .iter()
                    .map(|d| ReactionCount::new(d.emoji.clone(), d.count, d.chosen))
                    .collect();
                (derived, det, true)
            }
            (None, None) => (Vec::new(), Vec::new(), false),
        };
        Self {
            success: wire.success,
            chat_id: wire.chat_id,
            message_id: wire.message_id,
            pts: wire.pts,
            reactions,
            details,
            status: wire.status,
            reactions_authoritative,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePollHttpRequest {
    pub question: String,
    pub options: Vec<String>,
    #[serde(default = "default_true")]
    pub is_anonymous: bool,
    #[serde(default)]
    pub multiple_choice: bool,
    #[serde(default)]
    pub is_quiz: bool,
    #[serde(default)]
    pub correct_option_id: Option<u8>,
    #[serde(default)]
    pub solution: Option<String>,
    #[serde(default)]
    pub topic_id: Option<i64>,
    #[serde(default)]
    pub client_nonce: Option<u64>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VotePollHttpRequest {
    pub option_ids: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PollHttpResponse {
    pub success: bool,
    pub pts: u64,
    pub poll: PollCard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadChatHttpRequest {
    pub max_id: i64,
    #[serde(default)]
    pub topic_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadChatHttpResponse {
    pub success: bool,
    pub chat_id: i64,
    pub pts: u64,
    pub read_inbox_max_id: i64,
    pub read_outbox_max_id: i64,
    pub unread_count: u32,
}
