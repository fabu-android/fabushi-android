//! MTProto RPC Request Builder & Dispatch Helper
//! Upstream Source: `Telegram/SourceFiles/mtproto/sender.h`

use std::collections::HashMap;
use crate::mtproto::core_types::{MsgId, RequestId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sender {
    pub next_request_id: RequestId,
    pub pending_requests: HashMap<RequestId, MsgId>,
    pub handled_count: usize,
}

impl Default for Sender {
    fn default() -> Self {
        Self {
            next_request_id: 1,
            pending_requests: HashMap::new(),
            handled_count: 0,
        }
    }
}

impl Sender {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.next_request_id > 0
    }

    pub fn send(&mut self) -> RequestId {
        let req_id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1);
        self.pending_requests.insert(req_id, 0);
        req_id
    }

    pub fn track_msg_id(&mut self, request_id: RequestId, msg_id: MsgId) {
        if let Some(tracked) = self.pending_requests.get_mut(&request_id) {
            *tracked = msg_id;
        }
    }

    pub fn sender_request_handled(&mut self, request_id: RequestId) {
        if self.pending_requests.remove(&request_id).is_some() {
            self.handled_count = self.handled_count.saturating_add(1);
        }
    }

    pub fn cancel(&mut self, request_id: RequestId) -> bool {
        self.pending_requests.remove(&request_id).is_some()
    }

    pub fn has_pending(&self) -> bool {
        !self.pending_requests.is_empty()
    }
}
