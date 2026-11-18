//! MTProto Session Orchestrator & Request-Response Demultiplexing
//! Upstream Source: `Telegram/SourceFiles/mtproto/session.h`, `session.cpp`

use std::collections::HashMap;
use crate::mtproto::core_types::{bare_dc_id, MsgId, RequestId, ShiftedDcId};
use crate::mtproto::mtproto_proxy_data::MtprotoProxyData;
use crate::mtproto::mtproto_response::Response;
use crate::mtproto::details::mtproto_serialized_request::MtprotoSerializedRequestDetails;
use crate::mtproto::session_private::SessionPrivate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOptions {
    pub system_lang_code: String,
    pub cloud_lang_code: String,
    pub lang_pack_name: String,
    pub proxy: MtprotoProxyData,
    pub use_ipv4: bool,
    pub use_ipv6: bool,
    pub use_http: bool,
    pub use_tcp: bool,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            system_lang_code: "en".to_string(),
            cloud_lang_code: "en".to_string(),
            lang_pack_name: "tdesktop".to_string(),
            proxy: MtprotoProxyData::default(),
            use_ipv4: true,
            use_ipv6: true,
            use_http: true,
            use_tcp: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SessionData {
    pub to_send: HashMap<RequestId, MtprotoSerializedRequestDetails>,
    pub have_sent: HashMap<MsgId, MtprotoSerializedRequestDetails>,
    pub received_responses: Vec<Response>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub shifted_dc_id: ShiftedDcId,
    pub options: SessionOptions,
    pub data: SessionData,
    pub private_state: SessionPrivate,
    pub last_msg_id: MsgId,
    pub is_running: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            shifted_dc_id: 2,
            options: SessionOptions::default(),
            data: SessionData::default(),
            private_state: SessionPrivate::default(),
            last_msg_id: 0,
            is_running: true,
        }
    }
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.is_running && self.private_state.is_active()
    }

    pub fn with_dc(shifted_dc_id: ShiftedDcId, key_id: u64, session_id: u64, salt: u64) -> Self {
        Self {
            shifted_dc_id,
            options: SessionOptions::default(),
            data: SessionData::default(),
            private_state: SessionPrivate::with_dc(shifted_dc_id, key_id, session_id, salt),
            last_msg_id: 0,
            is_running: true,
        }
    }

    /// Generates a strictly monotonically increasing client message ID:
    /// msg_id = (unixtime << 32) | (fractional_microseconds << 2)
    /// Lower 2 bits must be 0 (divisible by 4) for client-to-server messages.
    pub fn generate_msg_id(&mut self, unixtime_sec: i64) -> MsgId {
        let base_id = ((unixtime_sec as u64) << 32) & !3;
        let mut candidate = if base_id <= self.last_msg_id {
            (self.last_msg_id + 4) & !3
        } else {
            base_id
        };
        if candidate <= self.last_msg_id {
            candidate = self.last_msg_id + 4;
        }
        self.last_msg_id = candidate;
        candidate
    }

    pub fn queue_request(&mut self, mut request: MtprotoSerializedRequestDetails, unixtime_sec: i64) -> RequestId {
        let req_id = request.request_id;
        let msg_id = self.generate_msg_id(unixtime_sec);
        let seq_no = self.private_state.next_request_seq_number(true);
        request.set_msg_id(msg_id);
        request.set_seq_no(seq_no);
        self.data.to_send.insert(req_id, request);
        req_id
    }

    pub fn mark_sent(&mut self, request_id: RequestId) -> Option<MsgId> {
        if let Some(req) = self.data.to_send.remove(&request_id) {
            let msg_id = req.msg_id;
            self.data.have_sent.insert(msg_id, req);
            Some(msg_id)
        } else {
            None
        }
    }

    pub fn cancel_request(&mut self, request_id: RequestId) -> bool {
        let removed_to_send = self.data.to_send.remove(&request_id).is_some();
        let mut found_msg_id = None;
        for (&msg_id, req) in &self.data.have_sent {
            if req.request_id == request_id {
                found_msg_id = Some(msg_id);
                break;
            }
        }
        if let Some(msg_id) = found_msg_id {
            self.data.have_sent.remove(&msg_id);
        }
        removed_to_send || found_msg_id.is_some()
    }

    pub fn handle_response(&mut self, response: Response) {
        self.data.have_sent.remove(&response.outer_msg_id);
        self.private_state.queue_ack(response.outer_msg_id);
        self.data.received_responses.push(response);
    }

    pub fn pop_received_response(&mut self) -> Option<Response> {
        self.data.received_responses.pop()
    }

    pub fn bare_dc_id(&self) -> crate::mtproto::core_types::DcId {
        bare_dc_id(self.shifted_dc_id)
    }

    pub fn reinit_connection(&mut self) {
        self.private_state.reset_session();
    }
}
