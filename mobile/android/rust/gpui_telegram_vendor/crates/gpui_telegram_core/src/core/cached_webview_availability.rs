//! Platform Webview Engine Availability Cache
//! Upstream sources: `Telegram/SourceFiles/core/cached_webview_availability.h`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebviewEngine {
    WebKit,
    WebView2,
    WebKitGTK,
    None,
}

/// Cached platform webview availability for mini-apps and OAuth pages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedWebviewAvailability {
    pub is_available: bool,
    pub engine: WebviewEngine,
    pub description: String,
}

impl Default for CachedWebviewAvailability {
    fn default() -> Self {
        #[cfg(target_os = "macos")]
        let (engine, avail) = (WebviewEngine::WebKit, true);
        #[cfg(target_os = "windows")]
        let (engine, avail) = (WebviewEngine::WebView2, true);
        #[cfg(target_os = "linux")]
        let (engine, avail) = (WebviewEngine::WebKitGTK, true);
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        let (engine, avail) = (WebviewEngine::None, false);

        Self {
            is_available: avail,
            engine,
            description: format!("Platform default webview engine: {:?}", engine),
        }
    }
}

impl CachedWebviewAvailability {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn available(&self) -> bool {
        self.is_available
    }
}
