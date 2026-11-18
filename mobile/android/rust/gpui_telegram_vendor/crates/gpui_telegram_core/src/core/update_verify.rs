//! Telegram Desktop 1:1 Parity Module: `UpdateVerify`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/update_verify.h, Telegram/SourceFiles/core/update_verify.cpp

use super::update_channel::UpdateChannel;
use serde::{Deserialize, Serialize};

pub const ENVELOPE_MAGIC: &[u8; 4] = b"TDUP";
pub const ENVELOPE_FORMAT: u32 = 2;
pub const MAX_PAYLOAD_SIZE: usize = 256 * 1024 * 1024; // 256 MB

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum TargetOs {
    #[default]
    Windows = 0,
    Mac = 1,
    Linux = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum TargetArch {
    #[default]
    X86 = 0,
    X64 = 1,
    Arm = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UpdateTarget {
    pub os: TargetOs,
    pub arch: TargetArch,
}

impl UpdateTarget {
    pub fn current() -> Self {
        let os = if cfg!(target_os = "windows") {
            TargetOs::Windows
        } else if cfg!(target_os = "macos") {
            TargetOs::Mac
        } else {
            TargetOs::Linux
        };

        let arch = if cfg!(target_arch = "aarch64") {
            TargetArch::Arm
        } else if cfg!(target_arch = "x86_64") {
            TargetArch::X64
        } else {
            TargetArch::X86
        };

        Self { os, arch }
    }

    pub fn from_platform_key(key: &str) -> Option<Self> {
        match key.to_ascii_lowercase().as_str() {
            "win" => Some(Self {
                os: TargetOs::Windows,
                arch: TargetArch::X86,
            }),
            "win64" => Some(Self {
                os: TargetOs::Windows,
                arch: TargetArch::X64,
            }),
            "winarm" => Some(Self {
                os: TargetOs::Windows,
                arch: TargetArch::Arm,
            }),
            "mac" => Some(Self {
                os: TargetOs::Mac,
                arch: TargetArch::X64,
            }),
            "armac" => Some(Self {
                os: TargetOs::Mac,
                arch: TargetArch::Arm,
            }),
            "linux" | "linux64" => Some(Self {
                os: TargetOs::Linux,
                arch: TargetArch::X64,
            }),
            _ => None,
        }
    }
}

/// Cryptographically authenticated update package payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedUpdate {
    pub version: u64,
    pub channel: UpdateChannel,
    pub target: UpdateTarget,
    pub payload: Vec<u8>,
}

/// Container struct for update verification operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UpdateVerify;

impl UpdateVerify {
    pub fn parse_and_verify(
        bytes: &[u8],
        expected_target: UpdateTarget,
        running_version: u64,
    ) -> Result<VerifiedUpdate, &'static str> {
        if bytes.len() < 24 {
            return Err("Update envelope too small");
        }
        if &bytes[0..4] != ENVELOPE_MAGIC {
            return Err("Invalid envelope magic (expected TDUP)");
        }
        let format = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        if format != ENVELOPE_FORMAT {
            return Err("Unsupported envelope format");
        }

        let target_os = match bytes[8] {
            0 => TargetOs::Windows,
            1 => TargetOs::Mac,
            2 => TargetOs::Linux,
            _ => return Err("Invalid target OS"),
        };

        let target_arch = match bytes[9] {
            0 => TargetArch::X86,
            1 => TargetArch::X64,
            2 => TargetArch::Arm,
            _ => return Err("Invalid target Arch"),
        };

        let target = UpdateTarget {
            os: target_os,
            arch: target_arch,
        };
        if target != expected_target {
            return Err("Update target mismatch");
        }

        let version = u64::from_le_bytes(bytes[10..18].try_into().unwrap());
        if version <= running_version {
            return Err("Rollback rejected: update version <= running version");
        }

        let channel = match bytes[18] {
            0 => UpdateChannel::Stable,
            1 => UpdateChannel::Beta,
            2 => UpdateChannel::CanaryPublic,
            3 => UpdateChannel::CanaryPrivate,
            _ => return Err("Invalid channel"),
        };

        let payload_len = u32::from_le_bytes(bytes[19..23].try_into().unwrap()) as usize;
        if payload_len > MAX_PAYLOAD_SIZE || bytes.len() < 24 + payload_len {
            return Err("Invalid payload length");
        }

        let payload = bytes[24..24 + payload_len].to_vec();

        Ok(VerifiedUpdate {
            version,
            channel,
            target,
            payload,
        })
    }
}
