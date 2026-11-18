//! MTProto Packet Debug Dumper & Formatter
//! Upstream Source: `Telegram/SourceFiles/mtproto/details/mtproto_dump_to_text.h`, `mtproto_dump_to_text.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoDumpToTextDetails {
    pub max_dump_bytes: usize,
    pub redact_sensitive_auth_data: bool,
}

impl Default for MtprotoDumpToTextDetails {
    fn default() -> Self {
        Self {
            max_dump_bytes: 256,
            redact_sensitive_auth_data: true,
        }
    }
}

impl MtprotoDumpToTextDetails {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.max_dump_bytes > 0
    }

    pub fn dump_hex(&self, data: &[u8]) -> String {
        let print_len = data.len().min(self.max_dump_bytes);
        let mut hex = String::with_capacity(print_len * 3);
        for &byte in &data[..print_len] {
            use std::fmt::Write;
            let _ = write!(hex, "{:02x} ", byte);
        }
        if data.len() > self.max_dump_bytes {
            hex.push_str("... (truncated)");
        }
        hex
    }

    pub fn dump_packet_header(&self, auth_key_id: u64, msg_id: u64, seq_no: u32, len: u32) -> String {
        if self.redact_sensitive_auth_data && auth_key_id != 0 {
            format!("[Encrypted Packet: KeyId=0x{:016x} MsgId=0x{:016x} SeqNo={} Len={}]", auth_key_id, msg_id, seq_no, len)
        } else {
            format!("[Plaintext Packet: MsgId=0x{:016x} SeqNo={} Len={}]", msg_id, seq_no, len)
        }
    }
}
