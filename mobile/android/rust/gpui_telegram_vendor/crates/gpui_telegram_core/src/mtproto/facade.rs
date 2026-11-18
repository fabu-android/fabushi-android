//! High-Level MTProto Subsystem Facade & DC Shifting Helper
//! Upstream Source: `Telegram/SourceFiles/mtproto/facade.h`, `facade.cpp`

use crate::mtproto::core_types::{
    shift_dc_id, DcId, ShiftedDcId, CONFIG_DC_SHIFT, LOGOUT_DC_SHIFT, UPDATER_DC_SHIFT,
    GROUP_CALL_STREAM_DC_SHIFT, BASE_DOWNLOAD_DC_SHIFT, BASE_UPLOAD_DC_SHIFT,
    MAX_MEDIA_DC_COUNT,
};
use crate::mtproto::mtp_instance::MtpInstance;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facade {
    pub instance: MtpInstance,
    pub is_paused: bool,
}

impl Default for Facade {
    fn default() -> Self {
        Self {
            instance: MtpInstance::default(),
            is_paused: false,
        }
    }
}

impl Facade {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        !self.is_paused && self.instance.is_active()
    }

    pub fn config_dc_id(&self, dc_id: DcId) -> ShiftedDcId {
        shift_dc_id(dc_id, CONFIG_DC_SHIFT)
    }

    pub fn logout_dc_id(&self, dc_id: DcId) -> ShiftedDcId {
        shift_dc_id(dc_id, LOGOUT_DC_SHIFT)
    }

    pub fn updater_dc_id(&self, dc_id: DcId) -> ShiftedDcId {
        shift_dc_id(dc_id, UPDATER_DC_SHIFT)
    }

    pub fn group_call_stream_dc_id(&self, dc_id: DcId) -> ShiftedDcId {
        shift_dc_id(dc_id, GROUP_CALL_STREAM_DC_SHIFT)
    }

    pub fn download_dc_id(&self, dc_id: DcId, index: i32) -> ShiftedDcId {
        let clamped = index.clamp(0, MAX_MEDIA_DC_COUNT - 1);
        shift_dc_id(dc_id, BASE_DOWNLOAD_DC_SHIFT + clamped)
    }

    pub fn upload_dc_id(&self, index: i32) -> ShiftedDcId {
        shift_dc_id(self.instance.main_dc_id, BASE_UPLOAD_DC_SHIFT + index)
    }

    pub fn pause(&mut self) {
        self.is_paused = true;
        self.instance.pause();
    }

    pub fn unpause(&mut self) {
        self.is_paused = false;
        self.instance.unpause();
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused
    }
}
