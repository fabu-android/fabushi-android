//! Datacenter Secret & Key Store State
//! Upstream Source: `Telegram/SourceFiles/mtproto/details/mtproto_dcenter.h`, `mtproto_dcenter.cpp`

use crate::mtproto::core_types::DcId;
use crate::mtproto::mtproto_auth_key::MtprotoAuthKey;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoDcenterDetails {
    pub dc_id: DcId,
    pub persistent_key: Option<MtprotoAuthKey>,
    pub temporary_key: Option<MtprotoAuthKey>,
    pub server_salt: u64,
    pub salt_expires_at: i64,
    pub key_creation_in_progress: bool,
}

impl Default for MtprotoDcenterDetails {
    fn default() -> Self {
        Self {
            dc_id: 2,
            persistent_key: None,
            temporary_key: None,
            server_salt: 0,
            salt_expires_at: 0,
            key_creation_in_progress: false,
        }
    }
}

impl MtprotoDcenterDetails {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.dc_id > 0
    }

    pub fn with_dc_id(dc_id: DcId) -> Self {
        Self {
            dc_id,
            persistent_key: None,
            temporary_key: None,
            server_salt: 0,
            salt_expires_at: 0,
            key_creation_in_progress: false,
        }
    }

    pub fn has_auth_key(&self) -> bool {
        self.temporary_key.is_some() || self.persistent_key.is_some()
    }

    pub fn get_active_auth_key(&self) -> Option<&MtprotoAuthKey> {
        self.temporary_key.as_ref().or(self.persistent_key.as_ref())
    }

    pub fn set_persistent_key(&mut self, key: MtprotoAuthKey) {
        self.persistent_key = Some(key);
    }

    pub fn set_temporary_key(&mut self, key: MtprotoAuthKey) {
        self.temporary_key = Some(key);
    }

    pub fn destroy_temporary_key(&mut self) {
        self.temporary_key = None;
    }

    pub fn update_salt(&mut self, salt: u64, expires_at: i64) {
        self.server_salt = salt;
        self.salt_expires_at = expires_at;
    }

    pub fn is_salt_valid(&self, current_time: i64) -> bool {
        self.server_salt != 0 && (self.salt_expires_at == 0 || current_time < self.salt_expires_at)
    }
}
