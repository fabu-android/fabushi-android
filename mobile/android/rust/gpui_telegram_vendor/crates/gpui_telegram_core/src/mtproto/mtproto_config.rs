//! MTProto Configuration Model (`help.getConfig` representation)
//! Upstream Source: `Telegram/SourceFiles/mtproto/mtproto_config.h`, `mtproto_config.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoConfig {
    pub date: i32,
    pub expires: i32,
    pub test_mode: bool,
    pub this_dc: i32,
    pub dc_txt_domain_name: String,
    pub chat_size_max: i32,
    pub megagroup_size_max: i32,
    pub forwarded_count_max: i32,
    pub online_update_period_ms: i32,
    pub offline_blur_timeout_ms: i32,
    pub offline_idle_timeout_ms: i32,
    pub online_cloud_timeout_ms: i32,
    pub notify_cloud_delay_ms: i32,
    pub notify_default_delay_ms: i32,
    pub push_chat_period_ms: i32,
    pub push_chat_limit: i32,
    pub saved_gifs_limit: i32,
    pub edit_time_limit: i32,
    pub revoke_time_limit: i32,
    pub revoke_private_time_limit: i32,
    pub caption_length_max: i32,
}

impl Default for MtprotoConfig {
    fn default() -> Self {
        Self {
            date: 0,
            expires: 0,
            test_mode: false,
            this_dc: 2,
            dc_txt_domain_name: "dns.telegram.org".to_string(),
            chat_size_max: 200,
            megagroup_size_max: 200_000,
            forwarded_count_max: 100,
            online_update_period_ms: 120_000,
            offline_blur_timeout_ms: 5_000,
            offline_idle_timeout_ms: 30_000,
            online_cloud_timeout_ms: 300_000,
            notify_cloud_delay_ms: 30_000,
            notify_default_delay_ms: 1_500,
            push_chat_period_ms: 60_000,
            push_chat_limit: 10,
            saved_gifs_limit: 200,
            edit_time_limit: 172_800, // 48 hours
            revoke_time_limit: 172_800,
            revoke_private_time_limit: i32::MAX, // unlimited
            caption_length_max: 1024,
        }
    }
}

impl MtprotoConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.this_dc > 0 && self.chat_size_max > 0
    }

    pub fn is_expired(&self, current_unixtime: i32) -> bool {
        self.expires > 0 && current_unixtime >= self.expires
    }
}
