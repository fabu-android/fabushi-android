//! Telegram Test Harness: Screen and Window Capture Verification
//! Upstream sources: `Telegram/SourceFiles/test/test_capture.cpp`

use serde::{Deserialize, Serialize};

/// Test fixture simulating window frame capture and pixel buffer validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCapture {
    pub width: u32,
    pub height: u32,
    pub captured_frames: u32,
    pub last_frame_checksum: u64,
}

impl Default for TestCapture {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            captured_frames: 0,
            last_frame_checksum: 0,
        }
    }
}

impl TestCapture {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            captured_frames: 0,
            last_frame_checksum: 0,
        }
    }

    pub fn capture_frame(&mut self, fake_pixel_sum: u64) {
        self.captured_frames = self.captured_frames.saturating_add(1);
        self.last_frame_checksum = fake_pixel_sum;
    }

    pub fn frame_count(&self) -> u32 {
        self.captured_frames
    }
}
