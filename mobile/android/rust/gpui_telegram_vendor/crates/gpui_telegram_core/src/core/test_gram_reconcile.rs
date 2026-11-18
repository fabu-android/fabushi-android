//! Telegram Test Harness: Message Grammar & Local-Remote Cache Reconciliation
//! Upstream sources: `Telegram/SourceFiles/test/test_gram_reconcile.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRecord {
    pub msg_id: i64,
    pub text: String,
    pub date: i64,
}

/// Test fixture verifying message reconciliation between memory state and SQLite storage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestGramReconcile {
    pub local_messages: Vec<MessageRecord>,
    pub server_messages: Vec<MessageRecord>,
    pub reconciled_count: usize,
}

impl TestGramReconcile {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_local(&mut self, msg_id: i64, text: impl Into<String>, date: i64) {
        self.local_messages.push(MessageRecord {
            msg_id,
            text: text.into(),
            date,
        });
    }

    pub fn add_server(&mut self, msg_id: i64, text: impl Into<String>, date: i64) {
        self.server_messages.push(MessageRecord {
            msg_id,
            text: text.into(),
            date,
        });
    }

    pub fn reconcile(&mut self) -> usize {
        let mut count = 0;
        for s_msg in &self.server_messages {
            if !self.local_messages.iter().any(|m| m.msg_id == s_msg.msg_id) {
                self.local_messages.push(s_msg.clone());
                count += 1;
            }
        }
        self.reconciled_count = count;
        count
    }
}
