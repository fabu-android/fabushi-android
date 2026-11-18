//! MTProto Core Types and Protocol Constants
//! Upstream Source: `Telegram/SourceFiles/mtproto/core_types.h`

use serde::{Deserialize, Serialize};

pub type DcId = i32;
pub type ShiftedDcId = i32;
pub type MsgId = u64;
pub type RequestId = i32;
pub type PingId = u64;
pub type TypeId = u32;

pub const DC_SHIFT: ShiftedDcId = 10000;
pub const CONFIG_DC_SHIFT: i32 = 0x01;
pub const LOGOUT_DC_SHIFT: i32 = 0x02;
pub const UPDATER_DC_SHIFT: i32 = 0x03;
pub const EXPORT_DC_SHIFT: i32 = 0x04;
pub const EXPORT_MEDIA_DC_SHIFT: i32 = 0x05;
pub const GROUP_CALL_STREAM_DC_SHIFT: i32 = 0x06;
pub const STATS_DC_SHIFT: i32 = 0x07;
pub const TONCENTER_DC_SHIFT: i32 = 0x08;
pub const WALLET_SHARE_DC_SHIFT: i32 = 0x09;
pub const MAX_MEDIA_DC_COUNT: i32 = 0x10;
pub const BASE_DOWNLOAD_DC_SHIFT: i32 = 0x10;
pub const BASE_UPLOAD_DC_SHIFT: i32 = 0x20;
pub const DESTROY_KEY_START_DC_SHIFT: i32 = 0x100;

// Protocol Message / Boxed Type IDs
pub const MTPC_RPC_RESULT: u32 = 0xf35c6d01;
pub const MTPC_MSG_CONTAINER: u32 = 0x73f1f8dc;
pub const MTPC_GZIP_PACKED: u32 = 0x3072cfa1;
pub const MTPC_MSGS_ACK: u32 = 0x62d6b459;
pub const MTPC_BAD_MSG_NOTIFICATION: u32 = 0xa7eff811;
pub const MTPC_BAD_SERVER_SALT: u32 = 0xedab447b;
pub const MTPC_MSGS_STATE_REQ: u32 = 0xda69fb52;
pub const MTPC_MSGS_STATE_INFO: u32 = 0x04deb57d;
pub const MTPC_MSGS_ALL_INFO: u32 = 0x8cc0d131;
pub const MTPC_MSG_DETAILED_INFO: u32 = 0x276d3ec6;
pub const MTPC_MSG_NEW_DETAILED_INFO: u32 = 0x809db6ed;
pub const MTPC_NEW_SESSION_CREATED: u32 = 0x9ec20908;
pub const MTPC_HTTP_WAIT: u32 = 0x9299359f;

pub const MTPC_BOOL_FALSE: u32 = 0xbc799737;
pub const MTPC_BOOL_TRUE: u32 = 0x997275b5;

#[inline]
pub fn bare_dc_id(shifted_dc_id: ShiftedDcId) -> DcId {
    shifted_dc_id % DC_SHIFT
}

#[inline]
pub fn shift_dc_id(dc_id: DcId, value: i32) -> ShiftedDcId {
    dc_id + DC_SHIFT * value
}

#[inline]
pub fn get_dc_id_shift(shifted_dc_id: ShiftedDcId) -> i32 {
    shifted_dc_id / DC_SHIFT
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreTypes {
    pub primary_dc_id: DcId,
    pub shifted_dc_id: ShiftedDcId,
    pub max_media_dc_count: i32,
    pub is_test_environment: bool,
}

impl Default for CoreTypes {
    fn default() -> Self {
        Self {
            primary_dc_id: 2,
            shifted_dc_id: 2,
            max_media_dc_count: MAX_MEDIA_DC_COUNT,
            is_test_environment: false,
        }
    }
}

impl CoreTypes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.primary_dc_id > 0
    }

    pub fn with_dc(dc_id: DcId, is_test: bool) -> Self {
        Self {
            primary_dc_id: dc_id,
            shifted_dc_id: dc_id,
            max_media_dc_count: MAX_MEDIA_DC_COUNT,
            is_test_environment: is_test,
        }
    }

    pub fn download_dc_id(&self, index: i32) -> ShiftedDcId {
        shift_dc_id(self.primary_dc_id, BASE_DOWNLOAD_DC_SHIFT + index.clamp(0, MAX_MEDIA_DC_COUNT - 1))
    }

    pub fn upload_dc_id(&self, index: i32) -> ShiftedDcId {
        shift_dc_id(self.primary_dc_id, BASE_UPLOAD_DC_SHIFT + index)
    }

    pub fn config_dc_id(&self) -> ShiftedDcId {
        shift_dc_id(self.primary_dc_id, CONFIG_DC_SHIFT)
    }

    pub fn logout_dc_id(&self) -> ShiftedDcId {
        shift_dc_id(self.primary_dc_id, LOGOUT_DC_SHIFT)
    }

    pub fn is_download_dc(&self, shifted: ShiftedDcId) -> bool {
        let shift = get_dc_id_shift(shifted);
        (BASE_DOWNLOAD_DC_SHIFT..BASE_DOWNLOAD_DC_SHIFT + MAX_MEDIA_DC_COUNT).contains(&shift)
    }
}
