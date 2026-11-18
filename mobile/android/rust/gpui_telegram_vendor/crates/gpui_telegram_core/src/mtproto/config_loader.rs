//! Multi-DC Configuration Enumeration & Special Config Loader
//! Upstream Source: `Telegram/SourceFiles/mtproto/config_loader.h`, `config_loader.cpp`

use crate::mtproto::core_types::{DcId, RequestId};
use crate::mtproto::mtproto_config::MtprotoConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecialEndpoint {
    pub dc_id: DcId,
    pub ip: String,
    pub port: u16,
    pub secret: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigLoader {
    pub label: String,
    pub phone: String,
    pub proxy_enabled: bool,
    pub current_dc_id: DcId,
    pub special_endpoints: Vec<SpecialEndpoint>,
    pub tried_endpoints: Vec<SpecialEndpoint>,
    pub is_loading: bool,
    pub config: Option<MtprotoConfig>,
    pub enum_request_id: RequestId,
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self {
            label: "ConfigLoader".to_string(),
            phone: String::new(),
            proxy_enabled: false,
            current_dc_id: 2,
            special_endpoints: Vec::new(),
            tried_endpoints: Vec::new(),
            is_loading: false,
            config: None,
            enum_request_id: 0,
        }
    }
}

impl ConfigLoader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        true
    }

    pub fn set_phone(&mut self, phone: impl Into<String>) {
        self.phone = phone.into();
    }

    pub fn set_proxy_enabled(&mut self, value: bool) {
        self.proxy_enabled = value;
    }

    pub fn add_special_endpoint(&mut self, dc_id: DcId, ip: impl Into<String>, port: u16, secret: Vec<u8>) {
        let ep = SpecialEndpoint {
            dc_id,
            ip: ip.into(),
            port,
            secret,
        };
        if !self.special_endpoints.contains(&ep) {
            self.special_endpoints.push(ep);
        }
    }

    pub fn load(&mut self) {
        self.is_loading = true;
    }

    pub fn on_config_loaded(&mut self, config: MtprotoConfig) {
        self.config = Some(config);
        self.is_loading = false;
    }

    pub fn terminate(&mut self) {
        self.is_loading = false;
        self.enum_request_id = 0;
    }
}
