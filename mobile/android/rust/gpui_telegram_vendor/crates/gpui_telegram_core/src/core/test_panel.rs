//! Telegram Desktop 1:1 Parity Module: `TestPanel`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_panel.h, Telegram/SourceFiles/test/test_panel.cpp

use serde::{Deserialize, Serialize};

/// Test fixture verifying floating panel / drawer dimensions and visibility states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestPanel {
    pub is_visible: bool,
    pub width: i32,
    pub height: i32,
    pub is_pinned: bool,
}

impl Default for TestPanel {
    fn default() -> Self {
        Self {
            is_visible: false,
            width: 320,
            height: 600,
            is_pinned: false,
        }
    }
}

impl TestPanel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show(&mut self) {
        self.is_visible = true;
    }

    pub fn hide(&mut self) {
        self.is_visible = false;
    }

    pub fn set_pinned(&mut self, pinned: bool) {
        self.is_pinned = pinned;
    }
}
