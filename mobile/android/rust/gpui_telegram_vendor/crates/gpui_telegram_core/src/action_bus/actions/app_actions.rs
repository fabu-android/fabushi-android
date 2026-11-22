use crate::action_bus::{
    ActionCategory, ActionContext, ActionError, ActionRiskLevel, TypedPlatformAction,
};
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

macro_rules! simple_action {
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

fn default_search_limit() -> u32 {
    20
}

// --- Chat & Messaging ---
simple_action!(
    GetChatInfoAction,
    GetChatInfoInput,
    GetChatInfoOutput,
    "get_chat_info",
    ActionCategory::Chat,
    "Fetch metadata, member count, and permissions for a chat",
    ActionRiskLevel::ReadOnly,
    { chat_id: i64 },
    { chat_id: i64, title: String, kind: String },
    |_ctx: ActionContext, input: GetChatInfoInput| {
        if input.chat_id == 0 {
            return Err(ActionError::InvalidInput("chat_id cannot be 0".into()));
        }
        Ok(GetChatInfoOutput {
            chat_id: input.chat_id,
            title: format!("Chat {}", input.chat_id),
            kind: "group".into(),
        })
    }
);

simple_action!(
    CreateChatAction,
    CreateChatInput,
    CreateChatOutput,
    "create_chat",
    ActionCategory::Chat,
    "Create a new group chat or channel",
    ActionRiskLevel::Low,
    { title: String, member_ids: Vec<i64> },
    { chat_id: i64, title: String },
    |_ctx: ActionContext, input: CreateChatInput| {
        if input.title.trim().is_empty() {
            return Err(ActionError::InvalidInput("chat title cannot be empty".into()));
        }
        Ok(CreateChatOutput {
            chat_id: 1,
            title: input.title,
        })
    }
);

simple_action!(
    ReactMessageAction,
    ReactMessageInput,
    ReactMessageOutput,
    "react_message",
    ActionCategory::Messaging,
    "Toggle an emoji reaction on a message",
    ActionRiskLevel::Low,
    { chat_id: i64, message_id: i64, emoji: String },
    { ok: bool, chat_id: i64, message_id: i64 },
    |_ctx: ActionContext, input: ReactMessageInput| {
        if input.emoji.trim().is_empty() {
            return Err(ActionError::InvalidInput("emoji cannot be empty".into()));
        }
        Ok(ReactMessageOutput {
            ok: true,
            chat_id: input.chat_id,
            message_id: input.message_id,
        })
    }
);

// --- Search & Media ---
simple_action!(
    SearchMessagesAction,
    SearchMessagesInput,
    SearchMessagesOutput,
    "search_messages",
    ActionCategory::Search,
    "Search messages across all chats or within a specific chat",
    ActionRiskLevel::ReadOnly,
    { query: String, chat_id: Option<i64>, limit: u32 = "default_search_limit" },
    { total: u32, query: String },
    |_ctx: ActionContext, input: SearchMessagesInput| {
        if input.query.trim().is_empty() {
            return Err(ActionError::InvalidInput("search query cannot be empty".into()));
        }
        Ok(SearchMessagesOutput {
            total: 0,
            query: input.query,
        })
    }
);

simple_action!(
    UploadMediaAction,
    UploadMediaInput,
    UploadMediaOutput,
    "upload_media",
    ActionCategory::Media,
    "Upload a media attachment to content-addressed storage",
    ActionRiskLevel::Low,
    { path: String, mime_type: Option<String> },
    { media_id: String, path: String },
    |_ctx: ActionContext, input: UploadMediaInput| {
        if input.path.trim().is_empty() {
            return Err(ActionError::InvalidInput("media path cannot be empty".into()));
        }
        Ok(UploadMediaOutput {
            media_id: uuid::Uuid::new_v4().to_string(),
            path: input.path,
        })
    }
);

// --- User, Call, Vault, System, Admin ---
simple_action!(
    GetUserInfoAction,
    GetUserInfoInput,
    GetUserInfoOutput,
    "get_user_info",
    ActionCategory::User,
    "Get profile and wallet summary for the current or target user",
    ActionRiskLevel::ReadOnly,
    { user_id: Option<u64> },
    { user_id: u64, username: String },
    |ctx: ActionContext, input: GetUserInfoInput| {
        Ok(GetUserInfoOutput {
            user_id: input.user_id.or(ctx.user_id).unwrap_or(0),
            username: "fabushi_user".into(),
        })
    }
);

simple_action!(
    GetCallStatusAction,
    GetCallStatusInput,
    GetCallStatusOutput,
    "get_call_status",
    ActionCategory::Call,
    "Inspect active voice/video call state",
    ActionRiskLevel::ReadOnly,
    { call_id: Option<String> },
    { active: bool, state: String },
    |_ctx: ActionContext, _input: GetCallStatusInput| {
        Ok(GetCallStatusOutput {
            active: false,
            state: "idle".into(),
        })
    }
);

simple_action!(
    VaultStatusAction,
    VaultStatusInput,
    VaultStatusOutput,
    "vault_status",
    ActionCategory::Vault,
    "Inspect encrypted user vault unlock and E2EE key backup state",
    ActionRiskLevel::ReadOnly,
    {},
    { unlocked: bool, backed_up: bool },
    |_ctx: ActionContext, _input: VaultStatusInput| {
        Ok(VaultStatusOutput {
            unlocked: false,
            backed_up: false,
        })
    }
);

simple_action!(
    SystemStatusAction,
    SystemStatusInput,
    SystemStatusOutput,
    "system_status",
    ActionCategory::System,
    "Inspect Fabushi daemon, IPC socket, and application runtime status",
    ActionRiskLevel::ReadOnly,
    {},
    { ok: bool, version: String },
    |_ctx: ActionContext, _input: SystemStatusInput| {
        Ok(SystemStatusOutput {
            ok: true,
            version: env!("CARGO_PKG_VERSION").into(),
        })
    }
);

simple_action!(
    AdminAuditSummaryAction,
    AdminAuditSummaryInput,
    AdminAuditSummaryOutput,
    "admin_audit_summary",
    ActionCategory::Admin,
    "Retrieve administrative server and security audit summary",
    ActionRiskLevel::ReadOnly,
    {},
    { total_events: u64 },
    |_ctx: ActionContext, _input: AdminAuditSummaryInput| {
        Ok(AdminAuditSummaryOutput { total_events: 0 })
    }
);

// --- Bots, MiniApps, Tasks ---
simple_action!(
    ListBotsAction,
    ListBotsInput,
    ListBotsOutput,
    "list_bots",
    ActionCategory::Bot,
    "List configured AI and automation bots and their bound computer nodes",
    ActionRiskLevel::ReadOnly,
    {},
    { count: u32 },
    |_ctx: ActionContext, _input: ListBotsInput| {
        Ok(ListBotsOutput { count: 0 })
    }
);

simple_action!(
    BindBotComputerAction,
    BindBotComputerInput,
    BindBotComputerOutput,
    "bind_bot_computer",
    ActionCategory::Bot,
    "Bind a bot to a target local device or cloud computer node",
    ActionRiskLevel::Medium,
    { bot_id: u64, node_id: String },
    { bot_id: u64, node_id: String, bound: bool },
    |_ctx: ActionContext, input: BindBotComputerInput| {
        if input.node_id.trim().is_empty() {
            return Err(ActionError::InvalidInput("node_id cannot be empty".into()));
        }
        Ok(BindBotComputerOutput {
            bot_id: input.bot_id,
            node_id: input.node_id,
            bound: true,
        })
    }
);

simple_action!(
    ListMiniAppsAction,
    ListMiniAppsInput,
    ListMiniAppsOutput,
    "list_miniapps",
    ActionCategory::MiniApp,
    "List installed mini-apps and WebMCP tool providers",
    ActionRiskLevel::ReadOnly,
    {},
    { count: u32 },
    |_ctx: ActionContext, _input: ListMiniAppsInput| {
        Ok(ListMiniAppsOutput { count: 0 })
    }
);

simple_action!(
    ListTasksAction,
    ListTasksInput,
    ListTasksOutput,
    "list_tasks",
    ActionCategory::Task,
    "List scheduled and pending automation tasks",
    ActionRiskLevel::ReadOnly,
    {},
    { count: u32 },
    |_ctx: ActionContext, _input: ListTasksInput| {
        Ok(ListTasksOutput { count: 0 })
    }
);

simple_action!(
    CreateTaskAction,
    CreateTaskInput,
    CreateTaskOutput,
    "create_task",
    ActionCategory::Task,
    "Create an automation or agent task",
    ActionRiskLevel::Low,
    { title: String, target_node_id: Option<String> },
    { task_id: String, title: String },
    |_ctx: ActionContext, input: CreateTaskInput| {
        if input.title.trim().is_empty() {
            return Err(ActionError::InvalidInput("task title cannot be empty".into()));
        }
        Ok(CreateTaskOutput {
            task_id: uuid::Uuid::new_v4().to_string(),
            title: input.title,
        })
    }
);

simple_action!(
    ApproveTaskAction,
    ApproveTaskInput,
    ApproveTaskOutput,
    "approve_task",
    ActionCategory::Task,
    "Approve execution of a pending high-risk task or remote command",
    ActionRiskLevel::High,
    { task_id: String, approved: bool },
    { task_id: String, approved: bool },
    |_ctx: ActionContext, input: ApproveTaskInput| {
        if input.task_id.trim().is_empty() {
            return Err(ActionError::InvalidInput("task_id cannot be empty".into()));
        }
        Ok(ApproveTaskOutput {
            task_id: input.task_id,
            approved: input.approved,
        })
    }
);
