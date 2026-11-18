//! Telegram Desktop Screenshot & Window Capture Protection Subsystem
//! Upstream sources: `Telegram/SourceFiles/core/core_screenshot_protection.h`, `core_screenshot_protection.cpp`

use std::collections::{HashMap, HashSet};

/// Window protection coordinator managing OS-level screen capture prevention flags.
#[derive(Debug, Default, Clone)]
pub struct CoreScreenshotProtection {
    reasons_by_window: HashMap<u64, HashSet<u64>>,
    protected_windows: HashSet<u64>,
    next_reason_id: u64,
}

impl CoreScreenshotProtection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_content_reason(&mut self, window_id: u64) -> u64 {
        self.next_reason_id = self.next_reason_id.wrapping_add(1);
        let id = self.next_reason_id;
        self.reasons_by_window.entry(window_id).or_default().insert(id);
        self.protected_windows.insert(window_id);
        id
    }

    pub fn remove_content_reason(&mut self, window_id: u64, reason_id: u64) -> bool {
        if let Some(reasons) = self.reasons_by_window.get_mut(&window_id) {
            reasons.remove(&reason_id);
            if reasons.is_empty() {
                self.reasons_by_window.remove(&window_id);
                self.protected_windows.remove(&window_id);
            }
            return true;
        }
        false
    }

    pub fn is_window_protected(&self, window_id: u64) -> bool {
        self.protected_windows.contains(&window_id)
    }

    pub fn active_protection_count(&self) -> usize {
        self.protected_windows.len()
    }
}
