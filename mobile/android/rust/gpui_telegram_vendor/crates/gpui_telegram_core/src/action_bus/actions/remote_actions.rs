use crate::action_bus::{
    ActionCategory, ActionContext, ActionError, ActionRiskLevel, TypedPlatformAction,
};
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

macro_rules! remote_action {
    (
        $struct_name:ident,
        $input_name:ident,
        $output_name:ident,
        $action_name:expr,
        $category:expr,
        $description:expr,
        $risk:expr,
        { $( $in_field:ident : $in_ty:ty $( = $default_fn:expr )? ),* $(,)? },
        { $( $out_field:ident : $out_ty:ty ),* $(,)? },
        $exec:expr
    ) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $input_name {
            $(
                $( #[serde(default = $default_fn)] )?
                pub $in_field: $in_ty,
            )*
        }

        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        pub struct $output_name {
            $( pub $out_field: $out_ty, )*
        }

        #[derive(Debug, Clone, Default)]
        pub struct $struct_name;

        impl TypedPlatformAction for $struct_name {
            type Input = $input_name;
            type Output = $output_name;

            fn name(&self) -> &'static str {
                $action_name
            }

            fn category(&self) -> ActionCategory {
                $category
            }

            fn description(&self) -> &'static str {
                $description
            }

            fn risk_level(&self) -> ActionRiskLevel {
                $risk
            }

            fn execute<'a>(
                &'a self,
                ctx: ActionContext,
                input: Self::Input,
            ) -> BoxFuture<'a, Result<Self::Output, ActionError>> {
                Box::pin(async move {
                    if self.risk_level().requires_explicit_confirmation() && !ctx.approved {
                        return Err(ActionError::Forbidden(format!(
                            "Action '{}' requires explicit approval confirmation",
                            self.name()
                        )));
                    }
                    ($exec)(ctx, input)
                })
            }
        }
    };
}

// --- Device ---
remote_action!(
    DeviceListAction,
    DeviceListInput,
    DeviceListOutput,
    "device_list",
    ActionCategory::Device,
    "List online and offline devices registered under the current account",
    ActionRiskLevel::ReadOnly,
    { online_only: Option<bool> },
    { count: u32 },
    |_ctx: ActionContext, _input: DeviceListInput| Ok(DeviceListOutput { count: 0 })
);

remote_action!(
    DeviceInfoAction,
    DeviceInfoInput,
    DeviceInfoOutput,
    "device_info",
    ActionCategory::Device,
    "Inspect detailed hardware, OS, resolution, and transport status of a device",
    ActionRiskLevel::ReadOnly,
    { node_id: String },
    { node_id: String, online: bool },
    |_ctx: ActionContext, input: DeviceInfoInput| {
        if input.node_id.trim().is_empty() {
            return Err(ActionError::InvalidInput("node_id cannot be empty".into()));
        }
        Ok(DeviceInfoOutput {
            node_id: input.node_id,
            online: false,
        })
    }
);

remote_action!(
    DeviceConnectAction,
    DeviceConnectInput,
    DeviceConnectOutput,
    "device_connect",
    ActionCategory::Device,
    "Initiate a remote control session to a device",
    ActionRiskLevel::Low,
    { node_id: String, mode: Option<String> },
    { node_id: String, connected: bool },
    |_ctx: ActionContext, input: DeviceConnectInput| {
        if input.node_id.trim().is_empty() {
            return Err(ActionError::InvalidInput("node_id cannot be empty".into()));
        }
        Ok(DeviceConnectOutput {
            node_id: input.node_id,
            connected: true,
        })
    }
);

remote_action!(
    DevicePowerAction,
    DevicePowerInput,
    DevicePowerOutput,
    "device_power_action",
    ActionCategory::Device,
    "Execute lock, sleep, reboot, shutdown, or Wake-on-LAN on a target device",
    ActionRiskLevel::High,
    { node_id: String, action: String, mac_address: Option<String>, delay_minutes: Option<u32> },
    { node_id: String, action: String, dispatched: bool },
    |_ctx: ActionContext, input: DevicePowerInput| {
        if input.node_id.trim().is_empty() || input.action.trim().is_empty() {
            return Err(ActionError::InvalidInput("node_id and action are required".into()));
        }
        Ok(DevicePowerOutput {
            node_id: input.node_id,
            action: input.action,
            dispatched: true,
        })
    }
);

// --- Remote (Desktop, Screen, Input, Clipboard, Files, Sessions) ---
remote_action!(
    RemoteCaptureScreenAction,
    RemoteCaptureScreenInput,
    RemoteCaptureScreenOutput,
    "remote_capture_screen",
    ActionCategory::Remote,
    "Capture the current screen of a device",
    ActionRiskLevel::ReadOnly,
    { node_id: String, display_index: Option<u32> },
    { node_id: String, width: u32, height: u32 },
    |_ctx: ActionContext, input: RemoteCaptureScreenInput| {
        Ok(RemoteCaptureScreenOutput {
            node_id: input.node_id,
            width: 0,
            height: 0,
        })
    }
);

remote_action!(
    RemoteMouseAction,
    RemoteMouseInput,
    RemoteMouseOutput,
    "remote_mouse_action",
    ActionCategory::Remote,
    "Send mouse move, click, drag, or scroll input to a remote desktop",
    ActionRiskLevel::Medium,
    { node_id: String, action: String, x: Option<i32>, y: Option<i32>, button: Option<String> },
    { ok: bool },
    |_ctx: ActionContext, input: RemoteMouseInput| {
        if input.action.trim().is_empty() {
            return Err(ActionError::InvalidInput("mouse action cannot be empty".into()));
        }
        Ok(RemoteMouseOutput { ok: true })
    }
);

remote_action!(
    RemoteKeyboardAction,
    RemoteKeyboardInput,
    RemoteKeyboardOutput,
    "remote_keyboard_action",
    ActionCategory::Remote,
    "Send text typing or keyboard hotkey input to a remote desktop",
    ActionRiskLevel::Medium,
    { node_id: String, action: String, text: Option<String>, key: Option<String> },
    { ok: bool },
    |_ctx: ActionContext, input: RemoteKeyboardInput| {
        if input.action.trim().is_empty() {
            return Err(ActionError::InvalidInput("keyboard action cannot be empty".into()));
        }
        Ok(RemoteKeyboardOutput { ok: true })
    }
);

remote_action!(
    RemoteSessionsListAction,
    RemoteSessionsListInput,
    RemoteSessionsListOutput,
    "remote_sessions_list",
    ActionCategory::Remote,
    "List active remote desktop, assist, CLI, and AI MCP sessions controlling this computer",
    ActionRiskLevel::ReadOnly,
    {},
    { active_count: u32 },
    |_ctx: ActionContext, _input: RemoteSessionsListInput| Ok(RemoteSessionsListOutput { active_count: 0 })
);

remote_action!(
    RemoteDisconnectAllAction,
    RemoteDisconnectAllInput,
    RemoteDisconnectAllOutput,
    "remote_disconnect_all",
    ActionCategory::Remote,
    "Immediately terminate all active remote-control and AI MCP sessions on this device",
    ActionRiskLevel::Low,
    {},
    { disconnected: u32 },
    |_ctx: ActionContext, _input: RemoteDisconnectAllInput| Ok(RemoteDisconnectAllOutput { disconnected: 0 })
);

// --- Terminal ---
remote_action!(
    TerminalListAction,
    TerminalListInput,
    TerminalListOutput,
    "terminal_list",
    ActionCategory::Terminal,
    "List persistent PTY multiplexer sessions",
    ActionRiskLevel::ReadOnly,
    { node_id: Option<String> },
    { count: u32 },
    |_ctx: ActionContext, _input: TerminalListInput| Ok(TerminalListOutput { count: 0 })
);

remote_action!(
    TerminalExecAction,
    TerminalExecInput,
    TerminalExecOutput,
    "terminal_exec",
    ActionCategory::Terminal,
    "Execute a command inside a persistent PTY session and return exit code and output",
    ActionRiskLevel::Medium,
    { session_name: String, command: String, timeout_ms: Option<u64> },
    { session_name: String, exit_code: i32, output: String },
    |_ctx: ActionContext, input: TerminalExecInput| {
        if input.command.trim().is_empty() {
            return Err(ActionError::InvalidInput("command cannot be empty".into()));
        }
        Ok(TerminalExecOutput {
            session_name: input.session_name,
            exit_code: 0,
            output: String::new(),
        })
    }
);

// --- Assist ---
remote_action!(
    AssistInfoAction,
    AssistInfoInput,
    AssistInfoOutput,
    "assist_info",
    ActionCategory::Assist,
    "Fetch permanent 9-digit Remote Assist ID, allow state, and current code policy",
    ActionRiskLevel::ReadOnly,
    {},
    { assist_id: String, allow_assist: bool },
    |_ctx: ActionContext, _input: AssistInfoInput| {
        Ok(AssistInfoOutput {
            assist_id: "000000000".into(),
            allow_assist: false,
        })
    }
);

remote_action!(
    AssistStartAction,
    AssistStartInput,
    AssistStartOutput,
    "assist_start",
    ActionCategory::Assist,
    "Connect to a remote peer using its 9-digit Assist ID and verification code",
    ActionRiskLevel::High,
    { assist_id: String, access_code: String },
    { session_id: String, accepted: bool },
    |_ctx: ActionContext, input: AssistStartInput| {
        if input.assist_id.trim().len() != 9 {
            return Err(ActionError::InvalidInput("assist_id must be 9 digits".into()));
        }
        Ok(AssistStartOutput {
            session_id: uuid::Uuid::new_v4().to_string(),
            accepted: true,
        })
    }
);

// --- CloudPc ---
remote_action!(
    CloudPcListAction,
    CloudPcListInput,
    CloudPcListOutput,
    "cloudpc_list",
    ActionCategory::CloudPc,
    "List managed cloud servers and cloud sandbox instances",
    ActionRiskLevel::ReadOnly,
    {},
    { count: u32 },
    |_ctx: ActionContext, _input: CloudPcListInput| Ok(CloudPcListOutput { count: 0 })
);

remote_action!(
    CloudPcShutdownAction,
    CloudPcShutdownInput,
    CloudPcShutdownOutput,
    "cloudpc_shutdown",
    ActionCategory::CloudPc,
    "Shut down a running cloud PC instance",
    ActionRiskLevel::High,
    { node_id: String },
    { node_id: String, stopped: bool },
    |_ctx: ActionContext, input: CloudPcShutdownInput| {
        if input.node_id.trim().is_empty() {
            return Err(ActionError::InvalidInput("node_id cannot be empty".into()));
        }
        Ok(CloudPcShutdownOutput {
            node_id: input.node_id,
            stopped: true,
        })
    }
);

// --- Mcp ---
remote_action!(
    McpStatusAction,
    McpStatusInput,
    McpStatusOutput,
    "mcp_status",
    ActionCategory::Mcp,
    "Inspect official Streamable HTTP MCP server endpoint and OAuth configuration",
    ActionRiskLevel::ReadOnly,
    {},
    { enabled: bool, protocol_version: String },
    |_ctx: ActionContext, _input: McpStatusInput| {
        Ok(McpStatusOutput {
            enabled: true,
            protocol_version: "2025-11-25".into(),
        })
    }
);

remote_action!(
    McpRevokeGrantAction,
    McpRevokeGrantInput,
    McpRevokeGrantOutput,
    "mcp_revoke_grant",
    ActionCategory::Mcp,
    "Revoke an OAuth 2.1 AI MCP client token family grant",
    ActionRiskLevel::High,
    { family_id: String },
    { family_id: String, revoked: bool },
    |_ctx: ActionContext, input: McpRevokeGrantInput| {
        if input.family_id.trim().is_empty() {
            return Err(ActionError::InvalidInput("family_id cannot be empty".into()));
        }
        Ok(McpRevokeGrantOutput {
            family_id: input.family_id,
            revoked: true,
        })
    }
);
