//! Telegram Test Harness: Window Activation & Focus Tracking
//! Upstream sources: `Telegram/SourceFiles/test/test_activation.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FocusState {
    Focused,
    Unfocused,
    Pending,
}

/// Test fixture verifying window focus transitions, widget activation, and event propagation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestActivation {
    pub window_id: u64,
    pub focus_state: FocusState,
    pub is_active_window: bool,
    pub activation_events_count: u32,
    pub focused_widget_name: Option<String>,
}

impl Default for TestActivation {
    fn default() -> Self {
        Self {
            window_id: 1,
            focus_state: FocusState::Unfocused,
            is_active_window: false,
            activation_events_count: 0,
            focused_widget_name: None,
        }
    }
}

impl TestActivation {
    pub fn new(window_id: u64) -> Self {
        Self {
            window_id,
            ..Self::default()
        }
    }

    pub fn activate_window(&mut self) {
        self.is_active_window = true;
        self.focus_state = FocusState::Focused;
        self.activation_events_count = self.activation_events_count.saturating_add(1);
    }

    pub fn deactivate_window(&mut self) {
        self.is_active_window = false;
        self.focus_state = FocusState::Unfocused;
        self.focused_widget_name = None;
    }

    pub fn set_focus_widget(&mut self, widget_name: impl Into<String>) {
        if self.is_active_window {
            self.focused_widget_name = Some(widget_name.into());
            self.focus_state = FocusState::Focused;
        }
    }

    pub fn has_focus(&self) -> bool {
        self.focus_state == FocusState::Focused
    }
}
