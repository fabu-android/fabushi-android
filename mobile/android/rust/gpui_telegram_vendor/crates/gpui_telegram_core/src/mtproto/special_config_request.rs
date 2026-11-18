//! Special Configuration Request Loader
//! Upstream Source: `Telegram/SourceFiles/mtproto/special_config_request.h`, `special_config_request.cpp`

use crate::mtproto::core_types::{DcId, RequestId};
use crate::mtproto::mtproto_config::MtprotoConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecialConfigRequest {
    pub dc_id: DcId,
    pub phone: String,
    pub request_id: RequestId,
    pub is_loading: bool,
    pub config: Option<MtprotoConfig>,
}

impl Default for SpecialConfigRequest {
    fn default() -> Self {
        Self {
            dc_id: 2,
            phone: String::new(),
            request_id: 0,
            is_loading: false,
            config: None,
        }
    }
}

impl SpecialConfigRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.is_loading || self.config.is_some()
    }

    pub fn start_request(&mut self, phone: impl Into<String>, dc_id: DcId, request_id: RequestId) {
        self.phone = phone.into();
        self.dc_id = dc_id;
        self.request_id = request_id;
        self.is_loading = true;
    }

    pub fn finish_success(&mut self, config: MtprotoConfig) {
        self.config = Some(config);
        self.is_loading = false;
    }

    pub fn cancel(&mut self) {
        self.is_loading = false;
        self.request_id = 0;
    }
}
