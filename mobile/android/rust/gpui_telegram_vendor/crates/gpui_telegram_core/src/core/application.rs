//! Telegram Desktop Core Application Lifecycle & Window Coordination
//! Upstream sources: `Telegram/SourceFiles/core/application.h`, `application.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LaunchState {
    #[default]
    Running,
    QuitRequested,
    QuitProcessed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuitReason {
    Default,
    SystemEvent,
    UserLogout,
    CriticalError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyChange {
    pub was: Option<String>,
    pub now: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WindowState {
    #[default]
    Normal,
    Minimized,
    Maximized,
    FullScreen,
    Hidden,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: WindowId,
    pub title: String,
    pub state: WindowState,
    pub is_primary: bool,
    pub separate_chat_peer: Option<u64>,
}

/// Central application coordinator managing desktop lifecycle, window stacks,
/// auto-lock security timers, unread badges, and quit flows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Application {
    pub launch_state: LaunchState,
    pub quit_reason: Option<QuitReason>,
    pub active_window_id: Option<WindowId>,
    pub window_stack: Vec<WindowId>,
    pub windows: Vec<WindowInfo>,
    pub unread_badge_count: i32,
    pub unread_badge_muted: bool,
    pub passcode_locked: bool,
    pub last_non_idle_time: i64,
    pub auto_lock_timeout_secs: i64,
    pub screen_is_locked: bool,
    pub system_dark_mode: bool,
    pub debug_mode: bool,
}

impl Default for Application {
    fn default() -> Self {
        Self {
            launch_state: LaunchState::Running,
            quit_reason: None,
            active_window_id: None,
            window_stack: Vec::new(),
            windows: Vec::new(),
            unread_badge_count: 0,
            unread_badge_muted: false,
            passcode_locked: false,
            last_non_idle_time: chrono::Utc::now().timestamp(),
            auto_lock_timeout_secs: 3600,
            screen_is_locked: false,
            system_dark_mode: false,
            debug_mode: false,
        }
    }
}

impl Application {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn run(&mut self) {
        self.launch_state = LaunchState::Running;
        self.last_non_idle_time = chrono::Utc::now().timestamp();
    }

    pub fn is_running(&self) -> bool {
        self.launch_state == LaunchState::Running
    }

    pub fn register_window(&mut self, id: WindowId, title: String, is_primary: bool) {
        if self.windows.iter().any(|w| w.id == id) {
            return;
        }
        self.windows.push(WindowInfo {
            id,
            title,
            state: WindowState::Normal,
            is_primary,
            separate_chat_peer: None,
        });
        self.window_stack.push(id);
        if is_primary || self.active_window_id.is_none() {
            self.active_window_id = Some(id);
        }
    }

    pub fn unregister_window(&mut self, id: WindowId) {
        self.windows.retain(|w| w.id != id);
        self.window_stack.retain(|&w| w != id);
        if self.active_window_id == Some(id) {
            self.active_window_id = self.window_stack.last().copied();
        }
    }

    pub fn set_active_primary_window(&mut self, id: WindowId) {
        if self.windows.iter().any(|w| w.id == id) {
            self.active_window_id = Some(id);
            self.window_stack.retain(|&w| w != id);
            self.window_stack.push(id);
        }
    }

    pub fn has_active_window(&self) -> bool {
        self.active_window_id.is_some()
    }

    pub fn close_active_window(&mut self) -> bool {
        if let Some(active_id) = self.active_window_id {
            self.unregister_window(active_id);
            true
        } else {
            false
        }
    }

    pub fn close_all_windows(&mut self) {
        self.windows.clear();
        self.window_stack.clear();
        self.active_window_id = None;
    }

    pub fn close_other_windows(&mut self) -> bool {
        if let Some(active_id) = self.active_window_id {
            self.windows.retain(|w| w.id == active_id);
            self.window_stack.retain(|&w| w == active_id);
            true
        } else {
            false
        }
    }

    pub fn minimize_active_window(&mut self) -> bool {
        if let Some(active_id) = self.active_window_id {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == active_id) {
                win.state = WindowState::Minimized;
                return true;
            }
        }
        false
    }

    pub fn toggle_active_window_full_screen(&mut self) -> bool {
        if let Some(active_id) = self.active_window_id {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == active_id) {
                win.state = match win.state {
                    WindowState::FullScreen => WindowState::Normal,
                    _ => WindowState::FullScreen,
                };
                return true;
            }
        }
        false
    }

    pub fn set_unread_badge(&mut self, count: i32, muted: bool) {
        self.unread_badge_count = count.max(0);
        self.unread_badge_muted = muted;
    }

    pub fn lock_by_passcode(&mut self) {
        self.passcode_locked = true;
    }

    pub fn unlock_passcode(&mut self) {
        self.passcode_locked = false;
        self.last_non_idle_time = chrono::Utc::now().timestamp();
    }

    pub fn check_auto_lock(&mut self, now_secs: i64) -> bool {
        if self.auto_lock_timeout_secs > 0 && !self.passcode_locked {
            if now_secs.saturating_sub(self.last_non_idle_time) >= self.auto_lock_timeout_secs {
                self.passcode_locked = true;
                return true;
            }
        }
        false
    }

    pub fn update_non_idle(&mut self, now_secs: i64) {
        self.last_non_idle_time = now_secs;
    }

    pub fn set_screen_is_locked(&mut self, locked: bool) {
        self.screen_is_locked = locked;
    }

    pub fn check_system_dark_mode(&mut self, dark_mode: bool) {
        self.system_dark_mode = dark_mode;
    }

    pub fn request_quit(&mut self, reason: QuitReason) {
        self.launch_state = LaunchState::QuitRequested;
        self.quit_reason = Some(reason);
    }

    pub fn ready_to_quit(&self) -> bool {
        self.launch_state == LaunchState::QuitRequested
    }

    pub fn finish_quit(&mut self) {
        self.launch_state = LaunchState::QuitProcessed;
        self.close_all_windows();
    }
}
