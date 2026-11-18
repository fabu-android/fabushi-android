//! MTProto RPC Response & Error Data Types
//! Upstream Source: `Telegram/SourceFiles/mtproto/mtproto_response.h`, `mtproto_response.cpp`

use crate::mtproto::core_types::{MsgId, RequestId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub error_type: String,
    pub description: String,
}

impl RpcError {
    pub const NO_ERROR: i32 = 0;
    pub const TIMEOUT_ERROR: i32 = -408;
    pub const OTHER_ERROR: i32 = -499;

    pub fn new(code: i32, error_type: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            code,
            error_type: error_type.into(),
            description: description.into(),
        }
    }

    pub fn local(error_type: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            code: Self::OTHER_ERROR,
            error_type: error_type.into(),
            description: description.into(),
        }
    }

    pub fn flood_wait_seconds(&self) -> Option<u64> {
        if let Some(rest) = self.error_type.strip_prefix("FLOOD_WAIT_") {
            rest.parse::<u64>().ok()
        } else if let Some(rest) = self.error_type.strip_prefix("FLOOD_PREMIUM_WAIT_") {
            rest.parse::<u64>().ok()
        } else {
            None
        }
    }

    pub fn file_migrate_dc(&self) -> Option<i32> {
        if let Some(rest) = self.error_type.strip_prefix("FILE_MIGRATE_") {
            rest.parse::<i32>().ok()
        } else {
            None
        }
    }

    pub fn network_migrate_dc(&self) -> Option<i32> {
        if let Some(rest) = self.error_type.strip_prefix("NETWORK_MIGRATE_") {
            rest.parse::<i32>().ok()
        } else if let Some(rest) = self.error_type.strip_prefix("PHONE_MIGRATE_") {
            rest.parse::<i32>().ok()
        } else {
            None
        }
    }

    pub fn is_flood(&self) -> bool {
        self.flood_wait_seconds().is_some()
    }

    pub fn is_temporary(&self) -> bool {
        self.code < 0 || self.code >= 500 || self.is_flood()
    }

    pub fn is_ignored(&self) -> bool {
        self.code == 406
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub reply: Vec<u8>,
    pub outer_msg_id: MsgId,
    pub request_id: RequestId,
}

impl Response {
    pub fn new(request_id: RequestId, outer_msg_id: MsgId, reply: Vec<u8>) -> Self {
        Self {
            reply,
            outer_msg_id,
            request_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoResponse {
    pub response: Option<Response>,
    pub error: Option<RpcError>,
}

impl Default for MtprotoResponse {
    fn default() -> Self {
        Self {
            response: None,
            error: None,
        }
    }
}

impl MtprotoResponse {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.response.is_some() || self.error.is_some()
    }

    pub fn success(response: Response) -> Self {
        Self {
            response: Some(response),
            error: None,
        }
    }

    pub fn error(err: RpcError) -> Self {
        Self {
            response: None,
            error: Some(err),
        }
    }

    pub fn is_ok(&self) -> bool {
        self.response.is_some() && self.error.is_none()
    }
}
