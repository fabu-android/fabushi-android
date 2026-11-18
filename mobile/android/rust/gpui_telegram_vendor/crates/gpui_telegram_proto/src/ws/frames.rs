use crate::message::{InlineKeyboardButton, MessageContent, MessageEntity};
use crate::sync::{PtsUpdate, SyncGetDifferenceRequest, SyncGetDifferenceResponse, TypingEvent, UpdateEvent};
use gpui_telegram_core::types::{ChatId, MessageId, TopicId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub chat_id: ChatId,
    pub client_nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<MessageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_id: Option<TopicId>,
    pub content: MessageContent,
    #[serde(default)]
    pub silent: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<MessageEntity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forward_from_chat_id: Option<ChatId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forward_from_msg_id: Option<MessageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_keyboard: Option<Vec<Vec<InlineKeyboardButton>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_id: Option<String>,
}

impl SendMessageRequest {
    pub fn new(chat_id: ChatId, client_nonce: u64, content: MessageContent) -> Self {
        Self {
            chat_id,
            client_nonce,
            reply_to: None,
            topic_id: None,
            content,
            silent: false,
            entities: Vec::new(),
            quote_text: None,
            forward_from_chat_id: None,
            forward_from_msg_id: None,
            scheduled_at_ms: None,
            inline_keyboard: None,
            media_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditMessageRequest {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub content: MessageContent,
}

impl EditMessageRequest {
    pub fn new(chat_id: ChatId, message_id: MessageId, content: MessageContent) -> Self {
        Self {
            chat_id,
            message_id,
            content,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteMessagesRequest {
    pub chat_id: ChatId,
    pub message_ids: Vec<MessageId>,
    pub for_everyone: bool,
}

impl DeleteMessagesRequest {
    pub fn new(chat_id: ChatId, message_ids: Vec<MessageId>, for_everyone: bool) -> Self {
        Self {
            chat_id,
            message_ids,
            for_everyone,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactionRequest {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub emoji: String,
    pub add: bool,
}

impl ReactionRequest {
    pub fn new(chat_id: ChatId, message_id: MessageId, emoji: impl Into<String>, add: bool) -> Self {
        Self {
            chat_id,
            message_id,
            emoji: emoji.into(),
            add,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadHistoryRequest {
    pub chat_id: ChatId,
    pub max_id: MessageId,
}

impl ReadHistoryRequest {
    pub fn new(chat_id: ChatId, max_id: MessageId) -> Self {
        Self { chat_id, max_id }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum WsClientFrame {
    Ping {
        nonce: u64,
    },
    Sync(SyncGetDifferenceRequest),
    SendMessage(SendMessageRequest),
    EditMessage(EditMessageRequest),
    DeleteMessages(DeleteMessagesRequest),
    SendReaction(ReactionRequest),
    ReadHistory(ReadHistoryRequest),
    SendTyping(TypingEvent),
    SubscribeChat {
        chat_id: i64,
    },
    UnsubscribeChat {
        chat_id: i64,
    },
    VotePoll {
        chat_id: ChatId,
        message_id: MessageId,
        poll_id: u64,
        option_ids: Vec<u8>,
    },
    CallSignal {
        call_id: String,
        signal_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_user_id: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sdp: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        candidate: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sdp_mid: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sdp_mline_index: Option<u32>,
    },
    AckUpdates {
        chat_id: ChatId,
        pts: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum WsServerFrame {
    Pong {
        nonce: u64,
    },
    Update(UpdateEvent),
    PtsPush(PtsUpdate),
    SyncDiff(SyncGetDifferenceResponse),
    Ack {
        #[serde(default)]
        correlation_id: u64,
        #[serde(default)]
        client_nonce: u64,
        #[serde(default)]
        message_id: i64,
        #[serde(default)]
        pts: u64,
    },
    Error {
        #[serde(default)]
        code: u16,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<String>,
    },
}
