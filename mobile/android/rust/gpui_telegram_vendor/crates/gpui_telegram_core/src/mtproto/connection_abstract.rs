//! Abstract Connection Interface & MTProto Packet Framing
//! Upstream Source: `Telegram/SourceFiles/mtproto/connection_abstract.h`, `connection_abstract.cpp`

use crate::mtproto::core_types::MsgId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Suspended,
    Reconnecting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TransportProtocol {
    #[default]
    Abridged,
    Intermediate,
    PaddedIntermediate,
    Http,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionAbstract {
    pub state: ConnectionState,
    pub protocol: TransportProtocol,
    pub target_ip: String,
    pub target_port: u16,
    pub protocol_dc_id: i16,
    pub sent_encrypted_with_key_id: u64,
    pub ping_time_ms: u64,
    pub full_connect_timeout_ms: u64,
    pub received_queue: Vec<Vec<u8>>,
}

impl Default for ConnectionAbstract {
    fn default() -> Self {
        Self {
            state: ConnectionState::Disconnected,
            protocol: TransportProtocol::Intermediate,
            target_ip: String::new(),
            target_port: 443,
            protocol_dc_id: 2,
            sent_encrypted_with_key_id: 0,
            ping_time_ms: 0,
            full_connect_timeout_ms: 15_000,
            received_queue: Vec::new(),
        }
    }
}

impl ConnectionAbstract {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    pub fn connect_to_server(&mut self, ip: impl Into<String>, port: u16, dc_id: i16) {
        self.target_ip = ip.into();
        self.target_port = port;
        self.protocol_dc_id = dc_id;
        self.state = ConnectionState::Connecting;
    }

    pub fn mark_connected(&mut self) {
        self.state = ConnectionState::Connected;
    }

    pub fn disconnect_from_server(&mut self) {
        self.state = ConnectionState::Disconnected;
        self.received_queue.clear();
    }

    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    pub fn debug_state(&self) -> i32 {
        match self.state {
            ConnectionState::Disconnected => 0,
            ConnectionState::Connecting => 1,
            ConnectionState::Connected => 2,
            ConnectionState::Suspended => 3,
            ConnectionState::Reconnecting => 4,
        }
    }

    pub fn transport_name(&self) -> &'static str {
        match self.protocol {
            TransportProtocol::Abridged => "TCP-Abridged",
            TransportProtocol::Intermediate => "TCP-Intermediate",
            TransportProtocol::PaddedIntermediate => "TCP-PaddedIntermediate",
            TransportProtocol::Http => "HTTP-LongPoll",
        }
    }

    /// Formats an unencrypted MTProto packet:
    /// auth_key_id: 0u64 (8 bytes)
    /// msg_id: u64 (8 bytes)
    /// length: u32 (4 bytes)
    /// payload: bytes
    pub fn prepare_not_secure_packet(&self, payload: &[u8], msg_id: MsgId) -> Vec<u8> {
        let mut packet = Vec::with_capacity(20 + payload.len());
        packet.extend_from_slice(&0u64.to_le_bytes());
        packet.extend_from_slice(&msg_id.to_le_bytes());
        packet.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        packet.extend_from_slice(payload);
        packet
    }

    /// Parses an unencrypted MTProto response:
    /// Returns (msg_id, payload)
    pub fn parse_not_secure_response(&self, buffer: &[u8]) -> Option<(MsgId, Vec<u8>)> {
        if buffer.len() < 20 {
            return None;
        }
        let auth_key_id = u64::from_le_bytes(buffer[0..8].try_into().ok()?);
        if auth_key_id != 0 {
            return None;
        }
        let msg_id = u64::from_le_bytes(buffer[8..16].try_into().ok()?);
        let len = u32::from_le_bytes(buffer[16..20].try_into().ok()?) as usize;
        if buffer.len() < 20 + len {
            return None;
        }
        Some((msg_id, buffer[20..20 + len].to_vec()))
    }

    /// Encapsulates an encrypted payload:
    /// auth_key_id (8 bytes) + msg_key (16 bytes) + encrypted_data
    pub fn prepare_secure_packet(&mut self, key_id: u64, msg_key: &[u8; 16], encrypted_data: &[u8]) -> Vec<u8> {
        self.sent_encrypted_with_key_id = key_id;
        let mut packet = Vec::with_capacity(24 + encrypted_data.len());
        packet.extend_from_slice(&key_id.to_le_bytes());
        packet.extend_from_slice(msg_key);
        packet.extend_from_slice(encrypted_data);
        packet
    }
}
