use gpui_telegram_core::action_bus::*;
use serde_json::json;

#[test]
fn test_action_registry_register_and_lookup() {
    let registry = ActionRegistry::new();
    registry.register_typed(SendMessageAction).unwrap();
    registry.register_typed(GetChatHistoryAction).unwrap();
    registry.register_typed(ListDialogsAction).unwrap();
    registry.register_typed(DeleteMessageAction).unwrap();

    assert_eq!(registry.count(), 4);
    assert!(registry.contains("send_message"));
    assert!(registry.contains("get_chat_history"));
    assert!(registry.contains("list_dialogs"));
    assert!(registry.contains("delete_message"));
    assert!(!registry.contains("unknown_action"));

    let messaging_actions = registry.list_by_category(ActionCategory::Messaging);
    assert_eq!(messaging_actions.len(), 3); // send_message, get_chat_history, delete_message

    let dialogs_actions = registry.list_by_category(ActionCategory::Dialogs);
    assert_eq!(dialogs_actions.len(), 1);
    assert_eq!(dialogs_actions[0].name(), "list_dialogs");
}

#[test]
fn test_action_json_schema_generation() {
    let action = SendMessageAction;
    let in_schema = action.input_schema();
    let out_schema = action.output_schema();

    let in_str = serde_json::to_string(&in_schema).unwrap();
    assert!(in_str.contains("chat_id"));
    assert!(in_str.contains("text"));

    let out_str = serde_json::to_string(&out_schema).unwrap();
    assert!(out_str.contains("message_id"));
    assert!(out_str.contains("pts"));
}

#[test]
fn test_openapi_export() {
    let registry = ActionRegistry::new();
    registry.register_typed(SendMessageAction).unwrap();
    registry.register_typed(DeleteMessageAction).unwrap();

    let spec = OpenApiExporter::export_spec(&registry, "GPUI Telegram Action Bus", "1.0.0", "http://localhost:8080");

    assert_eq!(spec["openapi"], "3.1.0");
    assert_eq!(spec["info"]["title"], "GPUI Telegram Action Bus");

    let paths = spec["paths"].as_object().unwrap();
    assert!(paths.contains_key("/api/v1/actions/send_message"));
    assert!(paths.contains_key("/api/v1/actions/delete_message"));

    let send_op = &paths["/api/v1/actions/send_message"]["post"];
    assert_eq!(send_op["operationId"], "send_message");
    assert_eq!(send_op["x-risk-level"], "low");
    assert_eq!(send_op["x-requires-approval"], false);

    let del_op = &paths["/api/v1/actions/delete_message"]["post"];
    assert_eq!(del_op["x-risk-level"], "high");
    assert_eq!(del_op["x-requires-approval"], true);
}

#[test]
fn test_hermes_tool_export() {
    let registry = ActionRegistry::new();
    registry.register_typed(SendMessageAction).unwrap();
    registry.register_typed(GetChatHistoryAction).unwrap();
    registry.register_typed(DeleteMessageAction).unwrap();

    let tools = HermesToolExporter::export_tools(&registry);
    assert_eq!(tools.len(), 3);

    let send_tool = tools.iter().find(|t| t.name == "send_message").unwrap();
    assert_eq!(send_tool.category, ActionCategory::Messaging);
    assert_eq!(send_tool.risk_level, ActionRiskLevel::Low);
    assert!(!send_tool.requires_approval);
    assert!(!send_tool.read_only);

    let hist_tool = tools.iter().find(|t| t.name == "get_chat_history").unwrap();
    assert!(hist_tool.read_only);

    let del_tool = tools.iter().find(|t| t.name == "delete_message").unwrap();
    assert!(del_tool.requires_approval);

    let openai_tools = HermesToolExporter::export_openai_tools(&registry);
    assert_eq!(openai_tools.len(), 3);
    assert_eq!(openai_tools[0]["type"], "function");
    assert!(openai_tools[0]["function"]["name"].is_string());

    let anthropic_tools = HermesToolExporter::export_anthropic_tools(&registry);
    assert_eq!(anthropic_tools.len(), 3);
    assert!(anthropic_tools[0]["input_schema"].is_object());
}

#[test]
fn test_mcp_tool_export() {
    let registry = ActionRegistry::new();
    registry.register_typed(SendMessageAction).unwrap();
    registry.register_typed(DeleteMessageAction).unwrap();

    let mcp_res = McpExporter::export_tools_list(&registry);
    assert_eq!(mcp_res.tools.len(), 2);

    let send_spec = mcp_res.tools.iter().find(|t| t.name == "send_message").unwrap();
    assert_eq!(send_spec.requires_approval, false);

    let del_spec = mcp_res.tools.iter().find(|t| t.name == "delete_message").unwrap();
    assert_eq!(del_spec.requires_approval, true);

    let jsonrpc = McpExporter::export_jsonrpc_list(&registry, json!(1));
    assert_eq!(jsonrpc["jsonrpc"], "2.0");
    assert_eq!(jsonrpc["id"], 1);
    assert!(jsonrpc["result"]["tools"].is_array());
}

#[test]
fn test_cli_subcommand_export() {
    let registry = ActionRegistry::new();
    registry.register_typed(SendMessageAction).unwrap();
    registry.register_typed(DeleteMessageAction).unwrap();

    let cli_export = CliExporter::export_cli_schema(&registry, "gpui-tg-cli", "1.0.0");
    assert_eq!(cli_export.program_name, "gpui-tg-cli");
    assert_eq!(cli_export.subcommands.len(), 2);

    let send_sub = cli_export.subcommands.iter().find(|s| s.name == "send_message").unwrap();
    assert_eq!(send_sub.kebab_name, "send-message");
    assert_eq!(send_sub.category, "messaging");
    assert!(!send_sub.requires_approval);

    let chat_id_arg = send_sub.arguments.iter().find(|a| a.name == "chat_id").unwrap();
    assert_eq!(chat_id_arg.flag, "--chat-id");
    assert_eq!(chat_id_arg.value_type, "integer");

    let reply_arg = send_sub.arguments.iter().find(|a| a.name == "reply_to_id").unwrap();
    assert_eq!(reply_arg.value_type, "integer");

    let del_sub = cli_export.subcommands.iter().find(|s| s.name == "delete_message").unwrap();
    assert_eq!(del_sub.kebab_name, "delete-message");
    assert_eq!(del_sub.risk_level, ActionRiskLevel::High);
    assert!(del_sub.requires_approval);
}

#[test]
fn test_action_execution_success_and_validation_error() {
    futures::executor::block_on(async {
        let registry = ActionRegistry::new();
        registry.register_typed(SendMessageAction).unwrap();

        let ctx = ActionContext::new().with_user(1001);
        let valid_input = json!({
            "chat_id": 42,
            "text": "Hello world from Action Bus!"
        });

        let res = registry.execute("send_message", ctx.clone(), valid_input).await;
        assert!(res.is_ok());
        let val = res.unwrap();
        assert_eq!(val["chat_id"], 42);
        assert_eq!(val["message_id"], 1);

        let invalid_input = json!({
            "chat_id": 42,
            "text": "   "
        });
        let err = registry.execute("send_message", ctx.clone(), invalid_input).await;
        assert!(matches!(err, Err(ActionError::InvalidInput(_))));

        let missing_field = json!({
            "text": "Hello"
        });
        let err2 = registry.execute("send_message", ctx, missing_field).await;
        assert!(matches!(err2, Err(ActionError::InvalidInput(_))));
    });
}

#[test]
fn test_high_risk_approval_detection() {
    let del: &dyn PlatformAction = &DeleteMessageAction;
    assert_eq!(del.risk_level(), ActionRiskLevel::High);
    assert!(del.risk_level().requires_explicit_confirmation());
    assert!(del.risk_level().is_sensitive());
    assert!(!del.risk_level().is_read_only());
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct NestedTarget {
    peer_id: i64,
    label: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ComplexTestInput {
    target: NestedTarget,
    optional_target: Option<NestedTarget>,
    opt_i64: Option<i64>,
    opt_u64: Option<u64>,
    opt_i32: Option<i32>,
    opt_bool: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct ComplexTestOutput {
    ok: bool,
}

struct ComplexTestAction;

impl TypedPlatformAction for ComplexTestAction {
    type Input = ComplexTestInput;
    type Output = ComplexTestOutput;

    fn name(&self) -> &'static str {
        "complex_action"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::System
    }

    fn description(&self) -> &'static str {
        "Test action for nested $ref and nullable types"
    }

    fn risk_level(&self) -> ActionRiskLevel {
        ActionRiskLevel::High
    }

    fn execute<'a>(
        &'a self,
        _ctx: ActionContext,
        _input: Self::Input,
    ) -> futures::future::BoxFuture<'a, Result<Self::Output, ActionError>> {
        Box::pin(async move { Ok(ComplexTestOutput { ok: true }) })
    }
}

#[test]
fn test_schema_consistency_across_quad_export() {
    let registry = ActionRegistry::new();
    registry.register_typed(SendMessageAction).unwrap();
    registry.register_typed(GetChatHistoryAction).unwrap();
    registry.register_typed(ListDialogsAction).unwrap();
    registry.register_typed(DeleteMessageAction).unwrap();
    registry.register_typed(ComplexTestAction).unwrap();

    let openapi = OpenApiExporter::export_spec(&registry, "Test", "1.0.0", "http://localhost");
    let hermes = HermesToolExporter::export_tools(&registry);
    let mcp = McpExporter::export_tools_list(&registry);
    let cli = CliExporter::export_cli_schema(&registry, "gpui-tg-cli", "1.0.0");

    // 1. OpenAPI documents 422 and 400 and matches Hermes/MCP schemas
    let send_op = &openapi["paths"]["/api/v1/actions/send_message"]["post"];
    assert!(send_op["responses"]["422"].is_object());
    assert!(send_op["responses"]["400"].is_object());
    assert!(send_op["responses"]["429"].is_object());

    let send_hermes = hermes.iter().find(|t| t.name == "send_message").unwrap();
    let send_mcp = mcp.tools.iter().find(|t| t.name == "send_message").unwrap();
    assert_eq!(
        send_op["requestBody"]["content"]["application/json"]["schema"],
        send_hermes.parameters_schema
    );
    assert_eq!(send_hermes.parameters_schema, send_mcp.input_schema);

    // 2. CLI resolves Option<i64>, Option<u64>, Option<i32>, Option<bool>, and $ref nested structs
    let send_cli = cli.subcommands.iter().find(|s| s.name == "send_message").unwrap();
    assert!(!send_cli.requires_approval);
    for field in ["reply_to_id", "topic_id", "client_nonce"] {
        let arg = send_cli.arguments.iter().find(|a| a.name == field).unwrap();
        assert_eq!(arg.value_type, "integer", "field {field} mismatch");
    }

    let hist_cli = cli.subcommands.iter().find(|s| s.name == "get_chat_history").unwrap();
    assert!(!hist_cli.requires_approval);
    for field in ["offset_id", "topic_id"] {
        let arg = hist_cli.arguments.iter().find(|a| a.name == field).unwrap();
        assert_eq!(arg.value_type, "integer", "field {field} mismatch");
    }

    let list_cli = cli.subcommands.iter().find(|s| s.name == "list_dialogs").unwrap();
    assert!(!list_cli.requires_approval);
    assert_eq!(
        list_cli.arguments.iter().find(|a| a.name == "folder_id").unwrap().value_type,
        "integer"
    );

    let complex_cli = cli.subcommands.iter().find(|s| s.name == "complex_action").unwrap();
    assert!(complex_cli.requires_approval);
    assert_eq!(
        complex_cli.arguments.iter().find(|a| a.name == "target").unwrap().value_type,
        "object"
    );
    assert_eq!(
        complex_cli.arguments.iter().find(|a| a.name == "optional_target").unwrap().value_type,
        "object"
    );
    assert_eq!(
        complex_cli.arguments.iter().find(|a| a.name == "opt_i64").unwrap().value_type,
        "integer"
    );
    assert_eq!(
        complex_cli.arguments.iter().find(|a| a.name == "opt_u64").unwrap().value_type,
        "integer"
    );
    assert_eq!(
        complex_cli.arguments.iter().find(|a| a.name == "opt_i32").unwrap().value_type,
        "integer"
    );
    assert_eq!(
        complex_cli.arguments.iter().find(|a| a.name == "opt_bool").unwrap().value_type,
        "boolean"
    );

    let ctx = ActionContext::new().with_user(1001);
    let extra_res = futures::executor::block_on(registry.execute(
        "send_message",
        ctx,
        json!({ "chat_id": 42, "text": "hello", "unexpected_field": 123 }),
    ));
    assert!(matches!(extra_res, Err(ActionError::InvalidInput(_))));
}

#[test]
fn test_high_risk_action_requires_approval_in_registry() {
    futures::executor::block_on(async {
        let registry = ActionRegistry::new();
        registry.register_typed(DeleteMessageAction).unwrap();

        let unapproved_ctx = ActionContext::new().with_user(1001);
        let err = registry
            .execute(
                "delete_message",
                unapproved_ctx,
                json!({ "chat_id": 42, "message_ids": [1, 2] }),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ActionError::Forbidden(_)));

        let approved_ctx = ActionContext::new().with_user(1001).with_approval(true);
        let ok = registry
            .execute(
                "delete_message",
                approved_ctx,
                json!({ "chat_id": 42, "message_ids": [1, 2] }),
            )
            .await
            .unwrap();
        assert_eq!(ok["deleted_count"], 2);
    });
}
