//! MTProto Session Private State Machine & Protocol Sequence Tracking
//! Upstream Source: `Telegram/SourceFiles/mtproto/session_private.h`, `session_private.cpp`

use crate::mtproto::core_types::{MsgId, ShiftedDcId};
use crate::mtproto::details::mtproto_received_ids_manager::MtprotoReceivedIdsManagerDetails;
use crate::mtproto::connection_tcp::ConnectionTcp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentContainer {
    pub sent_time: i64,
    pub messages: Vec<MsgId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandleResult {
    Success,
    Ignored,
    RestartConnection,
    ResetSession,
    DestroyTemporaryKey,
    ParseError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPrivate {
    pub shifted_dc_id: ShiftedDcId,
    pub state: i32,
    pub key_id: u64,
    pub session_id: u64,
    pub session_salt: u64,
    pub messages_counter: u32,
    pub session_marked_as_started: bool,
    pub received_message_ids: MtprotoReceivedIdsManagerDetails,
    pub ack_request_data: Vec<MsgId>,
    pub resend_request_data: Vec<MsgId>,
    pub connection: Option<ConnectionTcp>,
}

impl Default for SessionPrivate {
    fn default() -> Self {
        Self {
            shifted_dc_id: 2,
            state: 0, // Disconnected
            key_id: 0,
            session_id: 0x1234_5678_90ab_cdef,
            session_salt: 0xfedc_ba09_8765_4321,
            messages_counter: 0,
            session_marked_as_started: false,
            received_message_ids: MtprotoReceivedIdsManagerDetails::default(),
            ack_request_data: Vec::new(),
            resend_request_data: Vec::new(),
            connection: None,
        }
    }
}

impl SessionPrivate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.session_id != 0 && self.session_salt != 0
    }

    pub fn with_dc(shifted_dc_id: ShiftedDcId, key_id: u64, session_id: u64, salt: u64) -> Self {
        Self {
            shifted_dc_id,
            state: 0,
            key_id,
            session_id,
            session_salt: salt,
            messages_counter: 0,
            session_marked_as_started: false,
            received_message_ids: MtprotoReceivedIdsManagerDetails::default(),
            ack_request_data: Vec::new(),
            resend_request_data: Vec::new(),
            connection: None,
        }
    }

    /// Monotonic sequence number calculation adhering to upstream MTProto specification:
    /// seq_no = need_ack ? ((messages_counter++) * 2 + 1) : (messages_counter * 2)
    pub fn next_request_seq_number(&mut self, need_ack: bool) -> u32 {
        if need_ack {
            let seq = self.messages_counter.wrapping_mul(2).wrapping_add(1);
            self.messages_counter = self.messages_counter.wrapping_add(1);
            seq
        } else {
            self.messages_counter.wrapping_mul(2)
        }
    }

    pub fn change_session_id(&mut self, new_session_id: u64) {
        self.session_id = new_session_id;
        self.messages_counter = 0;
        self.session_marked_as_started = false;
    }

    pub fn reset_session(&mut self) {
        self.messages_counter = 0;
        self.session_marked_as_started = false;
        self.received_message_ids.clear();
        self.ack_request_data.clear();
        self.resend_request_data.clear();
    }

    pub fn update_salt(&mut self, new_salt: u64) {
        self.session_salt = new_salt;
    }

    pub fn queue_ack(&mut self, msg_id: MsgId) {
        if !self.ack_request_data.contains(&msg_id) {
            self.ack_request_data.push(msg_id);
        }
    }

    pub fn drain_acks(&mut self) -> Vec<MsgId> {
        std::mem::take(&mut self.ack_request_data)
    }

    pub fn set_state(&mut self, new_state: i32) {
        self.state = new_state;
    }

    pub fn is_connected(&self) -> bool {
        self.state == 2 // Connected
    }
}
