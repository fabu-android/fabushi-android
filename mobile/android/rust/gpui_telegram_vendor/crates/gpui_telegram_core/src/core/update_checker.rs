//! Telegram Desktop Update Checker & Background Downloader FSM
//! Upstream sources: `Telegram/SourceFiles/core/update_checker.h`, `update_checker.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UpdateState {
    #[default]
    None,
    Checking,
    Download,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UpdateProgress {
    pub ready_bytes: u64,
    pub total_bytes: u64,
}

/// Update checking state machine managing background version polling and downloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateChecker {
    pub state: UpdateState,
    pub progress: UpdateProgress,
    pub available_version: Option<String>,
    pub download_url: Option<String>,
    pub disabled: bool,
    pub last_check_time: i64,
}

impl Default for UpdateChecker {
    fn default() -> Self {
        Self {
            state: UpdateState::None,
            progress: UpdateProgress::default(),
            available_version: None,
            download_url: None,
            disabled: false,
            last_check_time: 0,
        }
    }
}

impl UpdateChecker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self, _force_wait: bool) {
        if self.disabled {
            return;
        }
        self.state = UpdateState::Checking;
        self.last_check_time = chrono::Utc::now().timestamp();
    }

    pub fn stop(&mut self) {
        self.state = UpdateState::None;
        self.progress = UpdateProgress::default();
    }

    pub fn test(&mut self) {
        self.state = UpdateState::Download;
        self.progress = UpdateProgress {
            ready_bytes: 500,
            total_bytes: 1000,
        };
    }

    pub fn set_download_progress(&mut self, ready: u64, total: u64) {
        self.state = UpdateState::Download;
        self.progress = UpdateProgress {
            ready_bytes: ready,
            total_bytes: total,
        };
    }

    pub fn set_ready(&mut self, version: impl Into<String>) {
        self.state = UpdateState::Ready;
        self.available_version = Some(version.into());
    }

    pub fn set_failed(&mut self) {
        self.state = UpdateState::Failed;
    }

    pub fn state(&self) -> UpdateState {
        self.state
    }

    pub fn percent(&self) -> f32 {
        if self.progress.total_bytes == 0 {
            0.0
        } else {
            (self.progress.ready_bytes as f32 / self.progress.total_bytes as f32) * 100.0
        }
    }

    pub fn is_ready(&self) -> bool {
        self.state == UpdateState::Ready
    }
}
