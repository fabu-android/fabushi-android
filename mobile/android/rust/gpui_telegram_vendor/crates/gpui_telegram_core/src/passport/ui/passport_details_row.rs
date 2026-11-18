//! Telegram Passport Details Input Row Component
//! Upstream: `Telegram/SourceFiles/passport/ui/passport_details_row.h`, `passport_details_row.cpp`
//! Subsystem: `passport`, Feature ID: `TCG-N02`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PanelDetailsType {
    #[default]
    Text,
    Postcode,
    Country,
    Date,
    Gender,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Gender {
    Male,
    Female,
}

/// Generic details row component supporting text, dates, countries and gender selector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassportDetailsRowUi {
    pub detail_type: PanelDetailsType,
    pub field_label: String,
    pub field_value: String,
    pub max_label_width: f32,
    pub error_text: Option<String>,
    pub is_focused: bool,
}

impl Default for PassportDetailsRowUi {
    fn default() -> Self {
        Self {
            detail_type: PanelDetailsType::Text,
            field_label: String::new(),
            field_value: String::new(),
            max_label_width: 120.0,
            error_text: None,
            is_focused: false,
        }
    }
}

impl PassportDetailsRowUi {
    pub fn new(detail_type: PanelDetailsType, label: impl Into<String>, max_label_width: f32) -> Self {
        Self {
            detail_type,
            field_label: label.into(),
            field_value: String::new(),
            max_label_width,
            error_text: None,
            is_focused: false,
        }
    }

    pub fn set_field_value(&mut self, val: impl Into<String>) {
        let text = val.into();
        self.field_value = match self.detail_type {
            PanelDetailsType::Date => Self::format_date_input(&text),
            _ => text,
        };
        self.error_text = None;
    }

    pub fn show_error(&mut self, error: impl Into<String>) {
        self.error_text = Some(error.into());
    }

    pub fn hide_error(&mut self) {
        self.error_text = None;
    }

    pub fn has_error(&self) -> bool {
        self.error_text.is_some()
    }

    /// Formats raw numeric inputs into standard Telegram Passport date pattern `DD.MM.YYYY`.
    pub fn format_date_input(raw: &str) -> String {
        let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).take(8).collect();
        let mut formatted = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i == 2 || i == 4 {
                formatted.push('.');
            }
            formatted.push(c);
        }
        formatted
    }

    pub fn calculate_height(&self) -> f32 {
        let base_height = match self.detail_type {
            PanelDetailsType::Date => 30.0,
            _ => 32.0,
        };
        let padding_vertical = 20.0;
        let error_height = if self.has_error() { 24.0 } else { 0.0 };
        base_height + padding_vertical + error_height
    }
}
