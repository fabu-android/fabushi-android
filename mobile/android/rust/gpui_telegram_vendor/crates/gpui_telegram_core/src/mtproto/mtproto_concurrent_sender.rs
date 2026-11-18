//! Multi-Threaded Concurrent RPC Sender Pool
//! Upstream Source: `Telegram/SourceFiles/mtproto/mtproto_concurrent_sender.h`, `mtproto_concurrent_sender.cpp`

use std::collections::HashMap;
use super::sender::Sender;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoConcurrentSender {
    pub senders: HashMap<usize, Sender>,
    pub thread_count: usize,
}

impl Default for MtprotoConcurrentSender {
    fn default() -> Self {
        Self {
            senders: HashMap::new(),
            thread_count: 4,
        }
    }
}

impl MtprotoConcurrentSender {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.thread_count > 0
    }

    pub fn sender_for_thread(&mut self, thread_idx: usize) -> &mut Sender {
        self.senders.entry(thread_idx).or_default()
    }

    pub fn cancel_all(&mut self) {
        for sender in self.senders.values_mut() {
            sender.pending_requests.clear();
        }
    }
}
