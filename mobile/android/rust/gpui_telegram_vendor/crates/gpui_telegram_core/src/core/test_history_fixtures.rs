//! Telegram Test Harness: Chat History Fixtures Generator
//! Upstream sources: `Telegram/SourceFiles/test/test_history_fixtures.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockHistoryItem {
    pub id: i64,
    pub peer_id: i64,
    pub sender_id: i64,
    pub text: String,
    pub timestamp: i64,
}

/// Fixture generator populating synthetic history items for layout and scroll tests.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestHistoryFixtures {
    pub generated_items: Vec<MockHistoryItem>,
}

impl TestHistoryFixtures {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn generate_batch(&mut self, peer_id: i64, count: usize) {
        let base_id = self.generated_items.len() as i64 + 1;
        let now = chrono::Utc::now().timestamp();
        for i in 0..count {
            self.generated_items.push(MockHistoryItem {
                id: base_id + (i as i64),
                peer_id,
                sender_id: 1001,
                text: format!("Synthetic message #{}", base_id + (i as i64)),
                timestamp: now + (i as i64),
            });
        }
    }

    pub fn count(&self) -> usize {
        self.generated_items.len()
    }
}
