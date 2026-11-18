//! MTProto HTTP / HTTPS Long-Poll Transport
//! Upstream Source: `Telegram/SourceFiles/mtproto/connection_http.h`, `connection_http.cpp`

use super::connection_abstract::{ConnectionAbstract, TransportProtocol};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionHttp {
    pub base: ConnectionAbstract,
    pub http_url: String,
    pub using_http_wait: bool,
    pub pending_requests_count: usize,
    pub max_wait_ms: u64,
}

impl Default for ConnectionHttp {
    fn default() -> Self {
        let mut base = ConnectionAbstract::default();
        base.protocol = TransportProtocol::Http;
        Self {
            base,
            http_url: String::new(),
            using_http_wait: true,
            pending_requests_count: 0,
            max_wait_ms: 25_000,
        }
    }
}

impl ConnectionHttp {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        true
    }

    pub fn connect_http(&mut self, url: impl Into<String>, dc_id: i16) {
        let url_str = url.into();
        self.http_url = url_str.clone();
        self.base.connect_to_server(url_str, 443, dc_id);
    }

    pub fn post_packet(&mut self, packet: &[u8]) -> Vec<u8> {
        self.pending_requests_count = self.pending_requests_count.saturating_add(1);
        let mut framed = Vec::with_capacity(4 + packet.len());
        framed.extend_from_slice(&(packet.len() as u32).to_le_bytes());
        framed.extend_from_slice(packet);
        framed
    }

    pub fn need_http_wait(&self) -> bool {
        self.using_http_wait && self.pending_requests_count == 0
    }
}
