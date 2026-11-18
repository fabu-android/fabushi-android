//! MTProto TCP Socket Implementation
//! Upstream Source: `Telegram/SourceFiles/mtproto/details/mtproto_tcp_socket.h`, `mtproto_tcp_socket.cpp`

use super::mtproto_abstract_socket::MtprotoAbstractSocketDetails;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoTcpSocketDetails {
    pub base: MtprotoAbstractSocketDetails,
    pub tcp_nodelay: bool,
    pub send_buffer: Vec<u8>,
    pub receive_buffer: Vec<u8>,
}

impl Default for MtprotoTcpSocketDetails {
    fn default() -> Self {
        Self {
            base: MtprotoAbstractSocketDetails::default(),
            tcp_nodelay: true,
            send_buffer: Vec::new(),
            receive_buffer: Vec::new(),
        }
    }
}

impl MtprotoTcpSocketDetails {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.base.is_active()
    }

    pub fn connect(&mut self, address: impl Into<String>, port: u16) {
        self.base.connect_to(address, port);
        self.send_buffer.clear();
        self.receive_buffer.clear();
    }

    pub fn queue_send(&mut self, data: &[u8]) {
        self.send_buffer.extend_from_slice(data);
        self.base.record_write(data.len());
    }

    pub fn feed_received(&mut self, data: &[u8]) {
        self.receive_buffer.extend_from_slice(data);
        self.base.record_read(data.len());
    }

    pub fn take_received(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.receive_buffer)
    }

    pub fn close(&mut self) {
        self.base.close();
        self.send_buffer.clear();
        self.receive_buffer.clear();
    }
}
