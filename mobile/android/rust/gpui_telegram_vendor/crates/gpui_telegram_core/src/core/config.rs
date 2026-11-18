//! Telegram Desktop Global Network & Protocol Constants
//! Upstream sources: `Telegram/SourceFiles/config.h`

use serde::{Deserialize, Serialize};

pub const DEFAULT_API_ID: i32 = 17349;
pub const DEFAULT_API_HASH: &str = "344583e45741c457fe1862106095a5eb";

pub const MAX_SELECTED_ITEMS: usize = 100;
pub const AUTO_SEARCH_TIMEOUT_MS: u64 = 900;
pub const PRELOAD_HEIGHTS_COUNT: usize = 3;
pub const SEARCH_PEOPLE_LIMIT: usize = 5;
pub const UPDATE_DELAY_CONST_SECS: u64 = 8 * 3600;
pub const WRONG_PASSCODE_TIMEOUT_MS: u64 = 1500;
pub const CHOOSE_PEER_BY_DRAG_TIMEOUT_MS: u64 = 1000;

/// Network, API authentication, and UI rate-limiting configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub api_id: i32,
    pub api_hash: String,
    pub max_selected_items: usize,
    pub auto_search_timeout_ms: u64,
    pub preload_heights_count: usize,
    pub search_people_limit: usize,
    pub update_delay_secs: u64,
    pub wrong_passcode_timeout_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_id: DEFAULT_API_ID,
            api_hash: DEFAULT_API_HASH.to_string(),
            max_selected_items: MAX_SELECTED_ITEMS,
            auto_search_timeout_ms: AUTO_SEARCH_TIMEOUT_MS,
            preload_heights_count: PRELOAD_HEIGHTS_COUNT,
            search_people_limit: SEARCH_PEOPLE_LIMIT,
            update_delay_secs: UPDATE_DELAY_CONST_SECS,
            wrong_passcode_timeout_ms: WRONG_PASSCODE_TIMEOUT_MS,
        }
    }
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_production(&self) -> bool {
        self.api_id != 0 && !self.api_hash.is_empty()
    }
}
