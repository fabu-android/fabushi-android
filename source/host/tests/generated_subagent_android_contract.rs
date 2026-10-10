use fabushi_mahayana_host::android_json_runtime::{AndroidHostMode, AndroidJsonHost};
use fabushi_mahayana_host::runner::{
    AndroidRoutedToolBridge, DurableSubagentOwner, GeneratedChildToolClass,
    GeneratedChildToolRegistry, SubagentFrozenTurnConfig, SubagentToolBridge,
    SubagentToolContext, TurnSubagentCapabilityProjection,
};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[test]
fn shipping_host_routes_generated_subagent_tools_through_one_rust_owner() {
    let root = tempfile::tempdir().unwrap();
    let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
    let params = json!({
        "toolName":"Task",
        "toolCallId":"tool-contract-1",
        "parentAgentId":"parent-agent",
        "parentRequestId":"parent-turn-1",
        "rootParentRequestId":"root-turn-1",
        "model":"default",
        "boxId":"android-local",
        "arguments":{
            "prompt":"research the durable lifecycle",
            "subagent_type":"general-purpose"
        }
    });
    let first = host
        .dispatch("feature.agent.subagent.tool", &params)
        .expect("Task must enter shipping Host tool graph");
    assert_eq!(first["subagentRequestId"], "subagent:tool-contract-1");
    assert_eq!(first["duplicate"], false);

    let duplicate = host
        .dispatch("feature.agent.subagent.tool", &params)
        .expect("stable duplicate must be idempotent");
    assert_eq!(duplicate["subagentId"], first["subagentId"]);
    assert_eq!(duplicate["duplicate"], true);

    let list = host
        .dispatch(
            "feature.agent.subagent.tool",
            &json!({
                "toolName":"CheckSubagent",
                "toolCallId":"check-1",
                "parentAgentId":"parent-agent",
                "parentRequestId":"parent-turn-1",
                "arguments":{}
            }),
        )
        .expect("CheckSubagent must share the same owner");
    assert!(list.is_array());
}

#[test]
fn malformed_turn_capability_projection_is_rejected_before_transcript_persistence() {
    let root = tempfile::tempdir().unwrap();
    {
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        let error = host
            .dispatch(
                "feature.execute",
                &json!({
                    "command":{
                        "type":"chat.send",
                        "requestId":"malformed-capability-projection",
                        "agentId":"mahayana-assistant",
                        "text":"this rejected turn must never become durable",
                        "coordinatorSubagentCapabilities":{
                            "remoteBoxAvailable":true,
                            "remoteBoxHasDesktop":false,
                            "browserUseEnabled":true
                        }
                    }
                }),
            )
            .expect_err("inconsistent coordinator capability projection must fail closed");
        assert!(
            error.contains("browserUseEnabled requires a trusted remote box with desktop capability"),
            "unexpected rejection: {error}"
        );
        assert_eq!(
            host.dispatch("feature.transcript.snapshot", &json!({}))
                .expect("transcript snapshot"),
            json!([]),
            "capability validation must happen before canonical transcript mutation"
        );
    }

    let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
    assert_eq!(
        reopened
            .dispatch("feature.transcript.snapshot", &json!({}))
            .expect("reopened transcript snapshot"),
        json!([]),
        "rejected capability projection must not leave a durable ghost message"
    );
}


#[derive(Clone)]
struct ContractChildAdapter {
    name: &'static str,
}

impl AndroidRoutedToolBridge for ContractChildAdapter {
    fn list_tools(&self) -> Result<Vec<Value>, String> {
        Ok(vec![json!({
            "type":"function",
            "name":self.name,
            "description":"focused child execution adapter",
            "parameters":{"type":"object","additionalProperties":false}
        })])
    }

    fn call_tool(&self, name: &str, _args: Value, tool_call_id: &str) -> Result<Value, String> {
        if name != self.name || tool_call_id.trim().is_empty() {
            return Err("unexpected child adapter invocation".into());
        }
        Ok(json!({"adapter":self.name,"ok":true}))
    }
}

fn child_context(capabilities: TurnSubagentCapabilityProjection) -> SubagentToolContext {
    SubagentToolContext {
        parent_agent_id: "parent-agent".into(),
        parent_request_id: "parent-turn".into(),
        root_parent_request_id: Some("root-turn".into()),
        account_fence: "session:test:android".into(),
        box_id: "trusted-box".into(),
        quiet_origin: None,
        frozen_turn: SubagentFrozenTurnConfig {
            provider_id: "android-host-inference".into(),
            model_id: "deepseek-chat".into(),
            tool_names: Vec::new(),
            allowed_subagent_types: vec!["general-purpose".into(), "computeruse".into(), "browseruse".into()],
            privacy_mode: "no-storage".into(),
            summarization_binding_id: "android-host-inference:same-provider".into(),
        },
        child_capabilities: capabilities,
    }
}

#[test]
fn generated_child_projection_is_role_scoped_and_parent_controls_are_unreachable() {
    let root = tempfile::tempdir().unwrap();
    let owner = Arc::new(Mutex::new(
        DurableSubagentOwner::open(root.path().join("child-tools.json"), 1).unwrap(),
    ));
    let registry = GeneratedChildToolRegistry::default()
        .with_adapter(
            GeneratedChildToolClass::Box,
            Arc::new(ContractChildAdapter { name: "BoxRead" }),
        )
        .with_adapter(
            GeneratedChildToolClass::Computer,
            Arc::new(ContractChildAdapter { name: "Computer" }),
        )
        .with_adapter(
            GeneratedChildToolClass::Browser,
            Arc::new(ContractChildAdapter { name: "Browser" }),
        );
    let bridge = SubagentToolBridge::new(Arc::clone(&owner)).with_generated_child_tools(registry);
    let capabilities = TurnSubagentCapabilityProjection {
        multitask_enabled: true,
        remote_box_available: true,
        remote_box_has_desktop: true,
        browser_use_enabled: true,
    };
    let context = child_context(capabilities);

    let computer = bridge
        .project_generated_child_tool_names("computeruse", &capabilities)
        .unwrap();
    assert_eq!(computer, vec!["BoxRead".to_string(), "Computer".to_string()]);
    let browser = bridge
        .project_generated_child_tool_names("browseruse", &capabilities)
        .unwrap();
    assert_eq!(
        browser,
        vec!["BoxRead".to_string(), "Browser".to_string(), "Computer".to_string()]
    );
    let general = bridge
        .project_generated_child_tool_names("general-purpose", &capabilities)
        .unwrap();
    assert_eq!(general, vec!["BoxRead".to_string()]);

    let result = bridge
        .call(
            "Task",
            &json!({"prompt":"inspect","subagent_type":"browseruse"}),
            "child-projection-call",
            &context,
            2,
        )
        .unwrap();
    let launch = result.launch.expect("Task launch");
    let frozen = launch.record.frozen_turn.as_ref().expect("frozen child turn");
    assert_eq!(
        frozen.tool_names,
        vec!["BoxRead".to_string(), "Browser".to_string(), "Computer".to_string()]
    );
    let child = bridge.generated_child_routed_tools(&frozen.tool_names).unwrap();
    assert_eq!(child.list_tools().unwrap().len(), 3);
    assert_eq!(
        child.call_tool("Browser", json!({}), "child-tool-1").unwrap()["ok"],
        true
    );
    assert!(child.call_tool("CheckSubagent", json!({}), "child-tool-2").is_err());
}

#[test]
fn generated_child_projection_rejects_control_injection_duplicate_and_revoked_adapter() {
    let root = tempfile::tempdir().unwrap();
    let owner = Arc::new(Mutex::new(
        DurableSubagentOwner::open(root.path().join("child-tools.json"), 1).unwrap(),
    ));
    let capabilities = TurnSubagentCapabilityProjection {
        multitask_enabled: false,
        remote_box_available: true,
        remote_box_has_desktop: true,
        browser_use_enabled: true,
    };

    let injected = SubagentToolBridge::new(Arc::clone(&owner)).with_generated_child_tools(
        GeneratedChildToolRegistry::default().with_adapter(
            GeneratedChildToolClass::Box,
            Arc::new(ContractChildAdapter { name: "CheckSubagent" }),
        ),
    );
    let context = child_context(capabilities);
    assert!(injected
        .call(
            "Task",
            &json!({"prompt":"must fail before persistence","subagent_type":"general-purpose"}),
            "injected-control",
            &context,
            2,
        )
        .unwrap_err()
        .contains("parent-only control tool"));
    assert!(owner.lock().unwrap().all_records().is_empty());

    let duplicate = SubagentToolBridge::new(Arc::clone(&owner)).with_generated_child_tools(
        GeneratedChildToolRegistry::default()
            .with_adapter(
                GeneratedChildToolClass::Box,
                Arc::new(ContractChildAdapter { name: "BoxRead" }),
            )
            .with_adapter(
                GeneratedChildToolClass::Box,
                Arc::new(ContractChildAdapter { name: "BoxRead" }),
            ),
    );
    assert!(duplicate
        .project_generated_child_tool_names("general-purpose", &capabilities)
        .unwrap_err()
        .contains("duplicate generated child tool identity"));

    let authorized = SubagentToolBridge::new(Arc::clone(&owner)).with_generated_child_tools(
        GeneratedChildToolRegistry::default().with_adapter(
            GeneratedChildToolClass::Box,
            Arc::new(ContractChildAdapter { name: "BoxRead" }),
        ),
    );
    let frozen = authorized
        .project_generated_child_tool_names("general-purpose", &capabilities)
        .unwrap();
    assert_eq!(frozen, vec!["BoxRead".to_string()]);

    let revoked = SubagentToolBridge::new(Arc::clone(&owner));
    assert!(revoked
        .generated_child_routed_tools(&frozen)
        .unwrap_err()
        .contains("adapter is unavailable"));

    let unavailable_remote = TurnSubagentCapabilityProjection::default();
    assert!(authorized
        .project_generated_child_tool_names("computeruse", &unavailable_remote)
        .unwrap_err()
        .contains("trusted remote box"));
}

#[test]
fn typed_android_port_and_jni_route_generated_subagent_calls_through_coordinator_host() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let port = fs::read_to_string(
        root.join("../android-preload/src/main/kotlin/com/ombhrum/fabushi/androidpreload/runtime/AndroidCoordinatorPort.kt")
    ).expect("typed Android Coordinator port");
    let android_main = fs::read_to_string(
        root.join("../android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt")
    ).expect("shipping Android Coordinator runtime");
    let jni = fs::read_to_string(root.join("../android-host-jni/src/lib.rs"))
        .expect("JNI production composition");
    let host = fs::read_to_string(root.join("src/android_json_runtime.rs"))
        .expect("shipping Rust Host");

    assert!(port.contains("fun agentSubagentTool(params: JSONObject): JSONObject"));
    assert!(port.contains("fun agentSubagentReconcile(params: JSONObject): JSONObject"));
    assert!(android_main.contains("\"feature.agent.subagent.tool\","));
    assert!(android_main.contains("AgentTurnCapabilityProjection.forSubagentTool(params)"));
    assert!(android_main.contains("host.request(\"feature.agent.subagent.reconcile\", params)"));
    assert!(jni.contains("self.coordinator.request(request)"));
    assert!(host.contains("\"feature.agent.subagent.tool\" => self.agent_subagent_tool(params)"));
    assert!(host.contains("spawn_generated_subagent("));
    assert!(
        !android_main.contains("mutableMapOf<String, DurableSubagent"),
        "Kotlin must not create a second canonical subagent registry"
    );
}
