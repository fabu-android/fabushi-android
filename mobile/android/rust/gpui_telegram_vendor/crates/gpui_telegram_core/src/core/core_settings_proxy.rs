//! Telegram Desktop Proxy Configuration & Rotation Settings
//! Upstream sources: `Telegram/SourceFiles/core/core_settings_proxy.h`, `core_settings_proxy.cpp`

use serde::{Deserialize, Serialize};

pub const DEFAULT_PROXY_ROTATION_TIMEOUT: u32 = 10;
pub const PROXY_ROTATION_TIMEOUTS: [u32; 5] = [5, 10, 15, 30, 60];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ProxyMode {
    #[default]
    System,
    Disabled,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProxyProtocol {
    Socks5,
    Http,
    Mtproto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyEntry {
    pub protocol: ProxyProtocol,
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub password: Option<String>,
    pub secret: Option<String>, // for MTProto proxy
    pub ping_ms: Option<u32>,
    pub is_available: bool,
}

impl ProxyEntry {
    pub fn new_socks5(host: impl Into<String>, port: u16) -> Self {
        Self {
            protocol: ProxyProtocol::Socks5,
            host: host.into(),
            port,
            user: None,
            password: None,
            secret: None,
            ping_ms: None,
            is_available: true,
        }
    }

    pub fn new_mtproto(host: impl Into<String>, port: u16, secret: impl Into<String>) -> Self {
        Self {
            protocol: ProxyProtocol::Mtproto,
            host: host.into(),
            port,
            user: None,
            password: None,
            secret: Some(secret.into()),
            ping_ms: None,
            is_available: true,
        }
    }
}

/// Persistent proxy management, rotation timeouts, and proxy list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreSettingsProxy {
    pub mode: ProxyMode,
    pub try_ipv6: bool,
    pub use_proxy_for_calls: bool,
    pub proxy_rotation_enabled: bool,
    pub proxy_rotation_timeout: u32,
    pub check_ip_warning_shown: bool,
    pub selected: Option<ProxyEntry>,
    pub list: Vec<ProxyEntry>,
    pub preferred_indices: Vec<usize>,
}

impl Default for CoreSettingsProxy {
    fn default() -> Self {
        Self {
            mode: ProxyMode::System,
            try_ipv6: false,
            use_proxy_for_calls: false,
            proxy_rotation_enabled: false,
            proxy_rotation_timeout: DEFAULT_PROXY_ROTATION_TIMEOUT,
            check_ip_warning_shown: false,
            selected: None,
            list: Vec::new(),
            preferred_indices: Vec::new(),
        }
    }
}

impl CoreSettingsProxy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_enabled(&self) -> bool {
        self.mode == ProxyMode::Manual && self.selected.is_some()
    }

    pub fn is_system(&self) -> bool {
        self.mode == ProxyMode::System
    }

    pub fn is_disabled(&self) -> bool {
        self.mode == ProxyMode::Disabled
    }

    pub fn set_mode(&mut self, mode: ProxyMode) {
        self.mode = mode;
    }

    pub fn selected(&self) -> Option<&ProxyEntry> {
        self.selected.as_ref()
    }

    pub fn set_selected(&mut self, entry: Option<ProxyEntry>) {
        self.selected = entry;
        if self.selected.is_some() {
            self.mode = ProxyMode::Manual;
        }
    }

    pub fn list(&self) -> &[ProxyEntry] {
        &self.list
    }

    pub fn add_to_list(&mut self, entry: ProxyEntry) {
        if !self.list.iter().any(|e| e.host == entry.host && e.port == entry.port) {
            self.list.push(entry);
        }
    }

    pub fn remove_from_list(&mut self, host: &str, port: u16) -> bool {
        let initial_len = self.list.len();
        self.list.retain(|e| !(e.host == host && e.port == port));
        if let Some(ref sel) = self.selected {
            if sel.host == host && sel.port == port {
                self.selected = self.list.first().cloned();
            }
        }
        self.list.len() < initial_len
    }

    pub fn set_proxy_rotation_enabled(&mut self, enabled: bool) {
        self.proxy_rotation_enabled = enabled;
    }

    pub fn set_proxy_rotation_timeout(&mut self, timeout: u32) {
        if PROXY_ROTATION_TIMEOUTS.contains(&timeout) {
            self.proxy_rotation_timeout = timeout;
        }
    }

    pub fn set_try_ipv6(&mut self, try_ipv6: bool) {
        self.try_ipv6 = try_ipv6;
    }

    pub fn set_use_proxy_for_calls(&mut self, use_calls: bool) {
        self.use_proxy_for_calls = use_calls;
    }

    pub fn serialize(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    pub fn set_from_serialized(&mut self, bytes: &[u8]) -> Result<(), String> {
        let parsed: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        *self = parsed;
        Ok(())
    }
}
