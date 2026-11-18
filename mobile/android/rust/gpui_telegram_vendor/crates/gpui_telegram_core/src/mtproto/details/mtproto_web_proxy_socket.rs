//! MTProto Web Proxy Socket Layer
//! Upstream Source: `Telegram/SourceFiles/mtproto/details/mtproto_web_proxy_socket.h`, `mtproto_web_proxy_socket.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoWebProxySocketDetails {
    pub endpoint_url: String,
    pub session_id: String,
    pub is_connected: bool,
    pub frames_sent: usize,
    pub frames_received: usize,
    pub queue: Vec<Vec<u8>>,
}

impl Default for MtprotoWebProxySocketDetails {
    fn default() -> Self {
        Self {
            endpoint_url: String::new(),
            session_id: String::new(),
            is_connected: false,
            frames_sent: 0,
            frames_received: 0,
            queue: Vec::new(),
        }
    }
}

impl MtprotoWebProxySocketDetails {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.is_connected && !self.endpoint_url.is_empty()
    }

    pub fn connect(&mut self, endpoint_url: impl Into<String>, session_id: impl Into<String>) {
        self.endpoint_url = endpoint_url.into();
        self.session_id = session_id.into();
        self.is_connected = true;
    }

    pub fn push_frame(&mut self, data: Vec<u8>) {
        self.queue.push(data);
        self.frames_sent = self.frames_sent.saturating_add(1);
    }

    pub fn pop_frame(&mut self) -> Option<Vec<u8>> {
        if let Some(frame) = self.queue.pop() {
            self.frames_received = self.frames_received.saturating_add(1);
            Some(frame)
        } else {
            None
        }
    }

    pub fn disconnect(&mut self) {
        self.is_connected = false;
        self.queue.clear();
    }
}
