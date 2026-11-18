//! MTProto Type Serialization Utilities
//! Upstream Source: `Telegram/SourceFiles/mtproto/type_utils.h`

use crate::mtproto::core_types::{MTPC_BOOL_FALSE, MTPC_BOOL_TRUE};
use serde::{Deserialize, Serialize};

#[inline]
pub fn mtp_bool(val: bool) -> u32 {
    if val {
        MTPC_BOOL_TRUE
    } else {
        MTPC_BOOL_FALSE
    }
}

#[inline]
pub fn mtp_is_true(constructor: u32) -> bool {
    constructor == MTPC_BOOL_TRUE
}

#[inline]
pub fn mtp_is_false(constructor: u32) -> bool {
    constructor == MTPC_BOOL_FALSE
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeUtils {
    pub default_true_id: u32,
    pub default_false_id: u32,
}

impl Default for TypeUtils {
    fn default() -> Self {
        Self {
            default_true_id: MTPC_BOOL_TRUE,
            default_false_id: MTPC_BOOL_FALSE,
        }
    }
}

impl TypeUtils {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        true
    }

    pub fn encode_bool(&self, value: bool) -> u32 {
        mtp_bool(value)
    }

    pub fn decode_bool(&self, constructor: u32) -> Option<bool> {
        if constructor == self.default_true_id {
            Some(true)
        } else if constructor == self.default_false_id {
            Some(false)
        } else {
            None
        }
    }
}
