//! Telegram Test Harness: Synthetic Event Dispatch & Automated Test Agent
//! Upstream sources: `Telegram/SourceFiles/test/test_agent.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntheticEvent {
    pub name: String,
    pub timestamp_ms: i64,
    pub payload: Option<String>,
}

/// Simulated agent for automated headless UI testing and synthetic event replay.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestAgent {
    pub is_enabled: bool,
    pub recorded_events: Vec<SyntheticEvent>,
    pub step_index: usize,
}

impl TestAgent {
    pub fn new() -> Self {
        Self {
            is_enabled: true,
            recorded_events: Vec::new(),
            step_index: 0,
        }
    }

    pub fn dispatch_event(&mut self, name: impl Into<String>, payload: Option<String>) {
        if !self.is_enabled {
            return;
        }
        self.recorded_events.push(SyntheticEvent {
            name: name.into(),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            payload,
        });
        self.step_index = self.step_index.saturating_add(1);
    }

    pub fn has_event(&self, name: &str) -> bool {
        self.recorded_events.iter().any(|e| e.name == name)
    }

    pub fn clear(&mut self) {
        self.recorded_events.clear();
        self.step_index = 0;
    }
}
