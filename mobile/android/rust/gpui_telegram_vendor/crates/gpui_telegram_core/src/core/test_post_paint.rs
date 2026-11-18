//! Telegram Desktop 1:1 Parity Module: `TestPostPaint`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_post_paint.h, Telegram/SourceFiles/test/test_post_paint.cpp

use serde::{Deserialize, Serialize};

/// Test fixture tracking post-paint rendering passes and overlay triggers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestPostPaint {
    pub frames_rendered: usize,
    pub overlays_drawn: usize,
}

impl TestPostPaint {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_frame(&mut self) {
        self.frames_rendered += 1;
    }

    pub fn record_overlay(&mut self) {
        self.overlays_drawn += 1;
    }
}
