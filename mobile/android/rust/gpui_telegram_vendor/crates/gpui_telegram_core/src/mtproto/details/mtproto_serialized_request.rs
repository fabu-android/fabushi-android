//! Serialized Request Container & Wire Payload Header Formatting
//! Upstream Source: `Telegram/SourceFiles/mtproto/details/mtproto_serialized_request.h`, `mtproto_serialized_request.cpp`

use crate::mtproto::core_types::{MsgId, RequestId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoSerializedRequestDetails {
    pub request_id: RequestId,
    pub msg_id: MsgId,
    pub seq_no: u32,
    pub needs_layer: bool,
    pub force_send_in_container: bool,
    pub last_sent_time: i64,
    pub payload: Vec<u8>,
}

impl Default for MtprotoSerializedRequestDetails {
    fn default() -> Self {
        Self {
            request_id: 0,
            msg_id: 0,
            seq_no: 0,
            needs_layer: false,
            force_send_in_container: false,
            last_sent_time: 0,
            payload: Vec::new(),
        }
    }
}

impl MtprotoSerializedRequestDetails {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        !self.payload.is_empty() || self.request_id != 0
    }

    pub fn from_payload(request_id: RequestId, payload: Vec<u8>) -> Self {
        Self {
            request_id,
            msg_id: 0,
            seq_no: 0,
            needs_layer: false,
            force_send_in_container: false,
            last_sent_time: 0,
            payload,
        }
    }

    pub fn set_msg_id(&mut self, msg_id: MsgId) {
        self.msg_id = msg_id;
    }

    pub fn get_msg_id(&self) -> MsgId {
        self.msg_id
    }

    pub fn set_seq_no(&mut self, seq_no: u32) {
        self.seq_no = seq_no;
    }

    pub fn get_seq_no(&self) -> u32 {
        self.seq_no
    }

    pub fn message_size(&self) -> usize {
        self.payload.len()
    }

    pub fn need_ack(&self) -> bool {
        // In MTProto, messages with odd seq_no require an ACK from the peer
        (self.seq_no & 1) != 0
    }

    /// Formats the unencrypted packet header:
    /// auth_key_id (8 bytes = 0) + msg_id (8 bytes) + length (4 bytes) + payload
    pub fn format_unencrypted(&self) -> Vec<u8> {
        let mut packet = Vec::with_capacity(20 + self.payload.len());
        packet.extend_from_slice(&0u64.to_le_bytes()); // auth_key_id = 0
        packet.extend_from_slice(&self.msg_id.to_le_bytes());
        packet.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        packet.extend_from_slice(&self.payload);
        packet
    }
}
