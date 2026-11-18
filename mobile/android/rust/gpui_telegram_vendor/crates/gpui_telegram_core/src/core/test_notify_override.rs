//! Telegram Desktop 1:1 Parity Module: `TestNotifyOverride`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_notify_override.h, Telegram/SourceFiles/test/test_notify_override.cpp

use serde::{Deserialize, Serialize};

/// Peer-specific notification overrides verification fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestNotifyOverride {
    pub peer_id: u64,
    pub mute_until: i64,
    pub custom_sound: Option<String>,
    pub show_preview: bool,
}

impl Default for TestNotifyOverride {
    fn default() -> Self {
        Self {
            peer_id: 0,
            mute_until: 0,
            custom_sound: None,
            show_preview: true,
        }
    }
}

impl TestNotifyOverride {
    pub fn new(peer_id: u64) -> Self {
        Self {
            peer_id,
            mute_until: 0,
            custom_sound: None,
            show_preview: true,
        }
    }

    pub fn mute_for_seconds(&mut self, seconds: i64) {
        let now = chrono::Utc::now().timestamp();
        self.mute_until = now + seconds;
    }

    pub fn is_muted(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        self.mute_until > now
    }
}
