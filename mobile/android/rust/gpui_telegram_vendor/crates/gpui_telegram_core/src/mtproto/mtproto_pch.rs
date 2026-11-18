//! MTProto Precompiled Constants and Prelude
//! Upstream Source: `Telegram/SourceFiles/mtproto/mtproto_pch.h`

use serde::{Deserialize, Serialize};

pub const CURRENT_LAYER: u32 = 188;
pub const MIN_LAYER: u32 = 18;
pub const MAX_CONTAINER_SIZE: usize = 1024;
pub const MAX_PACKET_SIZE: usize = 2 * 1024 * 1024; // 2MB
pub const DEFAULT_PING_INTERVAL_MS: u64 = 60_000;
pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 15_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoPch {
    pub current_layer: u32,
    pub min_layer: u32,
    pub max_packet_size: usize,
    pub ping_interval_ms: u64,
}

impl Default for MtprotoPch {
    fn default() -> Self {
        Self {
            current_layer: CURRENT_LAYER,
            min_layer: MIN_LAYER,
            max_packet_size: MAX_PACKET_SIZE,
            ping_interval_ms: DEFAULT_PING_INTERVAL_MS,
        }
    }
}

impl MtprotoPch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.current_layer >= self.min_layer
    }

    pub fn is_supported_layer(&self, layer: u32) -> bool {
        layer >= self.min_layer && layer <= self.current_layer
    }
}
