//! Telegram Passport UI Item Row Component with Error Transitions
//! Upstream: `Telegram/SourceFiles/passport/ui/passport_form_row.h`, `passport_form_row.cpp`
//! Subsystem: `passport`, Feature ID: `TCG-N02`

use serde::{Deserialize, Serialize};

pub const UI_ROW_PADDING_LEFT: f32 = 22.0;
pub const UI_ROW_PADDING_TOP: f32 = 8.0;
pub const UI_ROW_PADDING_RIGHT: f32 = 25.0;
pub const UI_ROW_PADDING_BOTTOM: f32 = 8.0;
pub const UI_ROW_SKIP: f32 = 2.0;
pub const UI_ROW_ICON_WIDTH: f32 = 24.0;
pub const UI_ROW_ICON_SKIP: f32 = 10.0;

/// Interactive UI row for document checklists featuring error color animation states.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassportFormRowUi {
    pub row_title: String,
    pub row_description: String,
    pub is_ready: bool,
    pub is_error: bool,
    pub error_animation_progress: f32,
}

impl Default for PassportFormRowUi {
    fn default() -> Self {
        Self {
            row_title: String::new(),
            row_description: String::new(),
            is_ready: false,
            is_error: false,
            error_animation_progress: 0.0,
        }
    }
}

impl PassportFormRowUi {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            row_title: title.into(),
            row_description: description.into(),
            is_ready: false,
            is_error: false,
            error_animation_progress: 0.0,
        }
    }

    pub fn update_content(&mut self, title: impl Into<String>, description: impl Into<String>, ready: bool, error: bool) {
        self.row_title = title.into();
        self.row_description = description.into();
        self.is_ready = ready && !error;
        self.is_error = error;
        self.error_animation_progress = if error { 1.0 } else { 0.0 };
    }

    pub fn count_available_width(&self, total_width: f32) -> f32 {
        (total_width - UI_ROW_PADDING_LEFT - UI_ROW_PADDING_RIGHT - UI_ROW_ICON_WIDTH - UI_ROW_ICON_SKIP).max(0.0)
    }

    pub fn resize_get_height(&self, total_width: f32) -> f32 {
        let avail_width = self.count_available_width(total_width);
        let title_lines = ((self.row_title.len() as f32 * 8.0) / avail_width.max(1.0)).ceil().max(1.0);
        let desc_lines = ((self.row_description.len() as f32 * 7.0) / avail_width.max(1.0)).ceil().max(1.0);
        let title_height = title_lines * 18.0;
        let desc_height = desc_lines * 16.0;

        UI_ROW_PADDING_TOP + title_height + UI_ROW_SKIP + desc_height + UI_ROW_PADDING_BOTTOM
    }
}
