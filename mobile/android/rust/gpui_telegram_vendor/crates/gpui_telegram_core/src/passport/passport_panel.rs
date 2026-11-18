//! Telegram Passport Dedicated Floating Window Panel
//! Upstream: `Telegram/SourceFiles/passport/passport_panel.h`, `passport_panel.cpp`
//! Subsystem: `passport`, Feature ID: `TCG-N02`

use serde::{Deserialize, Serialize};

pub const PANEL_DEFAULT_WIDTH: f32 = 392.0;
pub const PANEL_DEFAULT_HEIGHT: f32 = 600.0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PanelView {
    #[default]
    FormOverview,
    AskPassword,
    NoPassword,
    EditScope(usize),
    CriticalError(String),
}

/// Floating separate panel window coordinator for Telegram Passport authorizations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassportPanel {
    pub current_view: PanelView,
    pub back_allowed: bool,
    pub toast_messages: Vec<String>,
    pub panel_width: f32,
    pub panel_height: f32,
    pub is_closing: bool,
}

impl Default for PassportPanel {
    fn default() -> Self {
        Self {
            current_view: PanelView::FormOverview,
            back_allowed: false,
            toast_messages: Vec::new(),
            panel_width: PANEL_DEFAULT_WIDTH,
            panel_height: PANEL_DEFAULT_HEIGHT,
            is_closing: false,
        }
    }
}

impl PassportPanel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show_ask_password(&mut self) {
        self.current_view = PanelView::AskPassword;
        self.back_allowed = false;
    }

    pub fn show_no_password(&mut self) {
        self.current_view = PanelView::NoPassword;
        self.back_allowed = false;
    }

    pub fn show_form(&mut self) {
        self.current_view = PanelView::FormOverview;
        self.back_allowed = false;
    }

    pub fn show_critical_error(&mut self, error: impl Into<String>) {
        self.current_view = PanelView::CriticalError(error.into());
        self.back_allowed = false;
    }

    pub fn show_edit_scope(&mut self, index: usize) {
        self.current_view = PanelView::EditScope(index);
        self.back_allowed = true;
    }

    pub fn set_back_allowed(&mut self, allowed: bool) {
        self.back_allowed = allowed;
    }

    pub fn push_toast(&mut self, toast: impl Into<String>) {
        self.toast_messages.push(toast.into());
    }

    pub fn pop_toast(&mut self) -> Option<String> {
        self.toast_messages.pop()
    }

    pub fn close(&mut self) -> u32 {
        self.is_closing = true;
        // returns hide duration in milliseconds
        200
    }
}
