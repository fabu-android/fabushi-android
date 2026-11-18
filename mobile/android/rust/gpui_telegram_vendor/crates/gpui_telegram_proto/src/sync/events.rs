use crate::cards::{PollCard, TodoCard};
use crate::contracts::{CallSignalRequest, DraftDto, FolderDto, ForumTopicDto, StoryDto};
use crate::message::{DeleteTombstone, MessageItem, ReactionCount};
use gpui_telegram_core::types::{ChatId, MessageId, UserId, UserStatus};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypingAction {
    Typing,
    RecordAudio,
    UploadAudio,
    RecordVideo,
    UploadVideo,
    RecordRound,
    UploadRound,
    UploadPhoto,
    UploadDocument,
    ChooseSticker,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypingEvent {
    pub chat_id: ChatId,
    pub user_id: UserId,
    pub action: TypingAction,
    pub timestamp_ms: i64,
}

impl TypingEvent {
    pub fn new(chat_id: ChatId, user_id: UserId, action: TypingAction) -> Self {
        Self {
            chat_id,
            user_id,
            action,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadHistoryEvent {
    pub chat_id: ChatId,
    pub max_id: MessageId,
    pub is_outbox: bool,
    pub unread_count: u32,
}

impl ReadHistoryEvent {
    pub fn new(chat_id: ChatId, max_id: MessageId, is_outbox: bool, unread_count: u32) -> Self {
        Self {
            chat_id,
            max_id,
            is_outbox,
            unread_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactionEvent {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub user_id: UserId,
    pub reactions: Vec<ReactionCount>,
}

impl ReactionEvent {
    pub fn new(
        chat_id: ChatId,
        message_id: MessageId,
        user_id: UserId,
        reactions: Vec<ReactionCount>,
    ) -> Self {
        Self {
            chat_id,
            message_id,
            user_id,
            reactions,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", content = "payload")]
pub enum ChatActionEvent {
    TitleChanged { new_title: String },
    PhotoChanged { photo_id: Option<String> },
    UserJoined { user_id: UserId },
    UserLeft { user_id: UserId },
    PinnedMessageChanged { message_id: Option<MessageId> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum UpdateEvent {
    NewMessage(MessageItem),
    EditMessage(MessageItem),
    DeleteMessages(DeleteTombstone),
    UserTyping(TypingEvent),
    ChatAction(ChatActionEvent),
    ReadHistory(ReadHistoryEvent),
    MessageReactions(ReactionEvent),
    ReactionUpdated(ReactionEvent),
    UserStatusChanged { user_id: UserId, status: UserStatus },
    PinnedMessageChanged { message_id: Option<MessageId> },
    ReadHistoryOut { max_id: MessageId },
    ReadHistoryIn { max_id: MessageId },
    PollUpdated {
        chat_id: ChatId,
        message_id: MessageId,
        poll: PollCard,
    },
    TodoUpdated {
        chat_id: ChatId,
        message_id: MessageId,
        todo: TodoCard,
    },
    PinnedMessagesChanged {
        chat_id: ChatId,
        pinned_ids: Vec<MessageId>,
    },
    DraftUpdated {
        chat_id: ChatId,
        topic_id: Option<i64>,
        draft: Option<DraftDto>,
    },
    ForumTopicChanged {
        chat_id: ChatId,
        topic: ForumTopicDto,
    },
    FolderChanged {
        folder: FolderDto,
    },
    CallUpdated {
        call_id: String,
        state: String,
        signal: Option<CallSignalRequest>,
    },
    StoryUpdated {
        story: StoryDto,
    },
    AgentFloorChanged {
        chat_id: ChatId,
        current_speaker: Option<String>,
        queued_count: usize,
        loop_hops: u32,
    },
    AgentWorkspaceUpdated {
        chat_id: ChatId,
        revision: u64,
        key: String,
        value: serde_json::Value,
        updated_by: String,
    },
    AgentHandoffEvent {
        chat_id: ChatId,
        from_agent: String,
        to_agent: String,
        hop: u32,
        max_hops: u32,
    },
}
