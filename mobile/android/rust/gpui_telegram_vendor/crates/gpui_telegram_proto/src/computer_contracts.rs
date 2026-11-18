//! Computer & Cloud Server control-plane contracts.
//!
//! Defines device registration, cloud server/sandbox configuration, authorization policies,
//! bot-to-computer bindings, and remote operation/approval DTOs shared across proto, client,
//! server, and desktop crates.

use serde::{Deserialize, Serialize};

/// Classification of a controllable computer node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ComputerNodeKind {
    #[default]
    LocalDevice,
    CloudServer,
    CloudSandbox,
}

impl ComputerNodeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LocalDevice => "local_device",
            Self::CloudServer => "cloud_server",
            Self::CloudSandbox => "cloud_sandbox",
        }
    }

    pub fn default_run_policy(&self) -> ComputerRunPolicy {
        ComputerRunPolicy::default_for_kind(*self)
    }

    pub fn default_tier_badge(&self) -> &'static str {
        match self {
            Self::LocalDevice => "本地",
            Self::CloudServer => "云电脑",
            Self::CloudSandbox => "免费",
        }
    }
}

/// Execution authorization policy for a computer node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ComputerRunPolicy {
    #[default]
    AskEveryTime,
    AllowReadOnly,
    AlwaysAllowAuthorized,
    Disabled,
}

impl ComputerRunPolicy {
    pub fn default_for_kind(kind: ComputerNodeKind) -> Self {
        match kind {
            ComputerNodeKind::LocalDevice => Self::AskEveryTime,
            ComputerNodeKind::CloudServer | ComputerNodeKind::CloudSandbox => {
                Self::AlwaysAllowAuthorized
            }
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AskEveryTime => "ask_every_time",
            Self::AllowReadOnly => "allow_read_only",
            Self::AlwaysAllowAuthorized => "always_allow_authorized",
            Self::Disabled => "disabled",
        }
    }

    pub fn label_zh(&self) -> &'static str {
        match self {
            Self::AskEveryTime => "每次询问",
            Self::AllowReadOnly => "自动允许只读",
            Self::AlwaysAllowAuthorized => "始终允许已授权",
            Self::Disabled => "禁止运行",
        }
    }

    pub fn label_en(&self) -> &'static str {
        match self {
            Self::AskEveryTime => "Ask Every Time",
            Self::AllowReadOnly => "Allow Read-Only",
            Self::AlwaysAllowAuthorized => "Always Allow Authorized",
            Self::Disabled => "Disabled",
        }
    }
}

/// Credential/transport method used to connect to a cloud computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CloudAuthMethod {
    #[default]
    SshPassword,
    SshPrivateKey,
    AgentHttpToken,
}

impl CloudAuthMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SshPassword => "ssh_password",
            Self::SshPrivateKey => "ssh_private_key",
            Self::AgentHttpToken => "agent_http_token",
        }
    }
}

/// Type of entity authorized to issue remote commands to a computer node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizedControllerKind {
    #[default]
    Device,
    Bot,
    AccountAllBots,
}

/// Authorized controller entry attached to a computer node's ACL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AuthorizedControllerEntry {
    pub controller_id: String,
    pub kind: AuthorizedControllerKind,
    pub display_name: String,
    pub added_at_ms: i64,
}

/// Public view of a registered local device, cloud server, or cloud sandbox.
/// Raw credentials are never exposed in this DTO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ComputerNodeDto {
    pub node_id: String,
    pub owner_user_id: u64,
    pub kind: ComputerNodeKind,
    pub display_name: String,
    pub ip_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<CloudAuthMethod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_endpoint_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workdir: Option<String>,
    pub has_saved_credential: bool,
    pub os_name: String,
    pub resolution: String,
    pub online: bool,
    pub is_current_device: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    pub tier_badge: String,
    pub run_policy: ComputerRunPolicy,
    pub authorized_controllers: Vec<AuthorizedControllerEntry>,
    pub bound_bot_ids: Vec<u64>,
    pub last_seen_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_frame_data_url: Option<String>,
}

/// Request payload for registering or heartbeating the current local device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RegisterLocalComputerRequest {
    pub device_id: String,
    pub display_name: String,
    pub ip_address: String,
    pub os_name: String,
    pub resolution: String,
}

/// Request payload for creating or updating a cloud server node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UpsertCloudComputerRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub display_name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: CloudAuthMethod,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_endpoint_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workdir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind_bot_id: Option<u64>,
    pub verify_connection: bool,
}

/// Request payload for updating a computer node's run policy, display name, or authorized controllers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UpdateComputerPolicyRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_policy: Option<ComputerRunPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub add_controller: Option<AuthorizedControllerEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove_controller_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Bot-level computer binding and execution preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BotComputerProfileDto {
    pub bot_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bound_node_id: Option<String>,
    pub persona_prompt: String,
    pub preferred_model: String,
    pub approval_mode: ComputerRunPolicy,
    pub allow_cross_device_control: bool,
    pub allow_cloud_self_config: bool,
    pub updated_at_ms: i64,
}

/// Request payload for updating a bot's bound computer and persona/approval profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UpdateBotComputerRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bound_node_id: Option<String>,
    pub clear_bound_node: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_mode: Option<ComputerRunPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_cross_device_control: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_cloud_self_config: Option<bool>,
}

/// Operation to execute on a target computer node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RemoteComputerOp {
    ShellExec {
        command: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },
    ReadFile {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_bytes: Option<usize>,
    },
    WriteFile {
        path: String,
        content: String,
        #[serde(default)]
        create_dirs: bool,
    },
    ListDir {
        path: String,
    },
    CaptureScreen,
    GuiInput {
        action: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        x: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        y: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    #[default]
    TestConnection,
}

impl RemoteComputerOp {
    /// Returns true when the operation only inspects state without mutating files, processes, or GUI.
    pub fn is_read_only(&self) -> bool {
        matches!(
            self,
            Self::ReadFile { .. }
                | Self::ListDir { .. }
                | Self::CaptureScreen
                | Self::TestConnection
        )
    }
}

/// Request payload for executing a remote operation on a computer node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RemoteComputerCommandRequest {
    pub node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_bot_id: Option<u64>,
    pub caller_label: String,
    pub operation: RemoteComputerOp,
}

/// Queued command waiting for local host polling or user approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PendingComputerCommandDto {
    pub command_id: String,
    pub node_id: String,
    pub target_device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_bot_id: Option<u64>,
    pub caller_label: String,
    pub requires_user_approval: bool,
    pub approved: bool,
    pub operation: RemoteComputerOp,
    pub created_at_ms: i64,
}

/// Approval decision payload for a pending remote computer command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ApproveComputerCommandRequest {
    pub approved: bool,
}

/// Result returned after executing a remote computer operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RemoteComputerCommandResult {
    pub command_id: String,
    pub node_id: String,
    pub ok: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub entries: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_data_url: Option<String>,
    pub executed_by: String,
    pub duration_ms: u64,
}

/// Combined response for `GET /api/v1/computers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ComputerListResponse {
    pub computers: Vec<ComputerNodeDto>,
    pub pending_approvals: Vec<PendingComputerCommandDto>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_computer_run_policy_roundtrip_and_labels() {
        let cases = [
            (
                ComputerRunPolicy::AskEveryTime,
                "\"ask_every_time\"",
                "每次询问",
                "Ask Every Time",
            ),
            (
                ComputerRunPolicy::AllowReadOnly,
                "\"allow_read_only\"",
                "自动允许只读",
                "Allow Read-Only",
            ),
            (
                ComputerRunPolicy::AlwaysAllowAuthorized,
                "\"always_allow_authorized\"",
                "始终允许已授权",
                "Always Allow Authorized",
            ),
            (
                ComputerRunPolicy::Disabled,
                "\"disabled\"",
                "禁止运行",
                "Disabled",
            ),
        ];

        for (policy, expected_json, expected_zh, expected_en) in cases {
            let json = serde_json::to_string(&policy).expect("serialize policy");
            assert_eq!(json, expected_json);
            let parsed: ComputerRunPolicy = serde_json::from_str(&json).expect("deserialize policy");
            assert_eq!(parsed, policy);
            assert_eq!(parsed.label_zh(), expected_zh);
            assert_eq!(parsed.label_en(), expected_en);
        }

        assert_eq!(
            ComputerRunPolicy::default_for_kind(ComputerNodeKind::LocalDevice),
            ComputerRunPolicy::AskEveryTime
        );
        assert_eq!(
            ComputerRunPolicy::default_for_kind(ComputerNodeKind::CloudServer),
            ComputerRunPolicy::AlwaysAllowAuthorized
        );
        assert_eq!(
            ComputerRunPolicy::default_for_kind(ComputerNodeKind::CloudSandbox),
            ComputerRunPolicy::AlwaysAllowAuthorized
        );
    }

    #[test]
    fn test_cloud_auth_method_roundtrip() {
        let cases = [
            (CloudAuthMethod::SshPassword, "\"ssh_password\""),
            (CloudAuthMethod::SshPrivateKey, "\"ssh_private_key\""),
            (CloudAuthMethod::AgentHttpToken, "\"agent_http_token\""),
        ];
        for (method, expected_json) in cases {
            let json = serde_json::to_string(&method).expect("serialize auth method");
            assert_eq!(json, expected_json);
            let parsed: CloudAuthMethod =
                serde_json::from_str(&json).expect("deserialize auth method");
            assert_eq!(parsed, method);
        }
    }

    #[test]
    fn test_remote_computer_op_tagged_roundtrips() {
        let ops = vec![
            RemoteComputerOp::ShellExec {
                command: "uname -a".to_string(),
                cwd: Some("/tmp".to_string()),
                timeout_ms: Some(5_000),
            },
            RemoteComputerOp::ReadFile {
                path: "/etc/hosts".to_string(),
                max_bytes: Some(4096),
            },
            RemoteComputerOp::WriteFile {
                path: "/tmp/test.txt".to_string(),
                content: "hello".to_string(),
                create_dirs: true,
            },
            RemoteComputerOp::ListDir {
                path: "/tmp".to_string(),
            },
            RemoteComputerOp::CaptureScreen,
            RemoteComputerOp::GuiInput {
                action: "click".to_string(),
                x: Some(100),
                y: Some(200),
                text: None,
                key: Some("Enter".to_string()),
            },
            RemoteComputerOp::TestConnection,
        ];

        for op in ops {
            let val = serde_json::to_value(&op).expect("serialize RemoteComputerOp");
            assert!(val.get("op").is_some(), "tagged op field must exist");
            let parsed: RemoteComputerOp =
                serde_json::from_value(val).expect("deserialize RemoteComputerOp");
            assert_eq!(parsed, op);
        }

        // Minimal JSON without optional fields should deserialize cleanly
        let minimal: RemoteComputerOp =
            serde_json::from_str(r#"{"op":"shell_exec","command":"ls"}"#)
                .expect("minimal shell_exec");
        assert_eq!(
            minimal,
            RemoteComputerOp::ShellExec {
                command: "ls".to_string(),
                cwd: None,
                timeout_ms: None,
            }
        );
        assert!(RemoteComputerOp::CaptureScreen.is_read_only());
        assert!(
            !RemoteComputerOp::WriteFile {
                path: "a".into(),
                content: "b".into(),
                create_dirs: false
            }
            .is_read_only()
        );
    }

    #[test]
    fn test_computer_node_dto_roundtrip_and_defaults() {
        let node = ComputerNodeDto {
            node_id: "cloud:42:prod-1".to_string(),
            owner_user_id: 42,
            kind: ComputerNodeKind::CloudServer,
            display_name: "Prod Ubuntu".to_string(),
            ip_address: "203.0.113.10".to_string(),
            host: Some("203.0.113.10".to_string()),
            ssh_port: Some(22),
            ssh_username: Some("root".to_string()),
            auth_method: Some(CloudAuthMethod::SshPassword),
            agent_endpoint_url: None,
            workdir: Some("/opt/app".to_string()),
            has_saved_credential: true,
            os_name: "Ubuntu 24.04".to_string(),
            resolution: "1920x1080".to_string(),
            online: true,
            is_current_device: false,
            device_id: None,
            tier_badge: "云电脑".to_string(),
            run_policy: ComputerRunPolicy::AlwaysAllowAuthorized,
            authorized_controllers: vec![AuthorizedControllerEntry {
                controller_id: "bot:777001".to_string(),
                kind: AuthorizedControllerKind::Bot,
                display_name: "OpsBot".to_string(),
                added_at_ms: 1_700_000_000_000,
            }],
            bound_bot_ids: vec![777001],
            last_seen_ms: 1_700_000_000_500,
            latest_frame_data_url: Some("data:image/png;base64,AAAA".to_string()),
        };

        let json = serde_json::to_string(&node).expect("serialize ComputerNodeDto");
        let parsed: ComputerNodeDto =
            serde_json::from_str(&json).expect("deserialize ComputerNodeDto");
        assert_eq!(parsed, node);

        // Partial JSON deserializes with safe defaults
        let partial: ComputerNodeDto =
            serde_json::from_str(r#"{"node_id":"local:1:dev-1","owner_user_id":1}"#)
                .expect("partial ComputerNodeDto");
        assert_eq!(partial.node_id, "local:1:dev-1");
        assert_eq!(partial.kind, ComputerNodeKind::LocalDevice);
        assert_eq!(partial.run_policy, ComputerRunPolicy::AskEveryTime);
        assert!(!partial.has_saved_credential);
    }
}
