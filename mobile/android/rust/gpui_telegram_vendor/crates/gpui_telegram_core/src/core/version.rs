//! Telegram Desktop Version Definitions & Semantic Constants
//! Upstream sources: `Telegram/SourceFiles/core/version.h`

use serde::{Deserialize, Serialize};

pub const APP_ID: &str = "{53F49750-6209-4FBF-9CA8-7A333C87D1ED}";
pub const APP_NAME: &str = "Telegram Desktop";
pub const APP_FILE: &str = "Telegram";
pub const APP_VERSION: u64 = 7003003;
pub const APP_VERSION_STR: &str = "7.3.3";
pub const APP_BETA_VERSION: bool = false;
pub const APP_ALPHA_VERSION: u64 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub is_beta: bool,
}

impl Default for Version {
    fn default() -> Self {
        Self {
            major: 7,
            minor: 3,
            patch: 3,
            is_beta: false,
        }
    }
}

impl Version {
    pub const CURRENT: Self = Self {
        major: 7,
        minor: 3,
        patch: 3,
        is_beta: false,
    };

    pub fn new(major: u32, minor: u32, patch: u32, is_beta: bool) -> Self {
        Self {
            major,
            minor,
            patch,
            is_beta,
        }
    }

    pub fn as_compact_u64(&self) -> u64 {
        (self.major as u64) * 1_000_000 + (self.minor as u64) * 1_000 + (self.patch as u64)
    }

    pub fn to_version_string(&self) -> String {
        if self.is_beta {
            format!("{}.{}.{} Beta", self.major, self.minor, self.patch)
        } else {
            format!("{}.{}.{}", self.major, self.minor, self.patch)
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let is_beta = s.to_lowercase().contains("beta");
        let clean = s.split_whitespace().next()?;
        let parts: Vec<&str> = clean.split('.').collect();
        if parts.len() < 2 {
            return None;
        }
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        let patch = if parts.len() >= 3 {
            parts[2].parse().ok()?
        } else {
            0
        };
        Some(Self {
            major,
            minor,
            patch,
            is_beta,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_version_string())
    }
}
