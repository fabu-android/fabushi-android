use chrono::{DateTime, Utc};
use gpui_telegram_core::types::{MessageId, PeerId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteTombstone {
    pub chat_id: PeerId,
    pub message_ids: Vec<MessageId>,
    pub deleted_at: DateTime<Utc>,
    pub for_everyone: bool,
    #[serde(default)]
    pub timestamp_ms: i64,
}

impl DeleteTombstone {
    pub fn new(chat_id: PeerId, message_ids: Vec<MessageId>, for_everyone: bool) -> Self {
        let now = Utc::now();
        Self {
            chat_id,
            message_ids,
            deleted_at: now,
            for_everyone,
            timestamp_ms: now.timestamp_millis(),
        }
    }

    pub fn is_deleted(&self, message_id: MessageId) -> bool {
        self.message_ids.contains(&message_id)
    }
}
