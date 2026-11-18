//! Telegram Passport Form Item Row Component
//! Upstream: `Telegram/SourceFiles/passport/passport_form_row.h`, `passport_form_row.cpp`
//! Subsystem: `passport`, Feature ID: `TCG-N02`

use serde::{Deserialize, Serialize};

pub const ROW_PADDING_LEFT: f32 = 22.0;
pub const ROW_PADDING_TOP: f32 = 8.0;
pub const ROW_PADDING_RIGHT: f32 = 25.0;
pub const ROW_PADDING_BOTTOM: f32 = 8.0;
pub const ROW_SKIP: f32 = 2.0;
pub const ROW_ICON_WIDTH: f32 = 24.0;
pub const ROW_ICON_SKIP: f32 = 10.0;

/// State for a single interactive row representing a required passport value or document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormRow {
    pub title: String,
    pub description: String,
    pub is_ready: bool,
    pub error_message: Option<String>,
}

impl FormRow {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            is_ready: false,
            error_message: None,
        }
    }

    pub fn set_ready(&mut self, ready: bool) {
        self.is_ready = ready;
        if ready {
            self.error_message = None;
        }
    }

    pub fn set_error(&mut self, error: Option<String>) {
        if error.is_some() {
            self.is_ready = false;
        }
        self.error_message = error;
    }

    pub fn count_available_width(&self, total_width: f32) -> f32 {
        (total_width - ROW_PADDING_LEFT - ROW_PADDING_RIGHT - ROW_ICON_WIDTH - ROW_ICON_SKIP).max(0.0)
    }

    pub fn resize_get_height(&self, total_width: f32) -> f32 {
        let avail_width = self.count_available_width(total_width);
        // Estimate line wrap heights based on standard character width ~8px
        let title_lines = ((self.title.len() as f32 * 8.0) / avail_width.max(1.0)).ceil().max(1.0);
        let desc_lines = ((self.description.len() as f32 * 7.0) / avail_width.max(1.0)).ceil().max(1.0);
        let title_height = title_lines * 18.0;
        let desc_height = desc_lines * 16.0;

        ROW_PADDING_TOP + title_height + ROW_SKIP + desc_height + ROW_PADDING_BOTTOM
    }
}

/// Primary domain entity representing a Telegram Passport Form Row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassportFormRow {
    pub inner: FormRow,
    pub value_identifier: String,
}

impl PassportFormRow {
    pub fn new(value_identifier: impl Into<String>, title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            inner: FormRow::new(title, description),
            value_identifier: value_identifier.into(),
        }
    }

    pub fn set_ready(&mut self, ready: bool) {
        self.inner.set_ready(ready);
    }

    pub fn set_error(&mut self, error: Option<String>) {
        self.inner.set_error(error);
    }

    pub fn is_ready(&self) -> bool {
        self.inner.is_ready
    }

    pub fn has_error(&self) -> bool {
        self.inner.error_message.is_some()
    }

    pub fn available_width(&self, total_width: f32) -> f32 {
        self.inner.count_available_width(total_width)
    }

    pub fn calculate_height(&self, total_width: f32) -> f32 {
        self.inner.resize_get_height(total_width)
    }
}
