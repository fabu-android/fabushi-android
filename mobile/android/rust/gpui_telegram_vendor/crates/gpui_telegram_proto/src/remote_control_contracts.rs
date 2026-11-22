//! Remote desktop control, remote assist, persistent PTY terminal, streaming settings,
//! file transfer, local IPC, and official MCP OAuth grant contracts.

use crate::tailnet_contracts::TailnetRelayMode;
use serde::{Deserialize, Serialize};

fn default_jsonrpc_version() -> String {
    "2.0".to_string()
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Classification of the actor controlling a computer node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RemoteControllerKind {
    #[default]
    OwnDevice,
    AssistGuest,
    AiMcp,
    Cli,
}

impl RemoteControllerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OwnDevice => "own_device",
            Self::AssistGuest => "assist_guest",
            Self::AiMcp => "ai_mcp",
            Self::Cli => "cli",
        }
    }

    pub fn label_zh(&self) -> &'static str {
        match self {
            Self::OwnDevice => "本账号设备",
            Self::AssistGuest => "远程协助设备",
            Self::AiMcp => "AI Agent (MCP)",
            Self::Cli => "本地 CLI",
        }
    }

    pub fn label_en(&self) -> &'static str {
        match self {
            Self::OwnDevice => "Own Device",
            Self::AssistGuest => "Assist Guest",
            Self::AiMcp => "AI Agent (MCP)",
            Self::Cli => "Local CLI",
        }
    }
}

/// Active remote control modality for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RemoteControlMode {
    #[default]
    Desktop,
    Terminal,
    FileTransfer,
    AiControl,
}

impl RemoteControlMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Terminal => "terminal",
            Self::FileTransfer => "file_transfer",
            Self::AiControl => "ai_control",
        }
    }

    pub fn label_zh(&self) -> &'static str {
        match self {
            Self::Desktop => "远程桌面",
            Self::Terminal => "远程终端",
            Self::FileTransfer => "文件传输",
            Self::AiControl => "AI 操控",
        }
    }

    pub fn label_en(&self) -> &'static str {
        match self {
            Self::Desktop => "Remote Desktop",
            Self::Terminal => "Remote Terminal",
            Self::FileTransfer => "File Transfer",
            Self::AiControl => "AI Control",
        }
    }
}

/// Live active remote-control session tracked by host control hub and coordinator server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RemoteSessionDto {
    pub session_id: String,
    pub controller_kind: RemoteControllerKind,
    pub controller_name: String,
    pub controller_platform: String,
    pub controller_id: String,
    pub target_node_id: String,
    pub target_device_id: String,
    pub mode: RemoteControlMode,
    pub started_at_ms: i64,
    pub last_activity_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_tool: Option<String>,
    pub relay_mode: TailnetRelayMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtt_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fps: Option<u32>,
}

/// Verification code policy for cross-account Remote Assist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AssistCodeMode {
    #[default]
    Temporary,
    Custom,
}

/// Device's Remote Assist configuration (permanent 9-digit assist ID + temporary/custom code).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RemoteAssistConfigDto {
    /// Permanent 9-digit device assist identifier (e.g. "482910375").
    pub assist_id: String,
    pub allow_assist: bool,
    pub code_mode: AssistCodeMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<i64>,
}

/// Request to start a cross-account Remote Assist session via 9-digit ID + verification code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AssistStartRequest {
    pub assist_id: String,
    pub access_code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requester_device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requester_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requester_platform: Option<String>,
    pub mode: RemoteControlMode,
}

/// Response returned after validating a Remote Assist ID + code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AssistStartResponse {
    pub session_id: String,
    pub target_node_id: String,
    pub target_device_id: String,
    pub temp_relay_token: String,
    pub requires_host_confirm: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<i64>,
}

/// Request payload for `POST /api/v1/remote/assist/allow`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AssistAllowRequest {
    pub allow_assist: bool,
}

/// Request payload for `POST /api/v1/remote/assist/set-code`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AssistSetCodeRequest {
    pub code: String,
}

/// Request payload for `POST /api/v1/remote/sessions/disconnect`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RemoteSessionDisconnectRequest {
    pub session_id: String,
}

/// Response payload for `GET /api/v1/remote/sessions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RemoteSessionsResponse {
    pub sessions: Vec<RemoteSessionDto>,
}

/// Preferred video codec for remote desktop streaming.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StreamCodec {
    #[default]
    Auto,
    H264,
    H265,
    Vp8,
    JpegFallback,
}

/// Stream visual quality preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StreamQuality {
    #[default]
    Auto,
    Smooth,
    Hd,
    Ultra4K,
    Original,
}

/// Chroma subsampling mode for remote desktop video.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ColorSubsampling {
    #[default]
    Yuv420,
    TrueColor444,
}

/// Streaming, audio, privacy, and input tuning configuration for a remote desktop session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteStreamSettingsDto {
    pub codec: StreamCodec,
    pub target_fps: u32,
    pub quality: StreamQuality,
    pub color_mode: ColorSubsampling,
    pub hdr: bool,
    pub audio_enabled: bool,
    pub mute_remote_speaker: bool,
    pub mic_passthrough: bool,
    pub privacy_screen: bool,
    pub relative_mouse: bool,
    pub lock_cursor_in_window: bool,
    pub macos_hotkey_mapping: bool,
    pub display_index: u32,
}

impl Default for RemoteStreamSettingsDto {
    fn default() -> Self {
        Self {
            codec: StreamCodec::Auto,
            target_fps: 60,
            quality: StreamQuality::Auto,
            color_mode: ColorSubsampling::Yuv420,
            hdr: false,
            audio_enabled: true,
            mute_remote_speaker: false,
            mic_passthrough: false,
            privacy_screen: false,
            relative_mouse: false,
            lock_cursor_in_window: false,
            macos_hotkey_mapping: true,
            display_index: 0,
        }
    }
}

/// Physical or virtual monitor descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DisplayInfoDto {
    pub display_id: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub scale_factor_x100: u32,
    pub is_primary: bool,
    pub is_virtual: bool,
    pub refresh_hz: u32,
}

/// Persistent PTY multiplexer session descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TerminalSessionDto {
    pub session_id: String,
    pub name: String,
    pub shell: String,
    pub cols: u16,
    pub rows: u16,
    pub attached_clients: u32,
    pub created_at_ms: i64,
    pub last_activity_ms: i64,
    pub alive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// Non-interactive or sentinel-framed command execution output inside a persistent PTY.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TerminalExecResultDto {
    pub session_id: String,
    pub exit_code: i32,
    pub output: String,
    pub timed_out: bool,
}

/// Conflict resolution strategy when transferring files to an existing destination path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FileConflictPolicy {
    #[default]
    Overwrite,
    Skip,
    Rename,
    ApplyAll,
}

/// Transfer direction between local initiator and remote host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FileTransferDirection {
    #[default]
    Upload,
    Download,
}

/// Lifecycle state of a chunked resumable file transfer task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FileTransferStatus {
    #[default]
    Queued,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// Resumable chunked file transfer descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FileTransferTaskDto {
    pub transfer_id: String,
    pub direction: FileTransferDirection,
    pub local_path: String,
    pub remote_path: String,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub chunk_size: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    pub status: FileTransferStatus,
    pub conflict_policy: FileConflictPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// Power management and Wake-on-LAN actions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PowerActionKind {
    Lock,
    Sleep,
    Reboot,
    Shutdown,
    ScheduleShutdown {
        delay_minutes: u32,
    },
    WakeOnLan {
        mac_address: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        broadcast_ip: Option<String>,
    },
}

impl Default for PowerActionKind {
    fn default() -> Self {
        Self::Lock
    }
}

/// Clipboard synchronization payload carrying loop-prevention origin tag and monotonic sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ClipboardPayloadDto {
    pub mime_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_b64: Option<String>,
    pub origin_id: String,
    pub seq: u64,
}

/// Structured error payload inside `LocalIpcEnvelope` and `LocalIpcResponse`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LocalIpcError {
    pub code: String,
    pub message: String,
}

impl LocalIpcError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Uniform CLI / Local IPC envelope `{ success, data, error, timestamp }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LocalIpcEnvelope<T> {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<LocalIpcError>,
    pub timestamp: String,
}

impl<T> LocalIpcEnvelope<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: now_rfc3339(),
        }
    }

    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(LocalIpcError::new(code, message)),
            timestamp: now_rfc3339(),
        }
    }
}

/// Local IPC JSON-RPC 2.0 request frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalIpcRequest {
    #[serde(default = "default_jsonrpc_version")]
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

impl Default for LocalIpcRequest {
    fn default() -> Self {
        Self {
            jsonrpc: default_jsonrpc_version(),
            id: String::new(),
            method: String::new(),
            params: serde_json::Value::Null,
        }
    }
}

impl LocalIpcRequest {
    pub fn new(
        id: impl Into<String>,
        method: impl Into<String>,
        params: serde_json::Value,
    ) -> Self {
        Self {
            jsonrpc: default_jsonrpc_version(),
            id: id.into(),
            method: method.into(),
            params,
        }
    }
}

/// Local IPC JSON-RPC 2.0 response frame carrying the uniform envelope fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalIpcResponse {
    #[serde(default = "default_jsonrpc_version")]
    pub jsonrpc: String,
    pub id: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<LocalIpcError>,
    pub timestamp: String,
}

impl Default for LocalIpcResponse {
    fn default() -> Self {
        Self {
            jsonrpc: default_jsonrpc_version(),
            id: String::new(),
            success: false,
            data: None,
            error: None,
            timestamp: now_rfc3339(),
        }
    }
}

impl LocalIpcResponse {
    pub fn ok(id: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            jsonrpc: default_jsonrpc_version(),
            id: id.into(),
            success: true,
            data: Some(data),
            error: None,
            timestamp: now_rfc3339(),
        }
    }

    pub fn err(
        id: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            jsonrpc: default_jsonrpc_version(),
            id: id.into(),
            success: false,
            data: None,
            error: Some(LocalIpcError::new(code, message)),
            timestamp: now_rfc3339(),
        }
    }

    pub fn into_envelope(self) -> LocalIpcEnvelope<serde_json::Value> {
        LocalIpcEnvelope {
            success: self.success,
            data: self.data,
            error: self.error,
            timestamp: self.timestamp,
        }
    }

    pub fn from_envelope(
        id: impl Into<String>,
        env: LocalIpcEnvelope<serde_json::Value>,
    ) -> Self {
        Self {
            jsonrpc: default_jsonrpc_version(),
            id: id.into(),
            success: env.success,
            data: env.data,
            error: env.error,
            timestamp: env.timestamp,
        }
    }
}

/// Summary of an active OAuth 2.1 token-family grant issued to an AI MCP client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct McpOAuthGrantSummaryDto {
    pub family_id: String,
    pub client_id: String,
    pub client_name: String,
    pub scopes: Vec<String>,
    pub allowed_node_ids: Vec<String>,
    pub created_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_ms: Option<i64>,
    pub expires_at_ms: i64,
}

/// Response payload for `GET /api/v1/mcp/grants`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct McpOAuthGrantsResponse {
    pub grants: Vec<McpOAuthGrantSummaryDto>,
}

/// Snapshot node of the host accessibility tree returned by `RemoteComputerOp::AccessibilityTree`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AccessibilityNodeDto {
    pub element_id: String,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<[i32; 4]>,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub children: Vec<AccessibilityNodeDto>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_session_and_assist_roundtrip() {
        let session = RemoteSessionDto {
            session_id: "rs_1".into(),
            controller_kind: RemoteControllerKind::AiMcp,
            controller_name: "Antigravity".into(),
            controller_platform: "macos".into(),
            controller_id: "client_1".into(),
            target_node_id: "local:1:mac".into(),
            target_device_id: "mac-studio".into(),
            mode: RemoteControlMode::AiControl,
            started_at_ms: 1_700_000_000_000,
            last_activity_ms: 1_700_000_001_000,
            active_tool: Some("capture_screen".into()),
            relay_mode: TailnetRelayMode::DirectUdp,
            rtt_ms: Some(12),
            fps: Some(60),
        };
        let json = serde_json::to_string(&session).unwrap();
        let parsed: RemoteSessionDto = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, session);

        let assist = RemoteAssistConfigDto {
            assist_id: "482910375".into(),
            allow_assist: true,
            code_mode: AssistCodeMode::Custom,
            current_code: Some("Abc12345".into()),
            expires_at_ms: None,
        };
        let ajson = serde_json::to_string(&assist).unwrap();
        let aparsed: RemoteAssistConfigDto = serde_json::from_str(&ajson).unwrap();
        assert_eq!(aparsed, assist);
    }

    #[test]
    fn test_power_action_and_ipc_envelope_roundtrip() {
        let wol = PowerActionKind::WakeOnLan {
            mac_address: "AA:BB:CC:DD:EE:FF".into(),
            broadcast_ip: Some("192.168.1.255".into()),
        };
        let val = serde_json::to_value(&wol).unwrap();
        assert_eq!(val["kind"], "wake_on_lan");
        let parsed: PowerActionKind = serde_json::from_value(val).unwrap();
        assert_eq!(parsed, wol);

        let env = LocalIpcEnvelope::ok(serde_json::json!({ "alive": true }));
        let resp = LocalIpcResponse::from_envelope("req-1", env.clone());
        assert!(resp.success);
        assert_eq!(resp.id, "req-1");
        assert_eq!(resp.into_envelope().data, env.data);
    }
}
