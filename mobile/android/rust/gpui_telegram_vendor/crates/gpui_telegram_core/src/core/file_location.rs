//! Telegram Desktop 1:1 Parity Module: `FileLocation`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/file_location.h, Telegram/SourceFiles/core/file_location.cpp

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const IN_MEDIA_CACHE_LOCATION: &str = "*media_cache*";
pub const MAX_FILE_SIZE_BYTES: u64 = 4000 * 1024 * 1024; // 4000 MB limit

/// RAII helper enabling read access to security-scoped sandboxed bookmarks.
#[derive(Debug)]
pub struct ReadAccessEnabler {
    has_access: bool,
}

impl ReadAccessEnabler {
    pub fn new(bookmark: &[u8]) -> Self {
        Self {
            has_access: !bookmark.is_empty(),
        }
    }

    pub fn failed(&self) -> bool {
        !self.has_access
    }
}

impl Drop for ReadAccessEnabler {
    fn drop(&mut self) {
        self.has_access = false;
    }
}

/// Represents a validated local file reference or media cache sentinel,
/// tracking modification time, size, and sandbox access bookmark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileLocation {
    pub fname: String,
    pub modified: i64,
    pub size: u64,
    pub bookmark: Vec<u8>,
}

impl Default for FileLocation {
    fn default() -> Self {
        Self {
            fname: String::new(),
            modified: 0,
            size: 0,
            bookmark: Vec::new(),
        }
    }
}

impl FileLocation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_name(name: impl Into<String>) -> Self {
        let name_str = name.into();
        if name_str.is_empty() || name_str == IN_MEDIA_CACHE_LOCATION {
            Self {
                fname: name_str,
                modified: 0,
                size: 0,
                bookmark: Vec::new(),
            }
        } else {
            let path = Path::new(&name_str);
            Self::from_path(path)
        }
    }

    pub fn from_path(path: &Path) -> Self {
        let fname = path.to_string_lossy().to_string();
        if let Ok(metadata) = std::fs::metadata(path) {
            let size = metadata.len();
            if size > MAX_FILE_SIZE_BYTES {
                return Self::default();
            }
            let modified = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            Self {
                fname,
                modified,
                size,
                bookmark: Vec::new(),
            }
        } else {
            Self {
                fname,
                modified: 0,
                size: 0,
                bookmark: Vec::new(),
            }
        }
    }

    pub fn in_media_cache_location() -> Self {
        Self {
            fname: IN_MEDIA_CACHE_LOCATION.to_string(),
            modified: 0,
            size: 0,
            bookmark: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.fname.is_empty()
    }

    pub fn in_media_cache(&self) -> bool {
        self.fname == IN_MEDIA_CACHE_LOCATION
    }

    pub fn name(&self) -> &str {
        &self.fname
    }

    pub fn path(&self) -> PathBuf {
        PathBuf::from(&self.fname)
    }

    pub fn set_bookmark(&mut self, bookmark: Vec<u8>) {
        self.bookmark = bookmark;
    }

    pub fn bookmark(&self) -> &[u8] {
        &self.bookmark
    }

    pub fn access_enable(&self) -> bool {
        if self.is_empty() {
            false
        } else if self.bookmark.is_empty() {
            true
        } else {
            let enabler = ReadAccessEnabler::new(&self.bookmark);
            !enabler.failed()
        }
    }

    pub fn access_disable(&self) {}

    /// Validates existence, size parity, and modification timestamp match.
    pub fn check(&self) -> bool {
        if self.is_empty() || self.in_media_cache() {
            return false;
        }

        let path = Path::new(&self.fname);
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return false,
        };

        let current_size = metadata.len();
        if current_size > MAX_FILE_SIZE_BYTES || current_size != self.size {
            return false;
        }

        if let Ok(modified_time) = metadata.modified() {
            if let Ok(duration) = modified_time.duration_since(std::time::UNIX_EPOCH) {
                let current_mod = duration.as_secs() as i64;
                if self.modified != 0 && (current_mod - self.modified).abs() > 2 {
                    return false;
                }
            }
        }

        true
    }
}
