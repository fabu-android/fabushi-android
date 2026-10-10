use fabushi_mahayana_host::android_json_runtime::{AndroidHostMode, AndroidJsonHost};
use serde_json::json;
use std::fs;
use std::path::PathBuf;

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
