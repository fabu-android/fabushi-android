//! Telegram Test Harness: Virtual Animation Clock & Ticking
//! Upstream sources: `Telegram/SourceFiles/test/test_animation_clock.cpp`

use serde::{Deserialize, Serialize};

/// Deterministic animation clock for testing transitions and easing without real-time delays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestAnimationClock {
    pub current_time_ms: i64,
    pub step_ms: i64,
    pub total_ticks: u64,
    pub is_active: bool,
}

impl Default for TestAnimationClock {
    fn default() -> Self {
        Self {
            current_time_ms: 0,
            step_ms: 16, // 60 FPS tick
            total_ticks: 0,
            is_active: false,
        }
    }
}

impl TestAnimationClock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self) {
        self.is_active = true;
    }

    pub fn tick(&mut self) -> i64 {
        if self.is_active {
            self.current_time_ms = self.current_time_ms.saturating_add(self.step_ms);
            self.total_ticks = self.total_ticks.saturating_add(1);
        }
        self.current_time_ms
    }

    pub fn advance_by(&mut self, ms: i64) {
        self.current_time_ms = self.current_time_ms.saturating_add(ms);
        let ticks = (ms / self.step_ms).max(1) as u64;
        self.total_ticks = self.total_ticks.saturating_add(ticks);
    }

    pub fn reset(&mut self) {
        self.current_time_ms = 0;
        self.total_ticks = 0;
        self.is_active = false;
    }
}
