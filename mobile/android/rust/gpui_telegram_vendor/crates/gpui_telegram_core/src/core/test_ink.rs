//! Telegram Test Harness: Ink Wave & Click Ripple Animation Tests
//! Upstream sources: `Telegram/SourceFiles/test/test_ink.cpp`

use serde::{Deserialize, Serialize};

/// Test fixture verifying material ripple and ink wave expanding circles on click.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestInk {
    pub origin_x: f32,
    pub origin_y: f32,
    pub max_radius: f32,
    pub current_radius: f32,
    pub opacity: f32,
    pub is_animating: bool,
}

impl Default for TestInk {
    fn default() -> Self {
        Self {
            origin_x: 0.0,
            origin_y: 0.0,
            max_radius: 100.0,
            current_radius: 0.0,
            opacity: 1.0,
            is_animating: false,
        }
    }
}

impl TestInk {
    pub fn new(x: f32, y: f32, max_radius: f32) -> Self {
        Self {
            origin_x: x,
            origin_y: y,
            max_radius,
            current_radius: 0.0,
            opacity: 1.0,
            is_animating: true,
        }
    }

    pub fn step(&mut self, delta_radius: f32) {
        if !self.is_animating {
            return;
        }
        self.current_radius += delta_radius;
        if self.current_radius >= self.max_radius {
            self.current_radius = self.max_radius;
            self.opacity = 0.0;
            self.is_animating = false;
        } else {
            self.opacity = 1.0 - (self.current_radius / self.max_radius);
        }
    }
}
