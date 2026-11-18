//! Telegram Test Harness: Window Rounded Corner Geometry & Masking
//! Upstream sources: `Telegram/SourceFiles/test/test_corner_patch.cpp`

use serde::{Deserialize, Serialize};

/// Test fixture verifying anti-aliased rounded corner patches for desktop windows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCornerPatch {
    pub radius: u32,
    pub patch_width: u32,
    pub patch_height: u32,
    pub is_applied: bool,
}

impl Default for TestCornerPatch {
    fn default() -> Self {
        Self {
            radius: 8,
            patch_width: 16,
            patch_height: 16,
            is_applied: false,
        }
    }
}

impl TestCornerPatch {
    pub fn new(radius: u32) -> Self {
        Self {
            radius,
            patch_width: radius * 2,
            patch_height: radius * 2,
            is_applied: false,
        }
    }

    pub fn apply(&mut self) {
        self.is_applied = true;
    }

    pub fn unapply(&mut self) {
        self.is_applied = false;
    }
}
