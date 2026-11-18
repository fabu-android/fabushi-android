//! Telegram Desktop 1:1 Parity Module: `TrayAccountsMenuDummy`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/tray_accounts_menu_dummy.cpp

use serde::{Deserialize, Serialize};

/// Fallback dummy implementation for platforms or environments where
/// native system tray menus or dynamic submenus are unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TrayAccountsMenuDummy {
    pub is_available: bool,
}

impl TrayAccountsMenuDummy {
    pub fn new() -> Self {
        Self {
            is_available: false,
        }
    }

    pub fn is_supported(&self) -> bool {
        false
    }
}
