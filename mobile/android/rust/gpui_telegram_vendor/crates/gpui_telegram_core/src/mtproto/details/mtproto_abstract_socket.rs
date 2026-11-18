//! Abstract Socket Abstraction for MTProto Connections
//! Upstream Source: `Telegram/SourceFiles/mtproto/details/mtproto_abstract_socket.h`, `mtproto_abstract_socket.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SocketState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Closing,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoAbstractSocketDetails {
    pub state: SocketState,
    pub address: String,
    pub port: u16,
    pub bytes_read: usize,
    pub bytes_written: usize,
    pub error_code: i32,
}

impl Default for MtprotoAbstractSocketDetails {
    fn default() -> Self {
        Self {
            state: SocketState::Disconnected,
            address: String::new(),
            port: 0,
            bytes_read: 0,
            bytes_written: 0,
            error_code: 0,
        }
    }
}

impl MtprotoAbstractSocketDetails {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.state == SocketState::Connected
    }

    pub fn connect_to(&mut self, address: impl Into<String>, port: u16) {
        self.address = address.into();
        self.port = port;
        self.state = SocketState::Connecting;
    }

    pub fn mark_connected(&mut self) {
        self.state = SocketState::Connected;
        self.error_code = 0;
    }

    pub fn record_read(&mut self, len: usize) {
        self.bytes_read = self.bytes_read.saturating_add(len);
    }

    pub fn record_write(&mut self, len: usize) {
        self.bytes_written = self.bytes_written.saturating_add(len);
    }

    pub fn mark_error(&mut self, error_code: i32) {
        self.state = SocketState::Error;
        self.error_code = error_code;
    }

    pub fn close(&mut self) {
        self.state = SocketState::Disconnected;
    }

    pub fn is_connected(&self) -> bool {
        self.state == SocketState::Connected
    }
}
