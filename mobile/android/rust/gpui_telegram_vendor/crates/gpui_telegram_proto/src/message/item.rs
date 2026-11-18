use crate::message::content::MessageContent;
use crate::message::InlineKeyboardButton;
use chrono::{DateTime, Utc};
use gpui_telegram_core::types::{ChatId, MessageFlags, MessageId, PeerId, TopicId, UserId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplyHeader {
    pub reply_to_msg_id: MessageId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to_peer_id: Option<PeerId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote_text: Option<String>,
}

impl ReplyHeader {
    pub fn new(reply_to_msg_id: MessageId) -> Self {
        Self {
            reply_to_msg_id,
            reply_to_peer_id: None,
            quote_text: None,
        }
    }

    pub fn with_quote(mut self, quote: impl Into<String>) -> Self {
        self.quote_text = Some(quote.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForwardInfo {
    pub from_id: PeerId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_name: Option<String>,
    pub date: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_message_id: Option<MessageId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactionCount {
    pub emoji: String,
    #[serde(default)]
    pub count: u32,
    #[serde(default, alias = "chosen", alias = "user_reacted")]
    pub is_chosen_by_me: bool,
}

impl ReactionCount {
    pub fn new(emoji: impl Into<String>, count: u32, is_chosen_by_me: bool) -> Self {
        Self {
            emoji: emoji.into(),
            count,
            is_chosen_by_me,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageItem {
    pub id: MessageId,
    pub chat_id: ChatId,
    pub peer_id: PeerId,
    pub to_peer: PeerId,
    pub sender_id: UserId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_id: Option<TopicId>,
    pub flags: MessageFlags,
    pub date: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit_date: Option<DateTime<Utc>>,
    pub content: MessageContent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<ReplyHeader>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forward_info: Option<ForwardInfo>,
    #[serde(default)]
    pub reactions: Vec<ReactionCount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_nonce: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_keyboard: Option<Vec<Vec<InlineKeyboardButton>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<i64>,
}

impl MessageItem {
    pub fn new(
        id: MessageId,
        chat_id: ChatId,
        sender_id: UserId,
        to_peer: PeerId,
        content: MessageContent,
    ) -> Self {
        Self {
            id,
            chat_id,
            peer_id: to_peer,
            to_peer,
            sender_id,
            topic_id: None,
            flags: MessageFlags::default(),
            date: Utc::now(),
            edit_date: None,
            content,
            reply_to: None,
            forward_info: None,
            reactions: Vec::new(),
            client_nonce: None,
            inline_keyboard: None,
            ttl_seconds: None,
            expires_at_ms: None,
        }
    }

    pub fn with_reply_to(mut self, reply_header: ReplyHeader) -> Self {
        self.reply_to = Some(reply_header);
        self
    }

    pub fn with_flags(mut self, flags: MessageFlags) -> Self {
        self.flags = flags;
        self
    }

    pub fn with_client_nonce(mut self, nonce: u64) -> Self {
        self.client_nonce = Some(nonce);
        self
    }

    pub fn add_reaction(&mut self, emoji: impl Into<String>, by_me: bool) {
        let emoji_str = emoji.into();
        if let Some(r) = self.reactions.iter_mut().find(|r| r.emoji == emoji_str) {
            r.count += 1;
            if by_me {
                r.is_chosen_by_me = true;
            }
        } else {
            self.reactions.push(ReactionCount {
                emoji: emoji_str,
                count: 1,
                is_chosen_by_me: by_me,
            });
        }
    }
}
