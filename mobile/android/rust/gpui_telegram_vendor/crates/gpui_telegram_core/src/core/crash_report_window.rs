//! Telegram Desktop Pre-Launch Crash Diagnostics & Report Window
//! Upstream sources: `Telegram/SourceFiles/core/crash_report_window.h`, `crash_report_window.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SendingState {
    #[default]
    NoReport,
    SendingUpdateCheck,
    Progress,
    Uploading,
    Fail,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UpdatingState {
    #[default]
    None,
    Check,
    Latest,
    Download,
    Fail,
    Ready,
}

/// Pre-launch diagnostic dialog allowing users to review, save, or transmit previous crash dumps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashReportWindow {
    pub dump_raw: Vec<u8>,
    pub report_text: String,
    pub include_username: bool,
    pub sending_state: SendingState,
    pub updating_state: UpdatingState,
    pub is_visible: bool,
    pub upload_progress_percent: f32,
}

impl Default for CrashReportWindow {
    fn default() -> Self {
        Self {
            dump_raw: Vec::new(),
            report_text: String::new(),
            include_username: false,
            sending_state: SendingState::NoReport,
            updating_state: UpdatingState::None,
            is_visible: false,
            upload_progress_percent: 0.0,
        }
    }
}

impl CrashReportWindow {
    pub fn new(dump: Vec<u8>) -> Self {
        let text = String::from_utf8_lossy(&dump).to_string();
        Self {
            dump_raw: dump,
            report_text: text,
            include_username: false,
            sending_state: SendingState::NoReport,
            updating_state: UpdatingState::None,
            is_visible: true,
            upload_progress_percent: 0.0,
        }
    }

    pub fn set_include_username(&mut self, include: bool) {
        self.include_username = include;
    }

    pub fn send_report(&mut self) {
        self.sending_state = SendingState::Uploading;
        self.upload_progress_percent = 50.0;
    }

    pub fn report_sent_successfully(&mut self) {
        self.sending_state = SendingState::Done;
        self.upload_progress_percent = 100.0;
    }

    pub fn report_send_failed(&mut self) {
        self.sending_state = SendingState::Fail;
    }

    pub fn save_report(&self, dest: &std::path::Path) -> std::io::Result<()> {
        std::fs::write(dest, &self.dump_raw)
    }

    pub fn dismiss(&mut self) {
        self.is_visible = false;
    }
}
