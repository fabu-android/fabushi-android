//! Telegram Desktop Version Changelog Formatter & Cloud Update Notes
//! Upstream sources: `Telegram/SourceFiles/core/changelogs.h`, `changelogs.cpp`

use serde::{Deserialize, Serialize};

pub fn format_version_display(version: i32) -> String {
    let major = version / 1_000_000;
    let minor = (version % 1_000_000) / 1_000;
    let patch = version % 1_000;
    format!("{}.{}.{}", major, minor, patch)
}

pub fn format_version_precise(version: i32) -> String {
    format_version_display(version)
}

/// Release notes and changelog manager fetching local and cloud release notes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Changelogs {
    pub old_version: i32,
    pub local_logs: Vec<String>,
    pub cloud_logs: Vec<String>,
}

impl Default for Changelogs {
    fn default() -> Self {
        Self {
            old_version: 0,
            local_logs: Vec::new(),
            cloud_logs: Vec::new(),
        }
    }
}

impl Changelogs {
    pub fn new(old_version: i32) -> Self {
        Self {
            old_version,
            local_logs: Vec::new(),
            cloud_logs: Vec::new(),
        }
    }

    pub fn add_local_log(&mut self, note: impl Into<String>) {
        self.local_logs.push(note.into());
    }

    pub fn add_cloud_log(&mut self, note: impl Into<String>) {
        self.cloud_logs.push(note.into());
    }

    pub fn has_unseen_updates(&self, current_version: i32) -> bool {
        current_version > self.old_version
    }
}
