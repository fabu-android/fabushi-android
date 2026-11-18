//! Telegram Client-Server API Coordination Layer
//! Upstream sources: `Telegram/SourceFiles/apiwrap.h`, `apiwrap.cpp`

use serde::{Deserialize, Serialize};

/// High-level API dispatcher orchestrating account authorizations, blocked peers,
/// privacy settings, and real-time updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Apiwrap {
    pub session_id: u64,
    pub is_connected: bool,
    pub blocked_peers_count: usize,
    pub pending_requests_count: usize,
    pub sensitive_content_enabled: bool,
}

impl Default for Apiwrap {
    fn default() -> Self {
        Self {
            session_id: 0,
            is_connected: false,
            blocked_peers_count: 0,
            pending_requests_count: 0,
            sensitive_content_enabled: false,
        }
    }
}

impl Apiwrap {
    pub fn new(session_id: u64) -> Self {
        Self {
            session_id,
            is_connected: false,
            blocked_peers_count: 0,
            pending_requests_count: 0,
            sensitive_content_enabled: false,
        }
    }

    pub fn set_connected(&mut self, connected: bool) {
        self.is_connected = connected;
    }

    pub fn set_blocked_peers_count(&mut self, count: usize) {
        self.blocked_peers_count = count;
    }

    pub fn set_sensitive_content_enabled(&mut self, enabled: bool) {
        self.sensitive_content_enabled = enabled;
    }

    pub fn enqueue_request(&mut self) {
        self.pending_requests_count = self.pending_requests_count.saturating_add(1);
    }

    pub fn finish_request(&mut self) {
        self.pending_requests_count = self.pending_requests_count.saturating_sub(1);
    }
}
