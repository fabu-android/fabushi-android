//! Telegram Desktop 1:1 Parity Module: `TestWheel`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_wheel.h, Telegram/SourceFiles/test/test_wheel.cpp

use serde::{Deserialize, Serialize};

/// Mouse wheel delta normalization and inertia scrolling fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestWheel {
    pub accumulated_delta_y: i32,
    pub lines_scrolled: i32,
}

impl Default for TestWheel {
    fn default() -> Self {
        Self {
            accumulated_delta_y: 0,
            lines_scrolled: 0,
        }
    }
}

impl TestWheel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scroll(&mut self, delta_y: i32) {
        self.accumulated_delta_y += delta_y;
        let lines = self.accumulated_delta_y / 120;
        if lines != 0 {
            self.lines_scrolled += lines;
            self.accumulated_delta_y %= 120;
        }
    }
}
