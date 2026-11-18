//! Telegram Desktop 1:1 Parity Submodule: `ApiHash`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_hash.cpp, Telegram/SourceFiles/api/api_hash.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[inline]
pub fn hash_init() -> u64 {
    0
}

#[inline]
pub fn hash_update(already: &mut u64, value: u64) {
    *already ^= *already >> 21;
    *already ^= *already << 35;
    *already ^= *already >> 4;
    *already = already.wrapping_add(value);
}

#[inline]
pub fn hash_finalize(already: u64) -> u64 {
    already
}

pub fn count_hash<I: IntoIterator<Item = u64>>(items: I) -> u64 {
    let mut h = hash_init();
    for item in items {
        hash_update(&mut h, item);
    }
    hash_finalize(h)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiHash {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub last_computed_hash: u64,
    pub updated_at: i64,
}

impl Default for ApiHash {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiHash".to_string(),
            last_computed_hash: 0,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiHash {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_label(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.is_enabled = enabled;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn compute_hash<I: IntoIterator<Item = u64>>(&mut self, items: I) -> u64 {
        let h = count_hash(items);
        self.last_computed_hash = h;
        self.updated_at = chrono::Utc::now().timestamp();
        h
    }
}
