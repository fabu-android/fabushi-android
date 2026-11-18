//! Telegram Desktop 1:1 Parity Module: `TestStyle`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_style.h, Telegram/SourceFiles/test/test_style.cpp

use serde::{Deserialize, Serialize};

/// Style metrics and layout constraint validation fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestStyle {
    pub button_height: i32,
    pub input_padding: i32,
    pub default_margin: i32,
}

impl Default for TestStyle {
    fn default() -> Self {
        Self {
            button_height: 36,
            input_padding: 8,
            default_margin: 12,
        }
    }
}

impl TestStyle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_valid(&self) -> bool {
        self.button_height > 0 && self.input_padding >= 0 && self.default_margin >= 0
    }
}
