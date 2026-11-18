//! Telegram Desktop Multi-Proxy Health Monitor & Rotation Scheduler
//! Upstream sources: `Telegram/SourceFiles/core/proxy_rotation_manager.h`, `proxy_rotation_manager.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyProbeEntry {
    pub host: String,
    pub port: u16,
    pub checking: bool,
    pub ping_ms: Option<u32>,
    pub is_available: bool,
    pub last_checked_at: i64,
}

/// Dynamic proxy rotation manager cycling through configured proxies upon connection stalls.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProxyRotationManager {
    pub entries: Vec<ProxyProbeEntry>,
    pub is_checking: bool,
    pub waiting_to_switch: bool,
    pub active_index: Option<usize>,
    pub switch_timeout_secs: u32,
    pub switch_started_at: i64,
}

impl ProxyRotationManager {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            is_checking: false,
            waiting_to_switch: false,
            active_index: None,
            switch_timeout_secs: 10,
            switch_started_at: 0,
        }
    }

    pub fn set_entries(&mut self, entries: Vec<ProxyProbeEntry>) {
        self.entries = entries;
        if !self.entries.is_empty() && self.active_index.is_none() {
            self.active_index = Some(0);
        }
    }

    pub fn start_checking(&mut self) {
        self.is_checking = true;
    }

    pub fn stop_checking(&mut self) {
        self.is_checking = false;
        self.waiting_to_switch = false;
    }

    pub fn mark_available(&mut self, index: usize, ping_ms: u32) {
        if let Some(entry) = self.entries.get_mut(index) {
            entry.is_available = true;
            entry.ping_ms = Some(ping_ms);
            entry.checking = false;
            entry.last_checked_at = chrono::Utc::now().timestamp();
        }
    }

    pub fn mark_failed(&mut self, index: usize) {
        if let Some(entry) = self.entries.get_mut(index) {
            entry.is_available = false;
            entry.checking = false;
            entry.last_checked_at = chrono::Utc::now().timestamp();
        }
    }

    pub fn switch_to_next_available(&mut self) -> Option<usize> {
        let count = self.entries.len();
        if count <= 1 {
            return self.active_index;
        }
        let start = self.active_index.map(|i| (i + 1) % count).unwrap_or(0);
        for offset in 0..count {
            let idx = (start + offset) % count;
            if self.entries[idx].is_available {
                self.active_index = Some(idx);
                return Some(idx);
            }
        }
        None
    }
}
