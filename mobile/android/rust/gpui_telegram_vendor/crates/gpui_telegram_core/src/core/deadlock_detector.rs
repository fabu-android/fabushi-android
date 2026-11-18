//! Telegram Desktop Event Loop Deadlock Detector Watchdog
//! Upstream sources: `Telegram/SourceFiles/core/deadlock_detector.h`

use serde::{Deserialize, Serialize};

pub const DEFAULT_PING_INTERVAL_SECS: u64 = 60;
pub const DEFAULT_ABORT_TIMEOUT_SECS: u64 = 30;

/// Watchdog monitor detecting hangs in the main application UI thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadlockDetector {
    pub ping_interval_secs: u64,
    pub timeout_secs: u64,
    pub last_ping_timestamp: i64,
    pub last_pong_timestamp: i64,
    pub is_waiting_pong: bool,
    pub is_running: bool,
    pub deadlock_detected: bool,
}

impl Default for DeadlockDetector {
    fn default() -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            ping_interval_secs: DEFAULT_PING_INTERVAL_SECS,
            timeout_secs: DEFAULT_ABORT_TIMEOUT_SECS,
            last_ping_timestamp: now,
            last_pong_timestamp: now,
            is_waiting_pong: false,
            is_running: true,
            deadlock_detected: false,
        }
    }
}

impl DeadlockDetector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn post_ping(&mut self, now_secs: i64) {
        self.last_ping_timestamp = now_secs;
        self.is_waiting_pong = true;
    }

    pub fn receive_pong(&mut self, now_secs: i64) {
        self.last_pong_timestamp = now_secs;
        self.is_waiting_pong = false;
        self.deadlock_detected = false;
    }

    pub fn check_hang(&mut self, now_secs: i64) -> bool {
        if !self.is_running {
            return false;
        }
        if self.is_waiting_pong {
            let elapsed = now_secs.saturating_sub(self.last_ping_timestamp);
            if elapsed > 0 && (elapsed as u64) >= self.timeout_secs {
                self.deadlock_detected = true;
                return true;
            }
        }
        false
    }

    pub fn stop(&mut self) {
        self.is_running = false;
    }
}
