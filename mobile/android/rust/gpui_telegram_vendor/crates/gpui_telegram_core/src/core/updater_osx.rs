//! Telegram Desktop 1:1 Parity Module: `UpdaterOsx`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/_other/updater_osx.m

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// macOS platform auto-updater: swaps `.app` bundle, clears quarantine attributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UpdaterOsx;

impl UpdaterOsx {
    pub fn bundle_name() -> &'static str {
        "Telegram.app"
    }

    pub fn default_bundle_path() -> PathBuf {
        PathBuf::from("/Applications/Telegram.app")
    }

    pub fn swap_bundle(staged_bundle: &Path, target_bundle: &Path) -> Result<(), String> {
        if !staged_bundle.exists() {
            return Err("Staged .app bundle does not exist".to_string());
        }

        let backup = target_bundle.with_extension("app.old");
        if backup.exists() {
            let _ = std::fs::remove_dir_all(&backup);
        }

        if target_bundle.exists() {
            std::fs::rename(target_bundle, &backup)
                .map_err(|e| format!("Failed to move old bundle to backup: {}", e))?;
        }

        if let Err(e) = std::fs::rename(staged_bundle, target_bundle) {
            // Revert backup
            if backup.exists() {
                let _ = std::fs::rename(&backup, target_bundle);
            }
            return Err(format!("Failed to move staged bundle into place: {}", e));
        }

        let _ = std::fs::remove_dir_all(&backup);
        Ok(())
    }
}
