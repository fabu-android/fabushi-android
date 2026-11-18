//! Telegram Desktop 1:1 Parity Module: `StartupTaskWin`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/_other/startup_task_win.cpp

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const WINDOWS_AUTOSTART_ARG: &str = "-autostart";
pub const WINDOWS_EXECUTABLE_NAME: &str = "Telegram.exe";

/// Windows scheduled task / startup registry executor configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupTaskWin {
    pub autostart_enabled: bool,
    pub executable_dir: PathBuf,
}

impl Default for StartupTaskWin {
    fn default() -> Self {
        Self {
            autostart_enabled: false,
            executable_dir: std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .unwrap_or_else(|| PathBuf::from(".")),
        }
    }
}

impl StartupTaskWin {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn target_executable_path(&self) -> PathBuf {
        self.executable_dir.join(WINDOWS_EXECUTABLE_NAME)
    }

    pub fn autostart_command_line(&self) -> String {
        format!(
            "\"{}\" {}",
            self.target_executable_path().to_string_lossy(),
            WINDOWS_AUTOSTART_ARG
        )
    }

    pub fn is_autostart_argument(arg: &str) -> bool {
        arg.eq_ignore_ascii_case(WINDOWS_AUTOSTART_ARG)
    }
}
