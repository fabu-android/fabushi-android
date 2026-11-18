//! Telegram Desktop Core Application Settings
//! Upstream sources: `Telegram/SourceFiles/core/core_settings.h`, `core_settings.cpp`

use crate::core::core_settings_proxy::CoreSettingsProxy;
use serde::{Deserialize, Serialize};

pub const DEFAULT_VOLUME: f64 = 0.9;
pub const RECENT_EMOJI_LIMIT: usize = 54;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ScreenCorner {
    TopLeft,
    TopRight,
    #[default]
    BottomRight,
    BottomLeft,
}

impl ScreenCorner {
    pub fn is_left(self) -> bool {
        matches!(self, ScreenCorner::TopLeft | ScreenCorner::BottomLeft)
    }

    pub fn is_top(self) -> bool {
        matches!(self, ScreenCorner::TopLeft | ScreenCorner::TopRight)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NotifyView {
    #[default]
    ShowPreview,
    ShowName,
    ShowNothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WorkMode {
    #[default]
    WindowAndTray,
    TrayOnly,
    WindowOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CloseBehavior {
    #[default]
    Quit,
    CloseToTaskbar,
    RunInBackground,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowPosition {
    pub moncrc: i32,
    pub maximized: bool,
    pub scale: i32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Default for WindowPosition {
    fn default() -> Self {
        Self {
            moncrc: 0,
            maximized: false,
            scale: 100,
            x: 100,
            y: 100,
            w: 960,
            h: 680,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WindowTitleContent {
    pub hide_chat_name: bool,
    pub hide_account_name: bool,
    pub hide_total_unread: bool,
}

/// Global Telegram settings container storing persistent user preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreSettings {
    pub window_position: WindowPosition,
    pub title_content: WindowTitleContent,
    pub screen_corner: ScreenCorner,
    pub notify_view: NotifyView,
    pub work_mode: WorkMode,
    pub close_behavior: CloseBehavior,

    // Audio & Video
    pub song_volume: f64,
    pub video_volume: f64,
    pub ask_download_path: bool,
    pub download_path: String,
    pub playback_device_id: String,
    pub capture_device_id: String,
    pub camera_device_id: String,

    // Notifications
    pub sound_notify: bool,
    pub desktop_notify: bool,
    pub flash_bounce_notify: bool,
    pub native_notifications: bool,
    pub notifications_count: u32,
    pub include_muted_counter: bool,
    pub include_muted_counter_folders: bool,
    pub count_unread_messages: bool,
    pub notify_about_pinned: bool,

    // Security & Locks
    pub auto_lock: i32, // seconds, 0 = disabled
    pub moderate_mode_enabled: bool,
    pub adaptive_for_wide: bool,

    // Proxy
    pub proxy: CoreSettingsProxy,
}

impl Default for CoreSettings {
    fn default() -> Self {
        Self {
            window_position: WindowPosition::default(),
            title_content: WindowTitleContent::default(),
            screen_corner: ScreenCorner::BottomRight,
            notify_view: NotifyView::ShowPreview,
            work_mode: WorkMode::WindowAndTray,
            close_behavior: CloseBehavior::Quit,

            song_volume: DEFAULT_VOLUME,
            video_volume: DEFAULT_VOLUME,
            ask_download_path: false,
            download_path: String::new(),
            playback_device_id: "default".to_string(),
            capture_device_id: "default".to_string(),
            camera_device_id: "default".to_string(),

            sound_notify: true,
            desktop_notify: true,
            flash_bounce_notify: true,
            native_notifications: true,
            notifications_count: 3,
            include_muted_counter: false,
            include_muted_counter_folders: false,
            count_unread_messages: false,
            notify_about_pinned: true,

            auto_lock: 3600,
            moderate_mode_enabled: false,
            adaptive_for_wide: true,

            proxy: CoreSettingsProxy::default(),
        }
    }
}

impl CoreSettings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_song_volume(&mut self, volume: f64) {
        self.song_volume = volume.clamp(0.0, 1.0);
    }

    pub fn set_video_volume(&mut self, volume: f64) {
        self.video_volume = volume.clamp(0.0, 1.0);
    }

    pub fn set_download_path(&mut self, path: impl Into<String>) {
        self.download_path = path.into();
    }

    pub fn set_auto_lock(&mut self, seconds: i32) {
        self.auto_lock = seconds.max(0);
    }

    pub fn serialize(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    pub fn from_serialized(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| e.to_string())
    }
}
