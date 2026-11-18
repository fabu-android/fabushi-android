//! Telegram Desktop 1:1 Parity Module: `Settings`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/settings.h, Telegram/SourceFiles/settings.cpp

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum LaunchMode {
    #[default]
    Normal = 0,
    AutoStart = 1,
    FixPrevious = 2,
    Cleanup = 3,
}

/// Legacy global application settings matching upstream `settings.h`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub lang_rtl: bool,
    pub install_beta_version: bool,
    pub auto_start: bool,
    pub start_minimized: bool,
    pub start_in_tray: bool,
    pub send_to_menu: bool,
    pub launch_mode: LaunchMode,
    pub working_dir: String,
    pub dialog_last_path: String,
    pub auto_update: bool,
    pub debug_mode: bool,
    pub test_agent: bool,
    pub screen_scale: i32,
    pub config_scale: i32,
    pub passcode_bad_tries: i32,
    pub passcode_last_try_timestamp_ms: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lang_rtl: false,
            install_beta_version: false,
            auto_start: false,
            start_minimized: false,
            start_in_tray: false,
            send_to_menu: false,
            launch_mode: LaunchMode::Normal,
            working_dir: String::new(),
            dialog_last_path: String::new(),
            auto_update: true,
            debug_mode: false,
            test_agent: false,
            screen_scale: 100,
            config_scale: 100,
            passcode_bad_tries: 0,
            passcode_last_try_timestamp_ms: 0,
        }
    }
}

impl Settings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates UI scale percentage; 0 means auto-scaled to screen.
    pub fn eval_scale(&self) -> i32 {
        if self.config_scale == 0 {
            self.screen_scale
        } else {
            self.config_scale
        }
    }

    /// Computes progressive passcode attempt delay (5s, 10s, 15s, 20s, 25s, 30s)
    /// to mitigate local brute-force attacks.
    pub fn passcode_can_try(&self, current_time_ms: i64) -> bool {
        if self.passcode_bad_tries < 3 {
            return true;
        }
        let dt = current_time_ms - self.passcode_last_try_timestamp_ms;
        let required_delay_ms = match self.passcode_bad_tries {
            3 => 5_000,
            4 => 10_000,
            5 => 15_000,
            6 => 20_000,
            7 => 25_000,
            _ => 30_000,
        };
        dt >= required_delay_ms
    }

    pub fn record_passcode_failure(&mut self, current_time_ms: i64) {
        self.passcode_bad_tries = self.passcode_bad_tries.saturating_add(1);
        self.passcode_last_try_timestamp_ms = current_time_ms;
    }

    pub fn reset_passcode_tries(&mut self) {
        self.passcode_bad_tries = 0;
        self.passcode_last_try_timestamp_ms = 0;
    }
}
