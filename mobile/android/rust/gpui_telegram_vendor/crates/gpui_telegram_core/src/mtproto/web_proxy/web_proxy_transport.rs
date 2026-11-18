//! Web Proxy Transport Multiplexer
//! Upstream Source: `Telegram/SourceFiles/mtproto/web_proxy/web_proxy_transport.h`, `web_proxy_transport.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebProxyTransportWebProxy {
    pub endpoint: String,
    pub is_connected: bool,
    pub bytes_sent: usize,
    pub bytes_received: usize,
    pub outgoing_queue: Vec<Vec<u8>>,
    pub incoming_queue: Vec<Vec<u8>>,
}

impl Default for WebProxyTransportWebProxy {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            is_connected: false,
            bytes_sent: 0,
            bytes_received: 0,
            outgoing_queue: Vec::new(),
            incoming_queue: Vec::new(),
        }
    }
}

impl WebProxyTransportWebProxy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.is_connected && !self.endpoint.is_empty()
    }

    pub fn connect(&mut self, endpoint: impl Into<String>) {
        self.endpoint = endpoint.into();
        self.is_connected = true;
    }

    pub fn send_packet(&mut self, packet: Vec<u8>) {
        self.bytes_sent = self.bytes_sent.saturating_add(packet.len());
        self.outgoing_queue.push(packet);
    }

    pub fn push_incoming(&mut self, packet: Vec<u8>) {
        self.bytes_received = self.bytes_received.saturating_add(packet.len());
        self.incoming_queue.push(packet);
    }

    pub fn pop_incoming(&mut self) -> Option<Vec<u8>> {
        self.incoming_queue.pop()
    }

    pub fn disconnect(&mut self) {
        self.is_connected = false;
        self.outgoing_queue.clear();
        self.incoming_queue.clear();
    }
}
