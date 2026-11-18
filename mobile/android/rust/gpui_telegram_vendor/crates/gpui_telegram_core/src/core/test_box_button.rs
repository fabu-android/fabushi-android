//! Telegram Test Harness: Modal Box Button Interaction & State Verification
//! Upstream sources: `Telegram/SourceFiles/test/test_box_button.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ButtonVisualState {
    #[default]
    Normal,
    Hovered,
    Pressed,
    Disabled,
}

/// Simulated modal action button for verifying dialog click mechanics and styles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestBoxButton {
    pub text: String,
    pub state: ButtonVisualState,
    pub clicks_count: u32,
    pub is_confirm: bool,
}

impl Default for TestBoxButton {
    fn default() -> Self {
        Self {
            text: "OK".to_string(),
            state: ButtonVisualState::Normal,
            clicks_count: 0,
            is_confirm: true,
        }
    }
}

impl TestBoxButton {
    pub fn new(text: impl Into<String>, is_confirm: bool) -> Self {
        Self {
            text: text.into(),
            state: ButtonVisualState::Normal,
            clicks_count: 0,
            is_confirm,
        }
    }

    pub fn click(&mut self) -> bool {
        if self.state == ButtonVisualState::Disabled {
            return false;
        }
        self.clicks_count = self.clicks_count.saturating_add(1);
        true
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        if self.state != ButtonVisualState::Disabled {
            self.state = if hovered {
                ButtonVisualState::Hovered
            } else {
                ButtonVisualState::Normal
            };
        }
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        self.state = if disabled {
            ButtonVisualState::Disabled
        } else {
            ButtonVisualState::Normal
        };
    }
}
