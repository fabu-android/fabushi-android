//! MTProto Proxy Health & Latency Probe
//! Upstream Source: `Telegram/SourceFiles/mtproto/proxy_check.h`, `proxy_check.cpp`

use super::mtproto_proxy_data::MtprotoProxyData;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyCheck {
    pub proxy: MtprotoProxyData,
    pub ping_ms: Option<u64>,
    pub is_checking: bool,
    pub last_checked_at: i64,
}

impl Default for ProxyCheck {
    fn default() -> Self {
        Self {
            proxy: MtprotoProxyData::default(),
            ping_ms: None,
            is_checking: false,
            last_checked_at: 0,
        }
    }
}

impl ProxyCheck {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.is_checking || self.ping_ms.is_some()
    }

    pub fn for_proxy(proxy: MtprotoProxyData) -> Self {
        Self {
            proxy,
            ping_ms: None,
            is_checking: false,
            last_checked_at: 0,
        }
    }

    pub fn start(&mut self) {
        self.is_checking = true;
    }

    pub fn finish_success(&mut self, latency_ms: u64) {
        self.ping_ms = Some(latency_ms);
        self.is_checking = false;
        self.last_checked_at = chrono::Utc::now().timestamp();
    }

    pub fn finish_failed(&mut self) {
        self.ping_ms = None;
        self.is_checking = false;
        self.last_checked_at = chrono::Utc::now().timestamp();
    }

    pub fn is_available(&self) -> bool {
        self.ping_ms.is_some()
    }
}
