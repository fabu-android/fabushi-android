//! Temporary-to-Persistent Key Binding Orchestrator
//! Upstream Source: `Telegram/SourceFiles/mtproto/details/mtproto_bound_key_creator.h`, `mtproto_bound_key_creator.cpp`

use crate::mtproto::core_types::DcId;
use super::mtproto_dc_key_creator::MtprotoDcKeyCreatorDetails;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoBoundKeyCreatorDetails {
    pub dc_id: DcId,
    pub key_creator: MtprotoDcKeyCreatorDetails,
    pub persistent_key_id: u64,
    pub binding_in_progress: bool,
    pub bind_msg_id: u64,
    pub is_successfully_bound: bool,
}

impl Default for MtprotoBoundKeyCreatorDetails {
    fn default() -> Self {
        Self {
            dc_id: 2,
            key_creator: MtprotoDcKeyCreatorDetails::default(),
            persistent_key_id: 0,
            binding_in_progress: false,
            bind_msg_id: 0,
            is_successfully_bound: false,
        }
    }
}

impl MtprotoBoundKeyCreatorDetails {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.binding_in_progress || self.is_successfully_bound
    }

    pub fn start_binding(&mut self, persistent_key_id: u64) {
        self.persistent_key_id = persistent_key_id;
        self.binding_in_progress = true;
        self.is_successfully_bound = false;
    }

    pub fn record_bind_request(&mut self, msg_id: u64) {
        self.bind_msg_id = msg_id;
    }

    pub fn complete_binding(&mut self, response_msg_id: u64) -> bool {
        if self.bind_msg_id != 0 && response_msg_id >= self.bind_msg_id {
            self.binding_in_progress = false;
            self.is_successfully_bound = true;
            true
        } else {
            false
        }
    }
}
