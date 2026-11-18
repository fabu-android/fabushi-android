//! Telegram Desktop Application Launcher & Command Line Parser
//! Upstream sources: `Telegram/SourceFiles/core/launcher.h`, `launcher.cpp`

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const OPTION_DEBUG_MODE: &str = "-debug";
pub const OPTION_START_IN_TRAY: &str = "-startintray";
pub const OPTION_SCALE: &str = "-scale";
pub const OPTION_WORKDIR: &str = "-workdir";
pub const OPTION_FRACTIONAL_SCALING: &str = "-fractional-scaling";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdaterLaunch {
    PerformUpdate,
    JustRelaunch,
}

/// Command-line argument processor, instance hasher, and working directory setup.
#[derive(Debug, Clone)]
pub struct Launcher {
    pub arguments: Vec<String>,
    pub initial_working_dir: PathBuf,
    pub custom_working_dir: Option<PathBuf>,
    pub debug_mode: bool,
    pub start_in_tray: bool,
    pub scale_percent: Option<u32>,
    pub installation_tag: u64,
}

impl Default for Launcher {
    fn default() -> Self {
        Self {
            arguments: Vec::new(),
            initial_working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            custom_working_dir: None,
            debug_mode: false,
            start_in_tray: false,
            scale_percent: None,
            installation_tag: 0,
        }
    }
}

impl Launcher {
    pub fn new(args: Vec<String>) -> Self {
        let mut launcher = Self::default();
        launcher.process_arguments(args);
        launcher
    }

    pub fn process_arguments(&mut self, args: Vec<String>) {
        self.arguments = args;
        let mut iter = self.arguments.iter();
        while let Some(arg) = iter.next() {
            if arg == OPTION_DEBUG_MODE {
                self.debug_mode = true;
            } else if arg == OPTION_START_IN_TRAY {
                self.start_in_tray = true;
            } else if arg == OPTION_WORKDIR {
                if let Some(dir) = iter.next() {
                    self.custom_working_dir = Some(PathBuf::from(dir));
                }
            } else if arg.starts_with("-scale=") {
                if let Ok(scale) = arg.trim_start_matches("-scale=").parse::<u32>() {
                    self.scale_percent = Some(scale);
                }
            }
        }
    }

    pub fn effective_working_dir(&self) -> &Path {
        if let Some(ref dir) = self.custom_working_dir {
            dir.as_path()
        } else {
            self.initial_working_dir.as_path()
        }
    }

    pub fn validate_custom_working_dir(&self) -> bool {
        if let Some(ref dir) = self.custom_working_dir {
            dir.exists() && dir.is_dir()
        } else {
            true
        }
    }

    pub fn instance_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.effective_working_dir().to_string_lossy().as_bytes());
        let result = hasher.finalize();
        hex::encode(result)
    }

    pub fn exec(&mut self) -> i32 {
        if !self.validate_custom_working_dir() {
            return 1;
        }
        0
    }
}

mod hex {
    pub fn encode(data: impl AsRef<[u8]>) -> String {
        data.as_ref().iter().map(|b| format!("{:02x}", b)).collect()
    }
}
