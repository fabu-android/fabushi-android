use fabushi_mahayana_host::android_json_runtime::{AndroidHostMode, AndroidJsonHost};
use serde_json::json;

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

#[test]
fn handoff_survives_process_reopen_and_logout_fences_old_account_state() {
    let root = tempfile::tempdir().unwrap();
    let request_id;
    {
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        sign_in_test_account(&mut host);
        let started = host
            .dispatch(
                "feature.agent.handoff.start",
                &json!({"agentId":"agent-1","instruction":"Complete the protected login"}),
            )
            .unwrap();
        assert_eq!(started["status"], "started");
        request_id = started["requestId"].as_str().unwrap().to_string();

        let duplicate = host
            .dispatch(
                "feature.agent.handoff.start",
                &json!({"agentId":"agent-1","instruction":"Do not replace the live handoff"}),
            )
            .unwrap();
        assert_eq!(duplicate["status"], "already-pending");
        assert_eq!(duplicate["requestId"], request_id);
    }

    let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
    sign_in_test_account(&mut reopened);
    let restored = reopened
        .dispatch(
            "feature.agent.handoff.status",
            &json!({"agentId":"agent-1"}),
        )
        .unwrap();
    assert_eq!(restored["requestId"], request_id);

    assert_eq!(
        reopened
            .dispatch(
                "feature.agent.handoff.attachSnapshot",
                &json!({
                    "agentId":"agent-1",
                    "requestId":"stale-request",
                    "snapshotDataUrl":"data:image/webp;base64,stale"
                }),
            )
            .unwrap()["attached"],
        false,
    );

    reopened.dispatch("feature.auth.logout", &json!({})).unwrap();
    sign_in_test_account(&mut reopened);
    assert!(
        reopened
            .dispatch(
                "feature.agent.handoff.status",
                &json!({"agentId":"agent-1"}),
            )
            .unwrap()
            .is_null()
    );
}

#[test]
fn handoff_end_is_request_fenced_and_durable() {
    let root = tempfile::tempdir().unwrap();
    let request_id;
    {
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        sign_in_test_account(&mut host);
        let started = host
            .dispatch(
                "feature.agent.handoff.start",
                &json!({"agentId":"agent-2","instruction":"Review the remote result"}),
            )
            .unwrap();
        request_id = started["requestId"].as_str().unwrap().to_string();

        assert_eq!(
            host.dispatch(
                "feature.agent.handoff.end",
                &json!({"agentId":"agent-2","requestId":"stale"}),
            )
            .unwrap()["ended"],
            false,
        );

        let ended = host
            .dispatch(
                "feature.agent.handoff.end",
                &json!({
                    "agentId":"agent-2",
                    "requestId":request_id,
                    "resolution":"completed",
                    "trigger":"user"
                }),
            )
            .unwrap();
        assert_eq!(ended["ended"], true);
        assert_eq!(ended["requestId"], request_id);
        assert_eq!(ended["resolution"], "completed");
    }

    let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
    sign_in_test_account(&mut reopened);
    assert!(
        reopened
            .dispatch(
                "feature.agent.handoff.status",
                &json!({"agentId":"agent-2"}),
            )
            .unwrap()
            .is_null()
    );
}
