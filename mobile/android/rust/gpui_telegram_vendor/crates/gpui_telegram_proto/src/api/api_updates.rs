//! Telegram Desktop 1:1 Parity Submodule: `ApiUpdates`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_updates.cpp, Telegram/SourceFiles/api/api_updates.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChannelDifferenceRequest {
    #[default]
    Unknown,
    PtsGapOrShortPoll,
    AfterFail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SkipUpdatePolicy {
    #[default]
    SkipNone,
    SkipMessageIds,
    SkipExceptGroupCallParticipants,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveChatTracker {
    pub peer_id: u64,
    pub last_active_timestamp: i64,
    pub is_active: bool,
}

impl ActiveChatTracker {
    pub fn new(peer_id: u64) -> Self {
        Self {
            peer_id,
            last_active_timestamp: chrono::Utc::now().timestamp(),
            is_active: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChannelRangeDifference {
    pub channel_id: u64,
    pub range_min_id: i64,
    pub range_max_id: i64,
    pub pts: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiUpdates {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub pts: i64,
    pub pts_count: i32,
    pub date: i64,
    pub seq: i32,
    pub is_idle: bool,
    pub last_set_online: i64,
    pub requesting_difference: bool,
    pub handling_channel_difference: bool,
    pub active_chats: Vec<ActiveChatTracker>,
    pub channel_pts: HashMap<u64, i64>,
    pub pending_channel_diffs: HashSet<u64>,
    pub updated_at: i64,
}

impl Default for ApiUpdates {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiUpdates".to_string(),
            pts: 1,
            pts_count: 0,
            date: chrono::Utc::now().timestamp(),
            seq: 0,
            is_idle: false,
            last_set_online: chrono::Utc::now().timestamp(),
            requesting_difference: false,
            handling_channel_difference: false,
            active_chats: Vec::new(),
            channel_pts: HashMap::new(),
            pending_channel_diffs: HashSet::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiUpdates {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_pts(pts: i64, date: i64) -> Self {
        Self {
            pts,
            date,
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

    pub fn pts(&self) -> i64 {
        self.pts
    }

    pub fn date(&self) -> i64 {
        self.date
    }

    pub fn seq(&self) -> i32 {
        self.seq
    }

    pub fn update_online(&mut self, last_non_idle_time: i64) {
        let now = chrono::Utc::now().timestamp();
        self.last_set_online = if last_non_idle_time > 0 {
            last_non_idle_time
        } else {
            now
        };
        self.is_idle = false;
        self.updated_at = now;
    }

    pub fn check_idle_finish(&mut self, is_idle: bool) {
        self.is_idle = is_idle;
    }

    pub fn update_and_apply(&mut self, pts: i64, pts_count: i32) -> bool {
        if pts <= self.pts {
            // Duplicate or already-applied update
            return false;
        }

        let prev_pts = pts - (pts_count as i64);
        if prev_pts == self.pts {
            // Contiguous sequence
            self.pts = pts;
            self.pts_count = pts_count;
            self.updated_at = chrono::Utc::now().timestamp();
            true
        } else if prev_pts > self.pts {
            // Sequence gap detected: trigger difference sync
            self.requesting_difference = true;
            false
        } else {
            // Overlapping range
            self.pts = pts;
            self.pts_count = pts_count;
            self.updated_at = chrono::Utc::now().timestamp();
            true
        }
    }

    pub fn update_channel_pts(&mut self, channel_id: u64, pts: i64, pts_count: i32) -> bool {
        let current = self.channel_pts.get(&channel_id).copied().unwrap_or(0);
        if pts <= current {
            return false;
        }
        let prev = pts - (pts_count as i64);
        if prev == current || current == 0 {
            self.channel_pts.insert(channel_id, pts);
            true
        } else {
            self.pending_channel_diffs.insert(channel_id);
            false
        }
    }

    pub fn get_difference(&mut self) {
        self.requesting_difference = true;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn finish_difference(&mut self, new_pts: i64, new_date: i64, new_seq: i32) {
        self.pts = new_pts;
        self.date = new_date;
        self.seq = new_seq;
        self.requesting_difference = false;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn add_active_chat(&mut self, peer_id: u64) {
        if let Some(entry) = self.active_chats.iter_mut().find(|c| c.peer_id == peer_id) {
            entry.is_active = true;
            entry.last_active_timestamp = chrono::Utc::now().timestamp();
        } else {
            self.active_chats.push(ActiveChatTracker::new(peer_id));
        }
    }

    pub fn in_active_chats(&self, peer_id: u64) -> bool {
        self.active_chats
            .iter()
            .any(|c| c.peer_id == peer_id && c.is_active)
    }

    pub fn requesting_difference(&self) -> bool {
        self.requesting_difference
    }

    pub fn handling_channel_difference(&self) -> bool {
        self.handling_channel_difference
    }
}
