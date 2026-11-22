//! Local Coding Agent Bot & Host ACP/CLI execution contracts.
//!
//! Defines provider kinds, permission modes, approval decisions, bot configuration,
//! host commands (`UpdateEvent::CodingHostCommand`), and host events
//! (`WsClientFrame::CodingHostEvent`) shared across proto, coding runtime, server,
//! client, and desktop crates.

use serde::{Deserialize, Serialize};

/// Supported local coding CLI / ACP provider backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CodingProviderKind {
    #[default]
    #[serde(alias = "claude", alias = "claudecode")]
    ClaudeCode,
    Codex,
    #[serde(alias = "opencode")]
    OpenCode,
    #[serde(alias = "cursor", alias = "cursor-agent", alias = "cursor_agent")]
    CursorCli,
    #[serde(alias = "agy")]
    Antigravity,
    #[serde(alias = "acp", alias = "custom_acp")]
    AcpCustom,
}

impl CodingProviderKind {
    /// Returns all supported provider kinds in canonical display order.
    pub const ALL: [Self; 6] = [
        Self::ClaudeCode,
        Self::Codex,
        Self::OpenCode,
        Self::CursorCli,
        Self::Antigravity,
        Self::AcpCustom,
    ];

    /// Canonical snake_case identifier matching serde serialization.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code",
            Self::Codex => "codex",
            Self::OpenCode => "open_code",
            Self::CursorCli => "cursor_cli",
            Self::Antigravity => "antigravity",
            Self::AcpCustom => "acp_custom",
        }
    }

    /// Human-readable display name for UI pickers and status messages.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::OpenCode => "OpenCode",
            Self::CursorCli => "Cursor CLI",
            Self::Antigravity => "Antigravity",
            Self::AcpCustom => "Custom ACP",
        }
    }

    /// Recommended default Telegram bot username for this provider.
    pub fn recommended_username(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code_bot",
            Self::Codex => "codex_bot",
            Self::OpenCode => "opencode_bot",
            Self::CursorCli => "cursor_cli_bot",
            Self::Antigravity => "antigravity_bot",
            Self::AcpCustom => "acp_coding_bot",
        }
    }

    /// Default executable name searched on `PATH` when no override is configured.
    pub fn default_binary(&self) -> Option<&'static str> {
        match self {
            Self::ClaudeCode => Some("claude"),
            Self::Codex => Some("codex"),
            Self::OpenCode => Some("opencode"),
            Self::CursorCli => Some("cursor-agent"),
            Self::Antigravity => Some("agy"),
            Self::AcpCustom => None,
        }
    }

    /// Whether the provider speaks Agent Client Protocol (ACP) directly.
    pub fn supports_acp(&self) -> bool {
        matches!(self, Self::OpenCode | Self::CursorCli | Self::AcpCustom)
    }

    /// Parses a provider kind from canonical names or common CLI aliases.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "claude_code" | "claudecode" | "claude-code" | "claude" => Some(Self::ClaudeCode),
            "codex" | "openai_codex" | "openai-codex" => Some(Self::Codex),
            "open_code" | "opencode" | "open-code" => Some(Self::OpenCode),
            "cursor_cli" | "cursorcli" | "cursor-cli" | "cursor" | "cursor-agent"
            | "cursor_agent" => Some(Self::CursorCli),
            "antigravity" | "agy" => Some(Self::Antigravity),
            "acp_custom" | "acpcustom" | "acp-custom" | "acp" | "custom_acp" => {
                Some(Self::AcpCustom)
            }
            _ => None,
        }
    }
}

/// Execution approval policy for local coding agent file edits and tool commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CodingPermissionMode {
    #[default]
    Ask,
    AutoEdit,
    FullAccess,
}

impl CodingPermissionMode {
    /// Canonical snake_case identifier matching serde serialization.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::AutoEdit => "auto_edit",
            Self::FullAccess => "full_access",
        }
    }

    /// Localized Chinese label for desktop/miniapp UI.
    pub fn label_zh(&self) -> &'static str {
        match self {
            Self::Ask => "每次询问",
            Self::AutoEdit => "自动编辑",
            Self::FullAccess => "完全访问",
        }
    }

    /// Localized English label for desktop/miniapp UI.
    pub fn label_en(&self) -> &'static str {
        match self {
            Self::Ask => "Ask",
            Self::AutoEdit => "Auto Edit",
            Self::FullAccess => "Full Access",
        }
    }

    /// Parses a permission mode from canonical strings or aliases.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "ask" | "prompt" => Some(Self::Ask),
            "auto_edit" | "autoedit" | "auto-edit" => Some(Self::AutoEdit),
            "full_access" | "fullaccess" | "full-access" | "full" | "autonomous" => {
                Some(Self::FullAccess)
            }
            _ => None,
        }
    }
}

/// User decision in response to an interactive coding tool/command approval prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodingApprovalDecision {
    AllowOnce,
    AllowSession,
    Deny,
}

/// Per-bot local coding agent configuration stored on Fabushi/Bot Registry entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CodingAgentBotConfig {
    pub provider_kind: CodingProviderKind,
    pub workspace_path: String,
    pub use_worktree: bool,
    pub permission_mode: CodingPermissionMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_override: Option<String>,
    #[serde(default)]
    pub extra_args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bound_device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bound_node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_override: Option<String>,
    #[serde(default)]
    pub allow_group_members: bool,
}

/// Discovery record for a coding provider CLI probed on a local host device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DetectedCodingProvider {
    pub kind: CodingProviderKind,
    pub display_name: String,
    pub binary_path: Option<String>,
    pub version: Option<String>,
    pub installed: bool,
    pub recommended_username: String,
    pub supports_acp: bool,
    pub login_hint: String,
}

/// Summary of workspace git diff changes produced by a coding turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CodingDiffStat {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
    pub changed_files: Vec<String>,
}

/// Token and cost telemetry reported for a completed coding turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CodingTokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub cost_usd: Option<f64>,
}

/// Slash-command / inspector control actions dispatched to a local coding host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum CodingControlAction {
    SetWorkspace {
        path: String,
    },
    WorktreeOp {
        op: String,
        #[serde(default)]
        branch: Option<String>,
    },
    GetDiff {
        #[serde(default)]
        turn_idx: Option<u64>,
    },
    RollbackCheckpoint {
        #[serde(default)]
        turn_idx: Option<u64>,
    },
    ListModels,
    SetModel {
        model: String,
    },
    SetPermissionMode {
        mode: CodingPermissionMode,
    },
    CommitAndPr {
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        title: Option<String>,
    },
    GetStatus,
    CheckLogin,
    ResetSession,
    BuildHandoffSummary {
        target_bot_username: String,
    },
}

/// Commands pushed from server to desktop/host runtime (`UpdateEvent::CodingHostCommand`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum CodingHostCommand {
    ProbeProviders {
        req_id: String,
        owner_user_id: u64,
        #[serde(default)]
        target_device_id: Option<String>,
    },
    StartTurn {
        turn_id: String,
        owner_user_id: u64,
        #[serde(default)]
        target_device_id: Option<String>,
        bot_user_id: i64,
        chat_id: i64,
        #[serde(default)]
        topic_id: Option<i64>,
        #[serde(default)]
        is_forum_topic: bool,
        prompt: String,
        config: CodingAgentBotConfig,
    },
    CancelTurn {
        turn_id: String,
        owner_user_id: u64,
        #[serde(default)]
        target_device_id: Option<String>,
    },
    ResolveApproval {
        turn_id: String,
        request_id: String,
        owner_user_id: u64,
        #[serde(default)]
        target_device_id: Option<String>,
        decision: CodingApprovalDecision,
    },
    Control {
        req_id: String,
        owner_user_id: u64,
        #[serde(default)]
        target_device_id: Option<String>,
        bot_user_id: i64,
        chat_id: i64,
        #[serde(default)]
        topic_id: Option<i64>,
        #[serde(default)]
        is_forum_topic: bool,
        config: CodingAgentBotConfig,
        action: CodingControlAction,
    },
}

/// Events emitted from desktop/host runtime back to server (`WsClientFrame::CodingHostEvent`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CodingHostEvent {
    HostRegister {
        device_id: String,
        display_name: String,
        os_name: String,
        providers: Vec<DetectedCodingProvider>,
    },
    ProbeResult {
        req_id: String,
        device_id: String,
        providers: Vec<DetectedCodingProvider>,
    },
    TurnStarted {
        turn_id: String,
        #[serde(default)]
        provider_session_id: Option<String>,
        workspace_path: String,
        #[serde(default)]
        worktree_path: Option<String>,
        #[serde(default)]
        git_branch: Option<String>,
        turn_idx: u64,
    },
    TurnActivity {
        turn_id: String,
        state: String,
        #[serde(default)]
        detail: Option<String>,
    },
    TurnThinkingDelta {
        turn_id: String,
        delta: String,
    },
    TurnTextDelta {
        turn_id: String,
        delta: String,
    },
    TurnToolStep {
        turn_id: String,
        call_id: String,
        tool_name: String,
        title: String,
        status: String,
        #[serde(default)]
        output_preview: Option<String>,
    },
    TurnApprovalRequested {
        turn_id: String,
        request_id: String,
        kind: String,
        title: String,
        detail: String,
        rule_key: String,
    },
    TurnCompleted {
        turn_id: String,
        ok: bool,
        final_text: String,
        #[serde(default)]
        error: Option<String>,
        turn_idx: u64,
        #[serde(default)]
        checkpoint_ref: Option<String>,
        #[serde(default)]
        diff_stat: Option<CodingDiffStat>,
        #[serde(default)]
        token_usage: Option<CodingTokenUsage>,
    },
    ControlResult {
        req_id: String,
        ok: bool,
        markdown_reply: String,
        #[serde(default)]
        updated_workspace: Option<String>,
        #[serde(default)]
        updated_use_worktree: Option<bool>,
        #[serde(default)]
        updated_model: Option<String>,
        #[serde(default)]
        updated_permission_mode: Option<CodingPermissionMode>,
        #[serde(default)]
        handoff_summary: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::miniapp_contracts::{
        BotRegistryEntry, CreateFabushiBotRequest, FabushiBotSummaryDto, PatchFabushiBotRequest,
    };
    use crate::sync::UpdateEvent;
    use crate::ws::WsClientFrame;

    #[test]
    fn provider_kind_helpers_and_serde_roundtrip() {
        assert_eq!(CodingProviderKind::default(), CodingProviderKind::ClaudeCode);

        for kind in CodingProviderKind::ALL {
            let s = kind.as_str();
            assert_eq!(CodingProviderKind::parse(s), Some(kind));
            assert!(!kind.display_name().is_empty());
            assert!(kind.recommended_username().ends_with("_bot"));

            let encoded = serde_json::to_string(&kind).unwrap();
            assert_eq!(encoded, format!("\"{s}\""));
            let decoded: CodingProviderKind = serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded, kind);
        }

        assert_eq!(CodingProviderKind::ClaudeCode.default_binary(), Some("claude"));
        assert_eq!(CodingProviderKind::Codex.default_binary(), Some("codex"));
        assert_eq!(CodingProviderKind::OpenCode.default_binary(), Some("opencode"));
        assert_eq!(CodingProviderKind::CursorCli.default_binary(), Some("cursor-agent"));
        assert_eq!(CodingProviderKind::Antigravity.default_binary(), Some("agy"));
        assert_eq!(CodingProviderKind::AcpCustom.default_binary(), None);

        assert!(CodingProviderKind::OpenCode.supports_acp());
        assert!(CodingProviderKind::CursorCli.supports_acp());
        assert!(CodingProviderKind::AcpCustom.supports_acp());
        assert!(!CodingProviderKind::ClaudeCode.supports_acp());

        // Aliases
        assert_eq!(CodingProviderKind::parse("opencode"), Some(CodingProviderKind::OpenCode));
        assert_eq!(CodingProviderKind::parse("  CLAUDE "), Some(CodingProviderKind::ClaudeCode));
        assert_eq!(CodingProviderKind::parse("cursor"), Some(CodingProviderKind::CursorCli));
        assert_eq!(CodingProviderKind::parse("agy"), Some(CodingProviderKind::Antigravity));
        assert_eq!(CodingProviderKind::parse("acp"), Some(CodingProviderKind::AcpCustom));
        assert_eq!(CodingProviderKind::parse("unknown"), None);
    }

    #[test]
    fn permission_mode_and_approval_decision_roundtrip() {
        assert_eq!(CodingPermissionMode::default(), CodingPermissionMode::Ask);
        for mode in [
            CodingPermissionMode::Ask,
            CodingPermissionMode::AutoEdit,
            CodingPermissionMode::FullAccess,
        ] {
            assert_eq!(CodingPermissionMode::parse(mode.as_str()), Some(mode));
            assert!(!mode.label_zh().is_empty());
            assert!(!mode.label_en().is_empty());
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, format!("\"{}\"", mode.as_str()));
            let decoded: CodingPermissionMode = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, mode);
        }
        assert_eq!(CodingPermissionMode::parse("invalid"), None);

        for decision in [
            CodingApprovalDecision::AllowOnce,
            CodingApprovalDecision::AllowSession,
            CodingApprovalDecision::Deny,
        ] {
            let json = serde_json::to_string(&decision).unwrap();
            let decoded: CodingApprovalDecision = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, decision);
        }
    }

    #[test]
    fn config_and_control_actions_roundtrip() {
        let default_cfg: CodingAgentBotConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(default_cfg.provider_kind, CodingProviderKind::ClaudeCode);
        assert_eq!(default_cfg.permission_mode, CodingPermissionMode::Ask);
        assert!(!default_cfg.allow_group_members);
        assert!(default_cfg.extra_args.is_empty());

        let cfg = CodingAgentBotConfig {
            provider_kind: CodingProviderKind::OpenCode,
            workspace_path: "/tmp/repo".to_string(),
            use_worktree: true,
            permission_mode: CodingPermissionMode::AutoEdit,
            executable_override: Some("/usr/local/bin/opencode".to_string()),
            extra_args: vec!["--model".to_string(), "gpt-5".to_string()],
            bound_device_id: Some("dev_mac_1".to_string()),
            bound_node_id: Some("node_1".to_string()),
            model_override: Some("claude-sonnet-4".to_string()),
            allow_group_members: true,
        };
        let cfg_rt: CodingAgentBotConfig =
            serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(cfg, cfg_rt);

        let actions = vec![
            CodingControlAction::SetWorkspace {
                path: "/repo".to_string(),
            },
            CodingControlAction::WorktreeOp {
                op: "create".to_string(),
                branch: Some("feat/acp".to_string()),
            },
            CodingControlAction::GetDiff { turn_idx: Some(3) },
            CodingControlAction::RollbackCheckpoint { turn_idx: None },
            CodingControlAction::ListModels,
            CodingControlAction::SetModel {
                model: "opus".to_string(),
            },
            CodingControlAction::SetPermissionMode {
                mode: CodingPermissionMode::FullAccess,
            },
            CodingControlAction::CommitAndPr {
                message: Some("feat: add coding".to_string()),
                title: Some("Add coding".to_string()),
            },
            CodingControlAction::GetStatus,
            CodingControlAction::CheckLogin,
            CodingControlAction::ResetSession,
            CodingControlAction::BuildHandoffSummary {
                target_bot_username: "codex_bot".to_string(),
            },
        ];
        for action in actions {
            let json = serde_json::to_string(&action).unwrap();
            let decoded: CodingControlAction = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, action);
        }
    }

    #[test]
    fn host_commands_and_update_event_roundtrip() {
        let cfg = CodingAgentBotConfig {
            provider_kind: CodingProviderKind::CursorCli,
            workspace_path: "/Users/dev/project".to_string(),
            use_worktree: true,
            permission_mode: CodingPermissionMode::Ask,
            ..Default::default()
        };

        let commands = vec![
            CodingHostCommand::ProbeProviders {
                req_id: "req_1".to_string(),
                owner_user_id: 42,
                target_device_id: Some("dev_1".to_string()),
            },
            CodingHostCommand::StartTurn {
                turn_id: "turn_1".to_string(),
                owner_user_id: 42,
                target_device_id: None,
                bot_user_id: 800_001,
                chat_id: 900_001,
                topic_id: Some(7),
                is_forum_topic: true,
                prompt: "fix failing tests".to_string(),
                config: cfg.clone(),
            },
            CodingHostCommand::CancelTurn {
                turn_id: "turn_1".to_string(),
                owner_user_id: 42,
                target_device_id: Some("dev_1".to_string()),
            },
            CodingHostCommand::ResolveApproval {
                turn_id: "turn_1".to_string(),
                request_id: "apr_1".to_string(),
                owner_user_id: 42,
                target_device_id: Some("dev_1".to_string()),
                decision: CodingApprovalDecision::AllowSession,
            },
            CodingHostCommand::Control {
                req_id: "req_2".to_string(),
                owner_user_id: 42,
                target_device_id: Some("dev_1".to_string()),
                bot_user_id: 800_001,
                chat_id: 900_001,
                topic_id: None,
                is_forum_topic: false,
                config: cfg,
                action: CodingControlAction::SetPermissionMode {
                    mode: CodingPermissionMode::AutoEdit,
                },
            },
        ];

        for cmd in commands {
            let raw = serde_json::to_string(&cmd).unwrap();
            let decoded: CodingHostCommand = serde_json::from_str(&raw).unwrap();
            assert_eq!(decoded, cmd);

            let ev = UpdateEvent::CodingHostCommand(cmd.clone());
            let ev_raw = serde_json::to_string(&ev).unwrap();
            let ev_decoded: UpdateEvent = serde_json::from_str(&ev_raw).unwrap();
            assert_eq!(ev_decoded, ev);
        }
    }

    #[test]
    fn host_events_and_ws_client_frame_roundtrip() {
        let provider = DetectedCodingProvider {
            kind: CodingProviderKind::ClaudeCode,
            display_name: "Claude Code".to_string(),
            binary_path: Some("/usr/local/bin/claude".to_string()),
            version: Some("1.0.12".to_string()),
            installed: true,
            recommended_username: "claude_code_bot".to_string(),
            supports_acp: false,
            login_hint: "Run `claude auth status`".to_string(),
        };

        let events = vec![
            CodingHostEvent::HostRegister {
                device_id: "dev_1".to_string(),
                display_name: "MacBook Pro".to_string(),
                os_name: "macos".to_string(),
                providers: vec![provider.clone()],
            },
            CodingHostEvent::ProbeResult {
                req_id: "req_1".to_string(),
                device_id: "dev_1".to_string(),
                providers: vec![provider],
            },
            CodingHostEvent::TurnStarted {
                turn_id: "turn_1".to_string(),
                provider_session_id: Some("sess_abc".to_string()),
                workspace_path: "/repo".to_string(),
                worktree_path: Some("/tmp/wt/1".to_string()),
                git_branch: Some("bot/turn-1".to_string()),
                turn_idx: 1,
            },
            CodingHostEvent::TurnActivity {
                turn_id: "turn_1".to_string(),
                state: "thinking".to_string(),
                detail: Some("Inspecting Cargo.toml".to_string()),
            },
            CodingHostEvent::TurnThinkingDelta {
                turn_id: "turn_1".to_string(),
                delta: "Checking deps...".to_string(),
            },
            CodingHostEvent::TurnTextDelta {
                turn_id: "turn_1".to_string(),
                delta: "I found the issue.".to_string(),
            },
            CodingHostEvent::TurnToolStep {
                turn_id: "turn_1".to_string(),
                call_id: "call_1".to_string(),
                tool_name: "bash".to_string(),
                title: "Run cargo test".to_string(),
                status: "completed".to_string(),
                output_preview: Some("ok".to_string()),
            },
            CodingHostEvent::TurnApprovalRequested {
                turn_id: "turn_1".to_string(),
                request_id: "apr_1".to_string(),
                kind: "shell".to_string(),
                title: "Allow git push?".to_string(),
                detail: "git push origin main".to_string(),
                rule_key: "bash:git_push".to_string(),
            },
            CodingHostEvent::TurnCompleted {
                turn_id: "turn_1".to_string(),
                ok: true,
                final_text: "All tests pass.".to_string(),
                error: None,
                turn_idx: 1,
                checkpoint_ref: Some("refs/checkpoints/1".to_string()),
                diff_stat: Some(CodingDiffStat {
                    files_changed: 2,
                    insertions: 18,
                    deletions: 4,
                    changed_files: vec!["src/lib.rs".to_string(), "Cargo.toml".to_string()],
                }),
                token_usage: Some(CodingTokenUsage {
                    input_tokens: 1200,
                    output_tokens: 340,
                    cached_tokens: 800,
                    cost_usd: Some(0.0145),
                }),
            },
            CodingHostEvent::ControlResult {
                req_id: "req_2".to_string(),
                ok: true,
                markdown_reply: "Switched permission mode to auto_edit".to_string(),
                updated_workspace: Some("/repo".to_string()),
                updated_use_worktree: Some(true),
                updated_model: Some("sonnet".to_string()),
                updated_permission_mode: Some(CodingPermissionMode::AutoEdit),
                handoff_summary: Some("Summary".to_string()),
            },
        ];

        for ev in events {
            let raw = serde_json::to_string(&ev).unwrap();
            let decoded: CodingHostEvent = serde_json::from_str(&raw).unwrap();
            assert_eq!(decoded, ev);

            let frame = WsClientFrame::CodingHostEvent(ev.clone());
            let frame_raw = serde_json::to_string(&frame).unwrap();
            let frame_decoded: WsClientFrame = serde_json::from_str(&frame_raw).unwrap();
            assert_eq!(frame_decoded, frame);
        }
    }

    #[test]
    fn bot_dtos_backward_compatibility_and_coding_agent_roundtrip() {
        // Legacy JSON payloads lacking `coding_agent` must deserialize cleanly.
        let legacy_entry: BotRegistryEntry = serde_json::from_value(serde_json::json!({
            "bot_user_id": 800001,
            "owner_user_id": 100001,
            "username": "LegacyBot",
            "display_name": "Legacy Bot"
        }))
        .unwrap();
        assert_eq!(legacy_entry.coding_agent, None);
        assert!(serde_json::to_value(&legacy_entry)
            .unwrap()
            .get("coding_agent")
            .is_none());

        let legacy_create: CreateFabushiBotRequest = serde_json::from_value(serde_json::json!({
            "display_name": "Legacy Fabushi"
        }))
        .unwrap();
        assert_eq!(legacy_create.coding_agent, None);
        assert!(serde_json::to_value(&legacy_create)
            .unwrap()
            .get("coding_agent")
            .is_none());

        let legacy_patch: PatchFabushiBotRequest =
            serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(legacy_patch.coding_agent, None);
        assert!(serde_json::to_value(&legacy_patch)
            .unwrap()
            .get("coding_agent")
            .is_none());

        let legacy_summary: FabushiBotSummaryDto = serde_json::from_value(serde_json::json!({
            "bot_user_id": 800002,
            "owner_user_id": 100001,
            "agent_id": "agent_1",
            "username": "legacy_bot",
            "display_name": "Legacy Summary"
        }))
        .unwrap();
        assert_eq!(legacy_summary.coding_agent, None);
        assert!(serde_json::to_value(&legacy_summary)
            .unwrap()
            .get("coding_agent")
            .is_none());

        // Roundtrip with `coding_agent` populated
        let coding_cfg = CodingAgentBotConfig {
            provider_kind: CodingProviderKind::Codex,
            workspace_path: "/workspace/app".to_string(),
            use_worktree: true,
            permission_mode: CodingPermissionMode::AutoEdit,
            bound_device_id: Some("mac-1".to_string()),
            ..Default::default()
        };

        let entry_with_coding = BotRegistryEntry {
            bot_user_id: 800003,
            owner_user_id: 100001,
            username: "codex_bot".to_string(),
            display_name: "Codex".to_string(),
            coding_agent: Some(coding_cfg.clone()),
            ..Default::default()
        };
        let entry_rt: BotRegistryEntry =
            serde_json::from_str(&serde_json::to_string(&entry_with_coding).unwrap()).unwrap();
        assert_eq!(entry_rt.coding_agent.as_ref(), Some(&coding_cfg));

        let create_with_coding = CreateFabushiBotRequest {
            display_name: "Codex".to_string(),
            coding_agent: Some(coding_cfg.clone()),
            ..Default::default()
        };
        let create_rt: CreateFabushiBotRequest =
            serde_json::from_str(&serde_json::to_string(&create_with_coding).unwrap()).unwrap();
        assert_eq!(create_rt.coding_agent.as_ref(), Some(&coding_cfg));

        let patch_with_coding = PatchFabushiBotRequest {
            coding_agent: Some(coding_cfg.clone()),
            ..Default::default()
        };
        let patch_rt: PatchFabushiBotRequest =
            serde_json::from_str(&serde_json::to_string(&patch_with_coding).unwrap()).unwrap();
        assert_eq!(patch_rt.coding_agent.as_ref(), Some(&coding_cfg));

        let summary_with_coding = FabushiBotSummaryDto {
            bot_user_id: 800003,
            owner_user_id: 100001,
            agent_id: "codex-1".to_string(),
            username: "codex_bot".to_string(),
            display_name: "Codex".to_string(),
            coding_agent: Some(coding_cfg.clone()),
            ..Default::default()
        };
        let summary_rt: FabushiBotSummaryDto =
            serde_json::from_str(&serde_json::to_string(&summary_with_coding).unwrap()).unwrap();
        assert_eq!(summary_rt.coding_agent.as_ref(), Some(&coding_cfg));
    }
}
