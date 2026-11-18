//! Received Message IDs Registry & Deduplication Manager
//! Upstream Source: `Telegram/SourceFiles/mtproto/details/mtproto_received_ids_manager.h`, `mtproto_received_ids_manager.cpp`

use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

pub const DEFAULT_IDS_BUFFER_SIZE: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegisterResult {
    Success,
    Duplicate,
    TooOld,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdLookupState {
    NotFound,
    NeedsAck,
    NoAckNeeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoReceivedIdsManagerDetails {
    pub buffer_capacity: usize,
    ids_need_ack: BTreeMap<u64, bool>,
}

impl Default for MtprotoReceivedIdsManagerDetails {
    fn default() -> Self {
        Self {
            buffer_capacity: DEFAULT_IDS_BUFFER_SIZE,
            ids_need_ack: BTreeMap::new(),
        }
    }
}

impl MtprotoReceivedIdsManagerDetails {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.buffer_capacity > 0
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer_capacity: capacity,
            ids_need_ack: BTreeMap::new(),
        }
    }

    pub fn register_msg_id(&mut self, msg_id: u64, need_ack: bool) -> RegisterResult {
        if self.ids_need_ack.contains_key(&msg_id) {
            return RegisterResult::Duplicate;
        }

        if self.ids_need_ack.len() < self.buffer_capacity || msg_id > self.min() {
            self.ids_need_ack.insert(msg_id, need_ack);
            self.shrink();
            RegisterResult::Success
        } else {
            RegisterResult::TooOld
        }
    }

    pub fn min(&self) -> u64 {
        self.ids_need_ack.keys().next().copied().unwrap_or(0)
    }

    pub fn max(&self) -> u64 {
        self.ids_need_ack.keys().next_back().copied().unwrap_or(0)
    }

    pub fn lookup(&self, msg_id: u64) -> IdLookupState {
        match self.ids_need_ack.get(&msg_id) {
            None => IdLookupState::NotFound,
            Some(true) => IdLookupState::NeedsAck,
            Some(false) => IdLookupState::NoAckNeeded,
        }
    }

    pub fn collect_pending_acks(&mut self) -> Vec<u64> {
        let mut acks = Vec::new();
        for (&msg_id, needs_ack) in self.ids_need_ack.iter_mut() {
            if *needs_ack {
                acks.push(msg_id);
                *needs_ack = false;
            }
        }
        acks
    }

    pub fn shrink(&mut self) {
        while self.ids_need_ack.len() > self.buffer_capacity {
            if let Some(smallest) = self.ids_need_ack.keys().next().copied() {
                self.ids_need_ack.remove(&smallest);
            } else {
                break;
            }
        }
    }

    pub fn clear(&mut self) {
        self.ids_need_ack.clear();
    }

    pub fn len(&self) -> usize {
        self.ids_need_ack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids_need_ack.is_empty()
    }
}
