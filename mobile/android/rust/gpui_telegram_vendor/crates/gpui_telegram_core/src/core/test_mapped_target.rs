//! Telegram Desktop 1:1 Parity Module: `TestMappedTarget`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_mapped_target.h, Telegram/SourceFiles/test/test_mapped_target.cpp

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestTargetRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl TestTargetRect {
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }
}

/// Simulated UI event coordinate mapper targeting specific widgets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestMappedTarget {
    pub target_id: String,
    pub bounds: TestTargetRect,
    pub is_enabled: bool,
}

impl Default for TestMappedTarget {
    fn default() -> Self {
        Self {
            target_id: String::new(),
            bounds: TestTargetRect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            is_enabled: true,
        }
    }
}

impl TestMappedTarget {
    pub fn new(id: impl Into<String>, bounds: TestTargetRect) -> Self {
        Self {
            target_id: id.into(),
            bounds,
            is_enabled: true,
        }
    }

    pub fn hit_test(&self, x: i32, y: i32) -> bool {
        self.is_enabled && self.bounds.contains(x, y)
    }
}
