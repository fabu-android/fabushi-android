use serde::{Deserialize, Serialize};

pub const USER_VAULT_CATEGORY_PASSWORD: &str = "password";
pub const USER_VAULT_CATEGORY_BROWSER_COOKIE: &str = "browser_cookie";
pub const USER_VAULT_CATEGORY_AI_OAUTH: &str = "ai_oauth";

pub const USER_VAULT_KDF_ALG_ARGON2ID: &str = "argon2id";
pub const USER_VAULT_KDF_ALG_SPLIT_KEY_V2: &str = "split_key_v2";
pub const USER_VAULT_KDF_ALG_DEVICE_KEYCHAIN_V2: &str = "device_keychain_v2";
pub const DEVICE_SYNC_CODE_DOMAIN_V1: &[u8] = b"gpui-telegram/device-sync-code/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserVaultKeyEnvelopeDto {
    pub key_version: u32,
    pub kdf_alg: String,
    pub kdf_salt_b64: String,
    pub kdf_m_cost_kib: u32,
    pub kdf_t_cost: u32,
    pub kdf_p_cost: u32,
    pub wrapped_root_key_b64: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_wrapped_root_key_b64: Option<String>,
    #[serde(default)]
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserVaultItemDto {
    pub category: String,
    pub item_key: String,
    pub revision: i64,
    pub nonce_b64: String,
    pub ciphertext_b64: String,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertUserVaultItemRequest {
    #[serde(default)]
    pub expected_revision: Option<i64>,
    pub nonce_b64: String,
    pub ciphertext_b64: String,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertUserVaultItemResponse {
    pub ok: bool,
    pub category: String,
    pub item_key: String,
    pub revision: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserVaultEnvelopeResponse {
    pub envelope: Option<UserVaultKeyEnvelopeDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserVaultSyncPullResponse {
    pub latest_revision: i64,
    pub items: Vec<UserVaultItemDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveLocalBotReplyRequest {
    pub bot_user_id: i64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to_msg_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_nonce: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveLocalBotReplyResponse {
    pub message_id: i64,
    pub pts: u64,
    pub date_ms: i64,
}

/// Argon2id KDF cost parameters exchanged during Split-Key V2 authentication and key wrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Argon2KdfParamsDto {
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl Default for Argon2KdfParamsDto {
    fn default() -> Self {
        Self {
            m_cost_kib: 65_536,
            t_cost: 3,
            p_cost: 1,
        }
    }
}

/// Query payload for `GET /api/v1/auth/kdf-params?username=<str>`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct KdfParamsQuery {
    #[serde(default, alias = "username")]
    pub username_or_email: String,
}

/// Response returned by `GET /api/v1/auth/kdf-params`.
/// Missing users receive `auth_version = 2` with a deterministic HMAC fake salt to prevent enumeration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParamsResponse {
    pub auth_version: i32,
    pub auth_salt_b64: String,
    pub kdf_params: Argon2KdfParamsDto,
}

/// Wire-compatible registration payload supporting Split-Key V2 (`auth_verifier_b64`, zero raw password)
/// and V1 fallback (`password`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SplitKeyRegisterRequest {
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default)]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(default)]
    pub device_name: String,
    #[serde(default)]
    pub auto_welcome: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_version: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_salt_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_verifier_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kdf_params: Option<Argon2KdfParamsDto>,
}

/// Wire-compatible email registration payload supporting Split-Key V2 (`auth_verifier_b64`, zero raw password).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SplitKeyEmailRegisterRequest {
    pub email: String,
    pub code: String,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default)]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(default)]
    pub device_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_version: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_salt_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_verifier_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kdf_params: Option<Argon2KdfParamsDto>,
}

/// Wire-compatible login payload supporting Split-Key V2 (`auth_verifier_b64` + empty `password`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SplitKeyLoginRequest {
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub device_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_verifier_b64: Option<String>,
}

/// Authenticated payload for silent V1 -> Split-Key V2 upgrade (`POST /api/v1/auth/upgrade-split-key`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpgradeSplitKeyRequest {
    pub old_password: String,
    pub auth_salt_b64: String,
    pub auth_verifier_b64: String,
    pub kdf_params: Argon2KdfParamsDto,
}

/// Authenticated payload for password rotation (`POST /api/v1/auth/change-password`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangePasswordRequest {
    pub old_secret: String,
    pub auth_version: i32,
    pub new_auth_salt_b64: String,
    pub new_auth_verifier_b64: String,
    pub new_kdf_params: Argon2KdfParamsDto,
}

/// Response returned after upgrading to Split-Key V2 or changing password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthCustodyUpdateResponse {
    pub ok: bool,
    pub auth_version: i32,
}

/// Initiates an X25519 Device Trust Ring sync request (`POST /api/v1/user-vault/device-sync/request`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceSyncRequestBody {
    pub device_name: String,
    pub device_public_key_b64: String,
}

/// Response returned when creating a new Device Trust Ring sync request (timestamps are Unix epoch seconds).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceSyncRequestResponse {
    pub request_id: String,
    pub verification_code: String,
    pub expires_at: i64,
}

/// Pending Device Trust Ring approval entry returned to trusted devices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingDeviceSyncItem {
    pub request_id: String,
    pub device_name: String,
    pub device_public_key_b64: String,
    pub verification_code: String,
    pub status: String,
    pub created_at: i64,
    pub expires_at: i64,
}

/// List response returned by `GET /api/v1/user-vault/device-sync/pending`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingDeviceSyncListResponse {
    pub items: Vec<PendingDeviceSyncItem>,
}

/// Payload sent by an approving trusted device (`POST /api/v1/user-vault/device-sync/{request_id}/approve`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApproveDeviceSyncRequest {
    pub approver_ephemeral_pub_b64: String,
    pub encrypted_root_key_b64: String,
    pub nonce_b64: String,
}

/// Action acknowledgment returned by `approve` and `deny` endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceSyncActionResponse {
    pub ok: bool,
    pub request_id: String,
    pub status: String,
}

/// Polling response returned by `GET /api/v1/user-vault/device-sync/{request_id}/status`.
/// On the first `"approved"` read, ciphertext columns are atomically wiped in SQLite (`status = "consumed"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceSyncStatusResponse {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approver_ephemeral_pub_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_root_key_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce_b64: Option<String>,
    pub expires_at: i64,
}
