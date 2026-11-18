//! Telegram Desktop 1:1 Parity Module: `TonExplorerUrl`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/ton_explorer_url.h, Telegram/SourceFiles/core/ton_explorer_url.cpp

use serde::{Deserialize, Serialize};

pub const DEFAULT_TON_EXPLORER_BASE: &str = "https://tonviewer.com";

/// TON blockchain explorer URL generator with customizable base provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TonExplorerUrl {
    pub base_url: String,
}

impl Default for TonExplorerUrl {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_TON_EXPLORER_BASE.to_string(),
        }
    }
}

impl TonExplorerUrl {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_base(base: impl Into<String>) -> Self {
        Self {
            base_url: base.into(),
        }
    }

    /// Constructs a fully qualified explorer URL for a given route/hash/address.
    pub fn format_url(&self, route: &str) -> String {
        format_ton_explorer_url(&self.base_url, route)
    }

    /// Formats transaction view URL.
    pub fn transaction_url(&self, tx_hash: &str) -> String {
        self.format_url(&format!("transaction/{}", tx_hash))
    }

    /// Formats account address view URL.
    pub fn account_url(&self, address: &str) -> String {
        self.format_url(address)
    }
}

pub fn format_ton_explorer_url(base: &str, route: &str) -> String {
    let mut trimmed_base = base.trim_end_matches('/');
    if trimmed_base.is_empty() {
        trimmed_base = DEFAULT_TON_EXPLORER_BASE;
    }
    let trimmed_route = route.trim_start_matches('/');
    format!("{}/{}", trimmed_base, trimmed_route)
}
