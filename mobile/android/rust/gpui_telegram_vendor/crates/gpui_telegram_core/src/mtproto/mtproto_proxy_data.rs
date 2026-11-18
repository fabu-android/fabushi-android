//! MTProto Proxy Configuration & Validation
//! Upstream Source: `Telegram/SourceFiles/mtproto/mtproto_proxy_data.h`, `mtproto_proxy_data.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ProxyType {
    #[default]
    None,
    Socks5,
    Http,
    Mtproto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MtprotoProxyData {
    pub proxy_type: ProxyType,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub secret: Vec<u8>,
}

impl MtprotoProxyData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        true
    }

    pub fn new_socks5(host: impl Into<String>, port: u16) -> Self {
        Self {
            proxy_type: ProxyType::Socks5,
            host: host.into(),
            port,
            user: String::new(),
            password: String::new(),
            secret: Vec::new(),
        }
    }

    pub fn new_mtproto(host: impl Into<String>, port: u16, secret: Vec<u8>) -> Self {
        Self {
            proxy_type: ProxyType::Mtproto,
            host: host.into(),
            port,
            user: String::new(),
            password: String::new(),
            secret,
        }
    }

    pub fn validate_secret(&self) -> bool {
        if self.proxy_type != ProxyType::Mtproto {
            return true;
        }
        // Valid MTProto proxy secrets: 16 bytes raw, 17 bytes (0xdd + 16 bytes for padded intermediate),
        // or 17+ bytes (domain-fronted).
        if self.secret.len() < 16 {
            return false;
        }
        if self.secret.len() == 17 && self.secret[0] != 0xdd {
            return false;
        }
        true
    }

    pub fn is_padded_intermediate(&self) -> bool {
        self.secret.len() >= 17 && self.secret[0] == 0xdd
    }
}
