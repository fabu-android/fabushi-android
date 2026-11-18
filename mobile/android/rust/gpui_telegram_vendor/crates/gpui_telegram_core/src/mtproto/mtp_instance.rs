//! Central MTProto Runtime Instance Singleton
//! Upstream Source: `Telegram/SourceFiles/mtproto/mtp_instance.h`, `mtp_instance.cpp`

use std::collections::HashMap;
use crate::mtproto::core_types::{DcId, ShiftedDcId};
use crate::mtproto::mtproto_config::MtprotoConfig;
use crate::mtproto::mtproto_dc_options::MtprotoDcOptions;
use crate::mtproto::session::Session;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtpInstance {
    pub main_dc_id: DcId,
    pub dc_options: MtprotoDcOptions,
    pub sessions: HashMap<ShiftedDcId, Session>,
    pub is_paused: bool,
    pub config: MtprotoConfig,
}

impl Default for MtpInstance {
    fn default() -> Self {
        let dc_options = MtprotoDcOptions::default();
        let main_dc_id = 2;
        let mut sessions = HashMap::new();
        sessions.insert(main_dc_id, Session::with_dc(main_dc_id, 0, 0x1122334455667788, 0x99aabbccddeeff00));

        Self {
            main_dc_id,
            dc_options,
            sessions,
            is_paused: false,
            config: MtprotoConfig::default(),
        }
    }
}

impl MtpInstance {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.main_dc_id > 0 && !self.is_paused
    }

    pub fn set_main_dc_id(&mut self, dc_id: DcId) {
        self.main_dc_id = dc_id;
        if !self.sessions.contains_key(&dc_id) {
            self.sessions.insert(dc_id, Session::with_dc(dc_id, 0, 0x1122334455667788, 0x99aabbccddeeff00));
        }
    }

    pub fn get_session_mut(&mut self, shifted_dc_id: ShiftedDcId) -> &mut Session {
        self.sessions.entry(shifted_dc_id).or_insert_with(|| {
            Session::with_dc(shifted_dc_id, 0, 0x1122334455667788, 0x99aabbccddeeff00)
        })
    }

    pub fn pause(&mut self) {
        self.is_paused = true;
    }

    pub fn unpause(&mut self) {
        self.is_paused = false;
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused
    }
}
