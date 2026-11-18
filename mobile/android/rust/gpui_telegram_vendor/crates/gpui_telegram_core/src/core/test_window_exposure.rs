//! Telegram Desktop 1:1 Parity Module: `TestWindowExposure`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_window_exposure.h, Telegram/SourceFiles/test/test_window_exposure.cpp

use serde::{Deserialize, Serialize};

/// Window exposure, occlusion, and visibility state verification fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestWindowExposure {
    pub is_exposed: bool,
    pub is_occluded: bool,
    pub is_active: bool,
}

impl Default for TestWindowExposure {
    fn default() -> Self {
        Self {
            is_exposed: true,
            is_occluded: false,
            is_active: true,
        }
    }
}

impl TestWindowExposure {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_occluded(&mut self, occluded: bool) {
        self.is_occluded = occluded;
        if occluded {
            self.is_exposed = false;
        }
    }

    pub fn set_active(&mut self, active: bool) {
        self.is_active = active;
    }
}
