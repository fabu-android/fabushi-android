//! Telegram Desktop 1:1 Parity Module: `UiIntegration`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/ui_integration.h, Telegram/SourceFiles/core/ui_integration.cpp

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// UI integration bridge connecting platform UI primitives to core Telegram logic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiIntegration {
    pub touch_counter: i32,
    pub screen_is_locked: bool,
    pub emoji_cache_dir: PathBuf,
    pub fonts_cache_dir: PathBuf,
}

impl Default for UiIntegration {
    fn default() -> Self {
        let base_dir = std::env::temp_dir().join("telegram_cache");
        Self {
            touch_counter: 0,
            screen_is_locked: false,
            emoji_cache_dir: base_dir.join("emoji"),
            fonts_cache_dir: base_dir.join("fonts"),
        }
    }
}

impl UiIntegration {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn touch_counter_increment(&mut self) {
        self.touch_counter = self.touch_counter.saturating_add(1);
    }

    pub fn touch_counter_now(&self) -> i32 {
        self.touch_counter
    }

    pub fn set_screen_locked(&mut self, locked: bool) {
        self.screen_is_locked = locked;
    }

    pub fn is_screen_locked(&self) -> bool {
        self.screen_is_locked
    }

    pub fn emoji_cache_folder(&self) -> &std::path::Path {
        &self.emoji_cache_dir
    }

    pub fn fonts_cache_folder(&self) -> &std::path::Path {
        &self.fonts_cache_dir
    }

    // Standard UI phrasing tokens matching upstream Core::UiIntegration
    pub fn phrase_context_copy_text(&self) -> &'static str {
        "Copy Text"
    }

    pub fn phrase_context_copy_link(&self) -> &'static str {
        "Copy Link"
    }

    pub fn phrase_context_copy_selected(&self) -> &'static str {
        "Copy Selected"
    }

    pub fn phrase_formatting_bold(&self) -> &'static str {
        "Bold"
    }

    pub fn phrase_formatting_italic(&self) -> &'static str {
        "Italic"
    }

    pub fn phrase_formatting_monospace(&self) -> &'static str {
        "Monospace"
    }

    pub fn phrase_formatting_spoiler(&self) -> &'static str {
        "Spoiler"
    }

    pub fn phrase_formatting_blockquote(&self) -> &'static str {
        "Quote"
    }

    pub fn phrase_button_ok(&self) -> &'static str {
        "OK"
    }

    pub fn phrase_button_cancel(&self) -> &'static str {
        "Cancel"
    }

    pub fn phrase_button_close(&self) -> &'static str {
        "Close"
    }
}
