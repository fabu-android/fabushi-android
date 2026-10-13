use fabushi_mahayana_host::android_json_runtime::{AndroidHostMode, AndroidJsonHost};
use serde_json::json;

const TEST_ACCOUNT_FENCE: &str = "session:test:android";

fn sign_in_test_account(host: &mut AndroidJsonHost) {
    let started = host
        .dispatch("feature.auth.oauthStart", &json!({"provider":"google"}))
        .expect("test OAuth start must succeed");
    let attempt_id = started["attemptId"]
        .as_str()
        .expect("test OAuth start must return attemptId")
        .to_string();
    let completed = host
        .dispatch("feature.auth.oauthPoll", &json!({"attemptId":attempt_id}))
        .expect("test OAuth poll must succeed");
    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["auth"]["loggedIn"], true);
}

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
        sign_in_test_account(&mut host);
        first_id = create_agent(&mut host, "First");
        second_id = create_agent(&mut host, "Second");

        let written = host
            .dispatch(
                "feature.agent.channels.writeMetadata",
                &json!({"agentId":first_id,"platform":"slack","label":"Workspace A","accountFence":TEST_ACCOUNT_FENCE}),
            )
            .unwrap();
        assert_eq!(written["written"], true);

        let first = host
            .dispatch(
                "feature.agent.channels.listMetadata",
                &json!({"agentId":first_id,"accountFence":TEST_ACCOUNT_FENCE}),
            )
            .unwrap();
        assert_eq!(first.as_array().unwrap().len(), 1);
        assert_eq!(first[0]["platform"], "slack");
        assert_eq!(first[0]["label"], "Workspace A");

        let second = host
            .dispatch(
                "feature.agent.channels.listMetadata",
                &json!({"agentId":second_id,"accountFence":TEST_ACCOUNT_FENCE}),
            )
            .unwrap();
        assert!(second.as_array().unwrap().is_empty());
    }

    let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
    sign_in_test_account(&mut reopened);
    let restored = reopened
        .dispatch(
            "feature.agent.channels.listMetadata",
            &json!({"agentId":first_id,"accountFence":TEST_ACCOUNT_FENCE}),
        )
        .unwrap();
    assert_eq!(restored.as_array().unwrap().len(), 1);
    assert_eq!(restored[0]["platform"], "slack");

    let removed = reopened
        .dispatch(
            "feature.agent.channels.removeMetadata",
            &json!({"agentId":first_id,"platform":"slack","accountFence":TEST_ACCOUNT_FENCE}),
        )
        .unwrap();
    assert_eq!(removed["removed"], true);
    assert!(reopened
        .dispatch(
            "feature.agent.channels.listMetadata",
            &json!({"agentId":first_id,"accountFence":TEST_ACCOUNT_FENCE}),
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
    sign_in_test_account(&mut host);
    let agent_id = create_agent(&mut host, "Safe");

    let unknown = host.dispatch(
        "feature.agent.channels.writeMetadata",
        &json!({"agentId":"missing-agent","platform":"slack","label":"Nope","accountFence":TEST_ACCOUNT_FENCE}),
    );
    assert!(unknown.unwrap_err().contains("agent not found"));

    assert!(host
        .dispatch(
            "feature.agent.channels.listMetadata",
            &json!({"agentId":agent_id,"accountFence":"stale-account"}),
        )
        .unwrap_err()
        .contains("stale Agent channel account fence"));

    let unsafe_platform = host
        .dispatch(
            "feature.agent.channels.writeMetadata",
            &json!({"agentId":agent_id,"platform":"../escape","label":"Nope","accountFence":TEST_ACCOUNT_FENCE}),
        )
        .unwrap();
    assert_eq!(unsafe_platform["written"], false);
    assert!(host
        .dispatch(
            "feature.agent.channels.listMetadata",
            &json!({"agentId":agent_id,"accountFence":TEST_ACCOUNT_FENCE}),
        )
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
}
