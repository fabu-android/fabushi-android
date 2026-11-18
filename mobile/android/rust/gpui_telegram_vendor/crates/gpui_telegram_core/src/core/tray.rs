//! Telegram Desktop 1:1 Parity Module: `Tray`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/tray.h, Telegram/SourceFiles/tray.cpp

use serde::{Deserialize, Serialize};

/// System tray indicator management, icon badge counters, and menu lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tray {
    pub is_visible: bool,
    pub unread_counter: i32,
    pub unread_unmuted_counter: i32,
    pub sound_notifications_enabled: bool,
    pub window_is_minimized: bool,
}

impl Default for Tray {
    fn default() -> Self {
        Self {
            is_visible: false,
            unread_counter: 0,
            unread_unmuted_counter: 0,
            sound_notifications_enabled: true,
            window_is_minimized: false,
        }
    }
}

impl Tray {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&mut self) {
        self.is_visible = true;
    }

    pub fn destroy(&mut self) {
        self.is_visible = false;
    }

    pub fn has(&self) -> bool {
        self.is_visible
    }

    pub fn update_icon_counters(&mut self, total_unread: i32, unmuted_unread: i32) {
        self.unread_counter = total_unread.max(0);
        self.unread_unmuted_counter = unmuted_unread.max(0);
    }

    pub fn toggle_sound_notifications(&mut self) -> bool {
        self.sound_notifications_enabled = !self.sound_notifications_enabled;
        self.sound_notifications_enabled
    }

    pub fn set_window_minimized(&mut self, minimized: bool) {
        self.window_is_minimized = minimized;
    }

    /// Generates system tray tooltip in the upstream format:
    /// "Telegram Desktop" or "Telegram Desktop (3)".
    pub fn tooltip(&self) -> String {
        if self.unread_counter > 0 {
            format!("Telegram Desktop ({})", self.unread_counter)
        } else {
            "Telegram Desktop".to_string()
        }
    }
}

pub fn tray_icon_tooltip(unread_count: i32) -> String {
    if unread_count > 0 {
        format!("Telegram Desktop ({})", unread_count)
    } else {
        "Telegram Desktop".to_string()
    }
}
