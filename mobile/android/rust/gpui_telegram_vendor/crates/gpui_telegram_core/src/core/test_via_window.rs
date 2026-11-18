//! Telegram Desktop 1:1 Parity Module: `TestViaWindow`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_via_window.h, Telegram/SourceFiles/test/test_via_window.cpp

use serde::{Deserialize, Serialize};

/// Window coordinate mapping and event redirection test fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestViaWindow {
    pub window_x: i32,
    pub window_y: i32,
    pub events_dispatched: usize,
}

impl Default for TestViaWindow {
    fn default() -> Self {
        Self {
            window_x: 100,
            window_y: 100,
            events_dispatched: 0,
        }
    }
}

impl TestViaWindow {
    pub fn new(x: i32, y: i32) -> Self {
        Self {
            window_x: x,
            window_y: y,
            events_dispatched: 0,
        }
    }

    pub fn to_local_coordinates(&self, global_x: i32, global_y: i32) -> (i32, i32) {
        (global_x - self.window_x, global_y - self.window_y)
    }

    pub fn dispatch_event(&mut self) {
        self.events_dispatched += 1;
    }
}
