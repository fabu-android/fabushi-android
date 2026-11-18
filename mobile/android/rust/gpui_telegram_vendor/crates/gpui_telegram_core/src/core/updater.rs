//! Telegram Desktop 1:1 Parity Module: `Updater`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/_other/updater.h

use serde::{Deserialize, Serialize};

pub const UPDATER_VERSION: u32 = 1000;
pub const UPDATER_VERSION_STR: &str = "0.1.0";

/// Standalone updater bootstrap process manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Updater {
    pub version: u32,
    pub version_str: String,
    pub target_process_id: Option<u32>,
}

impl Default for Updater {
    fn default() -> Self {
        Self {
            version: UPDATER_VERSION,
            version_str: UPDATER_VERSION_STR.to_string(),
            target_process_id: None,
        }
    }
}

impl Updater {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn format_update_error(message: &str, error_code: u32) -> String {
        format!("{} (error code: {})", message, error_code)
    }

    pub fn parse_update_process(id_str: &str, created_str: &str) -> Option<(u32, u64)> {
        let pid = id_str.trim().parse::<u32>().ok()?;
        let created = created_str.trim().parse::<u64>().ok().unwrap_or(0);
        Some((pid, created))
    }

    pub fn finish_update_process(process_id: u32) -> bool {
        process_id > 0
    }
}
