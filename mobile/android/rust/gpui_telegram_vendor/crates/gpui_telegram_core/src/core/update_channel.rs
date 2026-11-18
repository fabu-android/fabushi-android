//! Telegram Desktop 1:1 Parity Module: `UpdateChannel`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/update_channel.h

use serde::{Deserialize, Serialize};

/// Update release channel selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum UpdateChannel {
    #[default]
    Stable = 0,
    Beta = 1,
    CanaryPublic = 2,
    CanaryPrivate = 3,
}

impl UpdateChannel {
    pub fn is_canary(&self) -> bool {
        matches!(self, Self::CanaryPublic | Self::CanaryPrivate)
    }

    pub fn channel_name(&self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::CanaryPublic => "canary_public",
            Self::CanaryPrivate => "canary_private",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "stable" => Some(Self::Stable),
            "beta" => Some(Self::Beta),
            "canary_public" | "canary" => Some(Self::CanaryPublic),
            "canary_private" => Some(Self::CanaryPrivate),
            _ => None,
        }
    }

    pub fn title_label(&self, canary_counter: u32) -> String {
        match self {
            Self::CanaryPrivate => format!("PRIVATE #{}", canary_counter),
            Self::CanaryPublic => format!("CANARY #{}", canary_counter),
            _ => String::new(),
        }
    }

    /// Combines base version (e.g. 7003003) and canary counter into 64-bit monotonic update version.
    pub fn make_update_version(base: u32, counter: u32) -> u64 {
        ((base as u64) << 32) | (counter as u64)
    }

    pub fn update_version_base(version: u64) -> u32 {
        (version >> 32) as u32
    }

    pub fn update_version_counter(version: u64) -> u32 {
        (version & 0xFFFFFFFF) as u32
    }
}
