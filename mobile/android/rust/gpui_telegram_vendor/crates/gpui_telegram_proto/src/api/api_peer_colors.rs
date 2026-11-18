//! Telegram Desktop 1:1 Parity Submodule: `ApiPeerColors`
//! Subsystem: `api` | Feature ID: `TCG-N11`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_peer_colors.cpp, Telegram/SourceFiles/api/api_peer_colors.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileColorOption {
    pub color_index: u8,
    pub background_emoji_id: Option<u64>,
    pub required_boost_level_channel: i32,
    pub required_boost_level_group: i32,
    pub is_hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiPeerColors {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub suggested_indices: Vec<u8>,
    pub profile_indices: Vec<u8>,
    pub required_levels_channel: HashMap<u8, i32>,
    pub required_levels_group: HashMap<u8, i32>,
    pub profile_colors: HashMap<u8, ProfileColorOption>,
    pub updated_at: i64,
}

impl Default for ApiPeerColors {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiPeerColors".to_string(),
            suggested_indices: (0..7).collect(),
            profile_indices: (0..7).collect(),
            required_levels_channel: HashMap::new(),
            required_levels_group: HashMap::new(),
            profile_colors: HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiPeerColors {
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

    pub fn required_level_for(&self, color_index: u8, is_megagroup: bool) -> i32 {
        if is_megagroup {
            self.required_levels_group.get(&color_index).copied().unwrap_or(0)
        } else {
            self.required_levels_channel.get(&color_index).copied().unwrap_or(0)
        }
    }

    pub fn register_color_option(&mut self, option: ProfileColorOption) {
        self.required_levels_channel.insert(option.color_index, option.required_boost_level_channel);
        self.required_levels_group.insert(option.color_index, option.required_boost_level_group);
        self.profile_colors.insert(option.color_index, option);
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
