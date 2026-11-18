//! Telegram Test Harness: Widget Hover State & Mouse Enter/Leave Tracking
//! Upstream sources: `Telegram/SourceFiles/test/test_hover.cpp`

use serde::{Deserialize, Serialize};

/// Test fixture tracking mouse hover entry, leave, and duration over interactive components.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestHover {
    pub is_hovered: bool,
    pub enter_count: u32,
    pub leave_count: u32,
}

impl TestHover {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enter(&mut self) {
        if !self.is_hovered {
            self.is_hovered = true;
            self.enter_count = self.enter_count.saturating_add(1);
        }
    }

    pub fn leave(&mut self) {
        if self.is_hovered {
            self.is_hovered = false;
            self.leave_count = self.leave_count.saturating_add(1);
        }
    }
}
