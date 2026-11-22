pub mod app_actions;
pub mod delete_message;
pub mod get_chat_history;
pub mod list_dialogs;
pub mod remote_actions;
pub mod send_message;

pub use app_actions::*;
pub use delete_message::*;
pub use get_chat_history::*;
pub use list_dialogs::*;
pub use remote_actions::*;
pub use send_message::*;

use crate::action_bus::{ActionError, ActionRegistry};

/// Registers the full-app catalog of Platform Actions across all 19 categories.
pub fn register_full_app_actions(registry: &ActionRegistry) -> Result<(), ActionError> {
    registry.register_typed(SendMessageAction)?;
    registry.register_typed(GetChatHistoryAction)?;
    registry.register_typed(ListDialogsAction)?;
    registry.register_typed(DeleteMessageAction)?;

    registry.register_typed(GetChatInfoAction)?;
    registry.register_typed(CreateChatAction)?;
    registry.register_typed(ReactMessageAction)?;
    registry.register_typed(SearchMessagesAction)?;
    registry.register_typed(UploadMediaAction)?;
    registry.register_typed(GetUserInfoAction)?;
    registry.register_typed(GetCallStatusAction)?;
    registry.register_typed(VaultStatusAction)?;
    registry.register_typed(SystemStatusAction)?;
    registry.register_typed(AdminAuditSummaryAction)?;
    registry.register_typed(ListBotsAction)?;
    registry.register_typed(BindBotComputerAction)?;
    registry.register_typed(ListMiniAppsAction)?;
    registry.register_typed(ListTasksAction)?;
    registry.register_typed(CreateTaskAction)?;
    registry.register_typed(ApproveTaskAction)?;

    registry.register_typed(DeviceListAction)?;
    registry.register_typed(DeviceInfoAction)?;
    registry.register_typed(DeviceConnectAction)?;
    registry.register_typed(DevicePowerAction)?;
    registry.register_typed(RemoteCaptureScreenAction)?;
    registry.register_typed(RemoteMouseAction)?;
    registry.register_typed(RemoteKeyboardAction)?;
    registry.register_typed(RemoteSessionsListAction)?;
    registry.register_typed(RemoteDisconnectAllAction)?;
    registry.register_typed(TerminalListAction)?;
    registry.register_typed(TerminalExecAction)?;
    registry.register_typed(AssistInfoAction)?;
    registry.register_typed(AssistStartAction)?;
    registry.register_typed(CloudPcListAction)?;
    registry.register_typed(CloudPcShutdownAction)?;
    registry.register_typed(McpStatusAction)?;
    registry.register_typed(McpRevokeGrantAction)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_bus::{ActionCategory, CliExporter, HermesToolExporter, McpExporter, OpenApiExporter};

    #[test]
    fn test_full_app_actions_cover_all_19_categories_and_export_cleanly() {
        let registry = ActionRegistry::new();
        register_full_app_actions(&registry).expect("register full app actions");
        assert!(registry.count() >= 37);

        for cat in ActionCategory::ALL {
            assert!(
                !registry.list_by_category(cat).is_empty(),
                "category {:?} must have at least one registered action",
                cat
            );
        }

        let cli = CliExporter::export_cli_schema(&registry, "fabushi", "1.4.0");
        assert_eq!(cli.subcommands.len(), registry.count());
        let mcp = McpExporter::export_tools_list(&registry);
        assert_eq!(mcp.tools.len(), registry.count());
        let hermes = HermesToolExporter::export_tools(&registry);
        assert_eq!(hermes.len(), registry.count());
        let openapi = OpenApiExporter::export_spec(&registry, "Fabushi", "1.4.0", "http://127.0.0.1:8080");
        assert_eq!(openapi["paths"].as_object().unwrap().len(), registry.count());
    }
}
