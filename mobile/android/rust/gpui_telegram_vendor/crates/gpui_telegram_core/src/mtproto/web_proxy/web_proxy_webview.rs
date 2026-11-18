//! Web Proxy WebView Platform Bridge
//! Upstream Source: `Telegram/SourceFiles/mtproto/web_proxy/web_proxy_webview.h`, `web_proxy_webview.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebProxyWebviewWebProxy {
    pub bridge_id: String,
    pub is_webview_available: bool,
    pub channel_open: bool,
    pub posted_messages_count: usize,
}

impl Default for WebProxyWebviewWebProxy {
    fn default() -> Self {
        Self {
            bridge_id: String::new(),
            is_webview_available: true,
            channel_open: false,
            posted_messages_count: 0,
        }
    }
}

impl WebProxyWebviewWebProxy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.is_webview_available && self.channel_open
    }

    pub fn init_bridge(&mut self, bridge_id: impl Into<String>) {
        self.bridge_id = bridge_id.into();
        self.channel_open = true;
    }

    pub fn post_message(&mut self, _message: &str) -> bool {
        if self.channel_open {
            self.posted_messages_count = self.posted_messages_count.saturating_add(1);
            true
        } else {
            false
        }
    }

    pub fn close_channel(&mut self) {
        self.channel_open = false;
    }
}
