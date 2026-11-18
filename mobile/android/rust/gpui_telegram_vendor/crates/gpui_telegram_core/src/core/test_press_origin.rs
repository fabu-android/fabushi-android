//! Telegram Desktop 1:1 Parity Module: `TestPressOrigin`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_press_origin.h, Telegram/SourceFiles/test/test_press_origin.cpp

use serde::{Deserialize, Serialize};

/// Pointer press tracking fixture for drag threshold detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestPressOrigin {
    pub press_x: i32,
    pub press_y: i32,
    pub drag_threshold: i32,
}

impl Default for TestPressOrigin {
    fn default() -> Self {
        Self {
            press_x: 0,
            press_y: 0,
            drag_threshold: 4,
        }
    }
}

impl TestPressOrigin {
    pub fn new(x: i32, y: i32) -> Self {
        Self {
            press_x: x,
            press_y: y,
            drag_threshold: 4,
        }
    }

    pub fn is_drag(&self, current_x: i32, current_y: i32) -> bool {
        let dx = (current_x - self.press_x).abs();
        let dy = (current_y - self.press_y).abs();
        dx > self.drag_threshold || dy > self.drag_threshold
    }
}
