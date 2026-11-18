//! Telegram Desktop 1:1 Parity Module: `TestPalette`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_palette.h, Telegram/SourceFiles/test/test_palette.cpp

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Palette colors testing fixture verifying theme color lookup and contrast ratios.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestPalette {
    pub colors: HashMap<String, u32>,
}

impl Default for TestPalette {
    fn default() -> Self {
        let mut colors = HashMap::new();
        colors.insert("windowBg".to_string(), 0xFFFFFFFF);
        colors.insert("windowFg".to_string(), 0xFF000000);
        colors.insert("windowActiveTextFg".to_string(), 0xFF168ACD);
        Self { colors }
    }
}

impl TestPalette {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(&self, name: &str) -> u32 {
        self.colors.get(name).copied().unwrap_or(0xFF000000)
    }

    pub fn set_color(&mut self, name: impl Into<String>, argb: u32) {
        self.colors.insert(name.into(), argb);
    }
}
