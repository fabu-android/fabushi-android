use crate::message::MessageItem;
use crate::sync::events::UpdateEvent;
use gpui_telegram_core::types::{ChatId, TopicId};
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PtsUpdate {
    pub chat_id: ChatId,
    pub pts: u64,
    pub pts_count: u32,
    pub event: UpdateEvent,
    pub timestamp_ms: i64,
}

impl PtsUpdate {
    pub fn new(
        chat_id: ChatId,
        pts: u64,
        pts_count: u32,
        event: UpdateEvent,
        timestamp_ms: i64,
    ) -> Self {
        Self {
            chat_id,
            pts,
            pts_count,
            event,
            timestamp_ms,
        }
    }

    /// Computes the PTS state preceding this update.
    /// Invariant: prev_pts = pts - pts_count.
    #[inline]
    pub fn prev_pts(&self) -> u64 {
        self.pts.saturating_sub(self.pts_count as u64)
    }

    /// Determines if this update connects contiguously to the client's current PTS.
    /// Ephemeral updates (pts_count == 0) do not advance sequence numbering and are
    /// considered continuous if the client has reached or passed their reference pts.
    pub fn is_continuous(&self, current_pts: u64) -> bool {
        if self.pts_count == 0 {
            self.pts <= current_pts || self.prev_pts() == current_pts
        } else {
            self.prev_pts() == current_pts
        }
    }

    /// Detects a sequence gap between current PTS and this update.
    /// Returns the missing half-open range `(current_pts + 1)..=prev_pts`.
    pub fn gap(&self, current_pts: u64) -> Option<RangeInclusive<u64>> {
        let prev = self.prev_pts();
        if prev > current_pts {
            Some((current_pts + 1)..=prev)
        } else {
            None
        }
    }

    /// Determines if this update is duplicate or stale relative to current PTS.
    #[inline]
    pub fn is_stale_or_duplicate(&self, current_pts: u64) -> bool {
        self.pts <= current_pts && self.pts_count > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncFilter {
    #[serde(default)]
    pub include_read_states: bool,
    #[serde(default)]
    pub include_reactions: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_id: Option<TopicId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncGetDifferenceRequest {
    pub chat_id: ChatId,
    pub pts: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pts_total_limit: Option<u32>,
    #[serde(default)]
    pub limit: u32,
    #[serde(default)]
    pub date: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

impl SyncGetDifferenceRequest {
    pub fn new(chat_id: ChatId, pts: u64, limit: u32) -> Self {
        Self {
            chat_id,
            pts,
            pts_total_limit: Some(limit),
            limit,
            date: chrono::Utc::now().timestamp(),
            filter: None,
        }
    }

    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncGetDifferenceResponse {
    pub chat_id: ChatId,
    #[serde(default)]
    pub new_messages: Vec<MessageItem>,
    #[serde(default)]
    pub other_updates: Vec<UpdateEvent>,
    pub pts: u64,
    #[serde(default)]
    pub current_pts: u64,
    #[serde(default)]
    pub updates: Vec<PtsUpdate>,
    pub has_more: bool,
    #[serde(default)]
    pub snapshot_required: bool,
}

impl SyncGetDifferenceResponse {
    pub fn new(
        chat_id: ChatId,
        new_messages: Vec<MessageItem>,
        other_updates: Vec<UpdateEvent>,
        pts: u64,
        has_more: bool,
    ) -> Self {
        Self {
            chat_id,
            new_messages,
            other_updates,
            pts,
            current_pts: pts,
            updates: Vec::new(),
            has_more,
            snapshot_required: false,
        }
    }
}
