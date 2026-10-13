use fabushi_mahayana_host::android_json_runtime::{AndroidHostMode, AndroidJsonHost};
use serde_json::json;

fn create_agent(host: &mut AndroidJsonHost, name: &str) -> String {
    host.dispatch("createAgent", &json!({"name":name,"description":""}))
        .unwrap()["agent"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn channel_metadata_is_agent_scoped_and_survives_reopen() {
    let root = tempfile::tempdir().unwrap();
    let (first_id, second_id);
    {
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        first_id = create_agent(&mut host, "First");
        second_id = create_agent(&mut host, "Second");

        let written = host
            .dispatch(
                "feature.agent.channels.writeMetadata",
                &json!({"agentId":first_id,"platform":"slack","label":"Workspace A"}),
            )
            .unwrap();
        assert_eq!(written["written"], true);

        let first = host
            .dispatch(
                "feature.agent.channels.listMetadata",
                &json!({"agentId":first_id}),
            )
            .unwrap();
        assert_eq!(first.as_array().unwrap().len(), 1);
        assert_eq!(first[0]["platform"], "slack");
        assert_eq!(first[0]["label"], "Workspace A");

        let second = host
            .dispatch(
                "feature.agent.channels.listMetadata",
                &json!({"agentId":second_id}),
            )
            .unwrap();
        assert!(second.as_array().unwrap().is_empty());
    }

    let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
    let restored = reopened
        .dispatch(
            "feature.agent.channels.listMetadata",
            &json!({"agentId":first_id}),
        )
        .unwrap();
    assert_eq!(restored.as_array().unwrap().len(), 1);
    assert_eq!(restored[0]["platform"], "slack");

    let removed = reopened
        .dispatch(
            "feature.agent.channels.removeMetadata",
            &json!({"agentId":first_id,"platform":"slack"}),
        )
        .unwrap();
    assert_eq!(removed["removed"], true);
    assert!(reopened
        .dispatch(
            "feature.agent.channels.listMetadata",
            &json!({"agentId":first_id}),
        )
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn channel_metadata_rejects_unknown_agents_and_unsafe_platform_ids() {
    let root = tempfile::tempdir().unwrap();
    let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
    let agent_id = create_agent(&mut host, "Safe");

    let unknown = host.dispatch(
        "feature.agent.channels.writeMetadata",
        &json!({"agentId":"missing-agent","platform":"slack","label":"Nope"}),
    );
    assert!(unknown.unwrap_err().contains("agent not found"));

    let unsafe_platform = host
        .dispatch(
            "feature.agent.channels.writeMetadata",
            &json!({"agentId":agent_id,"platform":"../escape","label":"Nope"}),
        )
        .unwrap();
    assert_eq!(unsafe_platform["written"], false);
    assert!(host
        .dispatch(
            "feature.agent.channels.listMetadata",
            &json!({"agentId":agent_id}),
        )
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
}
