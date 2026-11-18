//! Telegram Desktop 1:1 Parity Module: `TrayAccountsMenu`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/tray_accounts_menu.h, Telegram/SourceFiles/tray_accounts_menu.cpp

use serde::{Deserialize, Serialize};

/// Account descriptor displayed in system tray multi-account quick switch menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayAccountEntry {
    pub account_id: u32,
    pub name: String,
    pub username: Option<String>,
    pub unread_count: i32,
    pub is_current: bool,
}

/// Dynamic tray sub-menu listing all active user accounts for rapid profile switching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TrayAccountsMenu {
    pub entries: Vec<TrayAccountEntry>,
}

impl TrayAccountsMenu {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_account(&mut self, entry: TrayAccountEntry) {
        if entry.is_current {
            for existing in &mut self.entries {
                existing.is_current = false;
            }
        }
        self.entries.push(entry);
    }

    pub fn set_current_account(&mut self, account_id: u32) {
        for entry in &mut self.entries {
            entry.is_current = entry.account_id == account_id;
        }
    }

    pub fn current_account(&self) -> Option<&TrayAccountEntry> {
        self.entries.iter().find(|e| e.is_current)
    }

    pub fn total_unread_across_accounts(&self) -> i32 {
        self.entries.iter().map(|e| e.unread_count.max(0)).sum()
    }
}
