use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusResponse {
    pub server: String,
    pub status: String,
    pub version: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthResponse {
    pub user_id: u64,
    pub username: String,
    pub session_id: String,
    pub token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_generation: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhoneSendCodeRequest {
    pub phone: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhoneSendCodeResponse {
    pub ok: bool,
    pub sent: bool,
    pub phone_code_hash: String,
    pub expires_at_ms: i64,
    pub resend_after_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhoneVerifyRequest {
    pub phone: String,
    #[serde(default)]
    pub phone_code_hash: Option<String>,
    pub code: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwoFactorStatusResponse {
    pub has_password: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_email_pattern: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwoFactorSetRequest {
    #[serde(default)]
    pub current_password: Option<String>,
    pub new_password: String,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub recovery_email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwoFactorVerifyRequest {
    pub challenge_token: String,
    pub cloud_password: String,
    #[serde(default)]
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QrPairInitResponse {
    pub pair_token: String,
    pub qr_url: String,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QrPairAcceptRequest {
    pub pair_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QrPairPollResponse {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pair_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<AuthResponse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<UserProfileDto>,
}

pub type QrPairStatusResponse = QrPairPollResponse;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDto {
    pub session_id: String,
    pub device_name: String,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub ip_address: Option<String>,
    pub created_at_ms: i64,
    pub last_active_ms: i64,
    pub is_current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UpdateProfileRequest {
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default, alias = "photo_file_id")]
    pub avatar_media_id: Option<String>,
    #[serde(default)]
    pub emoji_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserProfileDto {
    pub user_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    pub first_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_media_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo_file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji_status: Option<String>,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(default)]
    pub is_verified: bool,
    #[serde(default)]
    pub is_premium: bool,
    #[serde(default)]
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactDto {
    pub user_id: u64,
    pub first_name: String,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(default)]
    pub is_mutual: bool,
    #[serde(default)]
    pub chat_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockedUserDto {
    pub user_id: u64,
    pub first_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(default)]
    pub blocked_at_ms: i64,
}

fn default_ttl_months() -> u32 {
    6
}
fn default_true() -> bool {
    true
}
fn default_p2p_mode() -> String {
    "Contacts".to_string()
}
fn default_privacy_contacts() -> String {
    "Contacts".to_string()
}
fn default_privacy_everybody() -> String {
    "Everybody".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserPrivacySettingsDto {
    #[serde(default = "default_privacy_contacts")]
    pub phone_privacy: String,
    #[serde(default = "default_privacy_everybody")]
    pub last_seen_privacy: String,
    #[serde(default = "default_privacy_everybody")]
    pub photo_privacy: String,
    #[serde(default = "default_privacy_everybody")]
    pub calls_privacy: String,
    #[serde(default = "default_privacy_everybody")]
    pub forwards_privacy: String,
    #[serde(default = "default_privacy_everybody")]
    pub groups_privacy: String,
    #[serde(default = "default_ttl_months")]
    pub ttl_months: u32,
    #[serde(default)]
    pub global_ttl_seconds: u32,
    #[serde(default)]
    pub sensitive_content_enabled: bool,
    #[serde(default)]
    pub passcode_enabled: bool,
    #[serde(default = "default_true", alias = "notify_private_chats")]
    pub notifications_private: bool,
    #[serde(default = "default_true", alias = "notify_groups")]
    pub notifications_groups: bool,
    #[serde(default, alias = "notify_channels")]
    pub notifications_channels: bool,
    #[serde(default = "default_true", alias = "notify_sound")]
    pub notifications_sound: bool,
    #[serde(default)]
    pub include_muted_in_badge: bool,
    #[serde(default = "default_true")]
    pub badge_count_messages: bool,
    #[serde(default = "default_p2p_mode")]
    pub p2p_mode: String,
    #[serde(default = "default_true")]
    pub accept_calls: bool,
    #[serde(default)]
    pub is_premium: bool,
}

impl Default for UserPrivacySettingsDto {
    fn default() -> Self {
        Self {
            phone_privacy: "Contacts".to_string(),
            last_seen_privacy: "Everybody".to_string(),
            photo_privacy: "Everybody".to_string(),
            calls_privacy: "Everybody".to_string(),
            forwards_privacy: "Everybody".to_string(),
            groups_privacy: "Everybody".to_string(),
            ttl_months: 6,
            global_ttl_seconds: 0,
            sensitive_content_enabled: false,
            passcode_enabled: false,
            notifications_private: true,
            notifications_groups: true,
            notifications_channels: false,
            notifications_sound: true,
            include_muted_in_badge: false,
            badge_count_messages: true,
            p2p_mode: "Contacts".to_string(),
            accept_calls: true,
            is_premium: false,
        }
    }
}
