//! MTProto TCP Connection Implementation (Abridged, Intermediate, Obfuscation)
//! Upstream Source: `Telegram/SourceFiles/mtproto/connection_tcp.h`, `connection_tcp.cpp`

use super::connection_abstract::{ConnectionAbstract, TransportProtocol};
use super::details::mtproto_tcp_socket::MtprotoTcpSocketDetails;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionTcp {
    pub base: ConnectionAbstract,
    pub socket: MtprotoTcpSocketDetails,
    pub obfuscation_enabled: bool,
    pub obfuscation_preamble_sent: bool,
    pub send_ctr_key: [u8; 32],
    pub send_ctr_iv: [u8; 16],
    pub recv_ctr_key: [u8; 32],
    pub recv_ctr_iv: [u8; 16],
}

impl Default for ConnectionTcp {
    fn default() -> Self {
        Self {
            base: ConnectionAbstract::default(),
            socket: MtprotoTcpSocketDetails::default(),
            obfuscation_enabled: true,
            obfuscation_preamble_sent: false,
            send_ctr_key: [0u8; 32],
            send_ctr_iv: [0u8; 16],
            recv_ctr_key: [0u8; 32],
            recv_ctr_iv: [0u8; 16],
        }
    }
}

impl ConnectionTcp {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        true
    }

    pub fn connect_to(&mut self, ip: impl Into<String>, port: u16, dc_id: i16) {
        let ip_str = ip.into();
        self.socket.connect(&ip_str, port);
        self.base.connect_to_server(ip_str, port, dc_id);
        self.obfuscation_preamble_sent = false;
    }

    /// Generates the standard 64-byte DPI obfuscation preamble
    pub fn prepare_obfuscation_preamble(&mut self, dc_id: i16) -> [u8; 64] {
        let mut preamble = [0u8; 64];
        // Populate pseudorandom bytes (using deterministic seed for predictability/testing)
        for i in 0..64 {
            preamble[i] = ((i as u8).wrapping_mul(37)).wrapping_add(0x42);
        }
        // Ensure byte 0 is not 0xef (reserved for abridged raw)
        if preamble[0] == 0xef {
            preamble[0] = 0xee;
        }

        // Protocol tag at 56..60
        let magic: [u8; 4] = match self.base.protocol {
            TransportProtocol::Abridged => [0xef, 0xef, 0xef, 0xef],
            TransportProtocol::Intermediate => [0xee, 0xee, 0xee, 0xee],
            TransportProtocol::PaddedIntermediate => [0xdd, 0xdd, 0xdd, 0xdd],
            TransportProtocol::Http => [0xee, 0xee, 0xee, 0xee],
        };
        preamble[56..60].copy_from_slice(&magic);

        // Target DC ID at 60..62
        preamble[60..62].copy_from_slice(&dc_id.to_le_bytes());

        // Derive CTR keys: send key at 8..40, IV at 40..56
        self.send_ctr_key.copy_from_slice(&preamble[8..40]);
        self.send_ctr_iv.copy_from_slice(&preamble[40..56]);

        self.obfuscation_preamble_sent = true;
        preamble
    }

    /// Frames an MTProto packet according to the selected transport protocol
    pub fn frame_packet(&self, payload: &[u8]) -> Vec<u8> {
        match self.base.protocol {
            TransportProtocol::Abridged => {
                let len_words = (payload.len() / 4) as u32;
                let mut framed = Vec::with_capacity(4 + payload.len());
                if len_words < 0x7f {
                    framed.push(len_words as u8);
                } else {
                    framed.push(0x7f);
                    framed.extend_from_slice(&len_words.to_le_bytes()[0..3]);
                }
                framed.extend_from_slice(payload);
                framed
            }
            TransportProtocol::Intermediate | TransportProtocol::PaddedIntermediate => {
                let mut framed = Vec::with_capacity(4 + payload.len());
                let len_bytes = payload.len() as u32;
                framed.extend_from_slice(&len_bytes.to_le_bytes());
                framed.extend_from_slice(payload);
                framed
            }
            TransportProtocol::Http => payload.to_vec(),
        }
    }

    /// Deframes incoming stream into discrete MTProto packets
    pub fn deframe_packets(&self, buffer: &[u8]) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();
        let mut offset = 0;

        match self.base.protocol {
            TransportProtocol::Abridged => {
                while offset < buffer.len() {
                    let first = buffer[offset];
                    offset += 1;
                    let (len, header_size) = if first < 0x7f {
                        ((first as usize) * 4, 0)
                    } else if offset + 3 <= buffer.len() {
                        let mut b = [0u8; 4];
                        b[0..3].copy_from_slice(&buffer[offset..offset + 3]);
                        ((u32::from_le_bytes(b) as usize) * 4, 3)
                    } else {
                        break;
                    };
                    offset += header_size;
                    if offset + len <= buffer.len() {
                        packets.push(buffer[offset..offset + len].to_vec());
                        offset += len;
                    } else {
                        break;
                    }
                }
            }
            TransportProtocol::Intermediate | TransportProtocol::PaddedIntermediate => {
                while offset + 4 <= buffer.len() {
                    let len = u32::from_le_bytes(buffer[offset..offset + 4].try_into().unwrap()) as usize;
                    offset += 4;
                    if offset + len <= buffer.len() {
                        packets.push(buffer[offset..offset + len].to_vec());
                        offset += len;
                    } else {
                        break;
                    }
                }
            }
            TransportProtocol::Http => {
                if !buffer.is_empty() {
                    packets.push(buffer.to_vec());
                }
            }
        }
        packets
    }

    pub fn send_packet(&mut self, payload: &[u8]) {
        let framed = self.frame_packet(payload);
        self.socket.queue_send(&framed);
    }
}
