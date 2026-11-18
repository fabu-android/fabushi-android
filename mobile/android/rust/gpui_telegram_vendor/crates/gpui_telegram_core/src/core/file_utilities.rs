//! Telegram Desktop 1:1 Parity Module: `FileUtilities`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/file_utilities.h, Telegram/SourceFiles/core/file_utilities.cpp

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Result from opening files via platform file dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FileDialogOpenResult {
    pub paths: Vec<PathBuf>,
    pub remote_content: Vec<u8>,
}

/// Type of file dialog request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FileDialogType {
    #[default]
    ReadFile,
    ReadFiles,
    ReadFolder,
    WriteFile,
}

/// Standard file filters for file chooser dialogues.
pub struct FileFilters;

impl FileFilters {
    pub fn all_files() -> &'static str {
        "All Files (*.*)"
    }

    pub fn images() -> &'static str {
        "Image Files (*.jpg *.jpeg *.png *.webp *.gif *.bmp *.svg)"
    }

    pub fn photo_video() -> &'static str {
        "Photo and Video Files (*.jpg *.jpeg *.png *.webp *.gif *.mp4 *.mov *.avi *.mkv *.webm)"
    }

    pub fn audio() -> &'static str {
        "Audio Files (*.mp3 *.m4a *.ogg *.opus *.flac *.wav *.wma *.aac)"
    }
}

/// Core file utilities service providing path sanitization, default download paths,
/// collision-avoiding filename generator, and sandboxed file/URL actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileUtilities {
    pub default_download_folder_name: String,
    pub custom_download_directory: Option<PathBuf>,
    pub last_file_dialog_path: PathBuf,
    pub ask_download_path_each_time: bool,
}

impl Default for FileUtilities {
    fn default() -> Self {
        Self {
            default_download_folder_name: "Telegram Desktop".to_string(),
            custom_download_directory: None,
            last_file_dialog_path: dirs_fallback_download_dir(),
            ask_download_path_each_time: false,
        }
    }
}

impl FileUtilities {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_download_folder(mut self, folder: impl Into<String>) -> Self {
        self.default_download_folder_name = folder.into();
        self
    }

    /// Resolves the default download destination directory.
    pub fn default_download_path(&self) -> PathBuf {
        if let Some(ref custom) = self.custom_download_directory {
            return custom.clone();
        }
        dirs_fallback_download_dir().join(&self.default_download_folder_name)
    }

    /// Sets last navigated path in file dialog.
    pub fn set_last_path(&mut self, path: PathBuf) {
        self.last_file_dialog_path = path;
    }

    /// Strips sensitive web login / bot authorization tokens from URL before
    /// dispatching to external browser (prevents session hijacking via shared links).
    pub fn url_without_web_auth_tokens(url: &str) -> String {
        if let Some(pos) = url.find('?') {
            let base = &url[..pos];
            let query = &url[pos + 1..];
            let filtered_params: Vec<&str> = query
                .split('&')
                .filter(|param| {
                    let key = param.split('=').next().unwrap_or("").to_ascii_lowercase();
                    !matches!(key.as_str(), "auth" | "token" | "tgauthtoken" | "webauthtoken")
                })
                .collect();

            if filtered_params.is_empty() {
                base.to_string()
            } else {
                format!("{}?{}", base, filtered_params.join("&"))
            }
        } else {
            url.to_string()
        }
    }

    /// Generates next non-colliding filename if `target` already exists on disk.
    /// E.g. "photo.jpg" -> "photo (2).jpg", "photo (3).jpg".
    pub fn filedialog_next_filename(name: &str, current: &str, dir: &Path) -> PathBuf {
        let (prefix, extension) = match name.rfind('.') {
            Some(idx) => (&name[..idx], &name[idx..]),
            None => (name, ""),
        };

        let mut candidate = dir.join(name);
        let mut counter = 2;
        while candidate.exists() && candidate.to_string_lossy().to_lowercase() != current.to_lowercase() {
            candidate = dir.join(format!("{} ({}){}", prefix, counter, extension));
            counter += 1;
        }
        candidate
    }

    /// Generates a timestamped default filename for downloads, e.g. "prefix_2026-11-06_17-00-00.jpg".
    pub fn filedialog_default_name(
        prefix: &str,
        extension: &str,
        dir: &Path,
        skip_existence: bool,
        unix_time: Option<i64>,
    ) -> PathBuf {
        let timestamp_str = match unix_time {
            Some(ts) => {
                let dt = chrono::DateTime::from_timestamp(ts, 0)
                    .unwrap_or_else(|| chrono::Utc::now());
                dt.format("_%Y-%m-%d_%H-%M-%S").to_string()
            }
            None => {
                chrono::Utc::now().format("_%Y-%m-%d_%H-%M-%S").to_string()
            }
        };

        let ext = if extension.starts_with('.') || extension.is_empty() {
            extension.to_string()
        } else {
            format!(".{}", extension)
        };

        let base_name = format!("{}{}{}", prefix, timestamp_str, ext);
        if skip_existence {
            dir.join(base_name)
        } else {
            Self::filedialog_next_filename(&base_name, "", dir)
        }
    }

    /// Safe launch check: verifies file exists and is not an executable script before opening.
    pub fn is_safe_to_launch(path: &Path) -> bool {
        if !path.exists() {
            return false;
        }
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        !matches!(
            ext.as_str(),
            "exe" | "bat" | "cmd" | "com" | "vbs" | "ps1" | "sh" | "command" | "app"
        )
    }
}

fn dirs_fallback_download_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            return PathBuf::from(profile).join("Downloads");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Downloads");
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Downloads");
        }
    }
    std::env::temp_dir()
}
