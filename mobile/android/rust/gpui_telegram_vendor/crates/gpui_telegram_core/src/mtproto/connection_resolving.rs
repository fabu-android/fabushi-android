//! MTProto Asynchronous Connection Endpoint Resolving FSM
//! Upstream Source: `Telegram/SourceFiles/mtproto/connection_resolving.h`, `connection_resolving.cpp`

use crate::mtproto::core_types::DcId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionResolving {
    pub dc_id: DcId,
    pub host: String,
    pub port: u16,
    pub resolved_ip: Option<String>,
    pub is_resolving: bool,
    pub retries: u32,
}

impl Default for ConnectionResolving {
    fn default() -> Self {
        Self {
            dc_id: 2,
            host: String::new(),
            port: 443,
            resolved_ip: None,
            is_resolving: false,
            retries: 0,
        }
    }
}

impl ConnectionResolving {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.is_resolving || self.resolved_ip.is_some()
    }

    pub fn start_resolving(&mut self, host: impl Into<String>, port: u16, dc_id: DcId) {
        self.host = host.into();
        self.port = port;
        self.dc_id = dc_id;
        self.is_resolving = true;
        self.resolved_ip = None;
    }

    pub fn complete_resolving(&mut self, ip: impl Into<String>) {
        self.resolved_ip = Some(ip.into());
        self.is_resolving = false;
    }

    pub fn fail_resolving(&mut self) {
        self.is_resolving = false;
        self.retries = self.retries.saturating_add(1);
    }

    pub fn is_resolved(&self) -> bool {
        self.resolved_ip.is_some()
    }
}
