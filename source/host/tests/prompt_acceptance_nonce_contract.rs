use fabushi_mahayana_host::android_json_runtime::{AndroidHostMode, AndroidJsonHost};
use serde_json::json;
use std::{thread, time::Duration};

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

fn settle(host: &mut AndroidJsonHost, operation_id: &str) -> serde_json::Value {
    for _ in 0..128 {
        let event = host.dispatch("feature.receive", &json!({})).unwrap();
        if event["operationId"] == operation_id
            && matches!(
                event["type"].as_str(),
                Some("operation.completed" | "operation.failed" | "operation.interrupted")
            )
        {
            return event;
        }
        if event.as_object().is_some_and(|object| object.is_empty()) {
            thread::sleep(Duration::from_millis(5));
        }
    }
    panic!("operation {operation_id} did not settle");
}

#[test]
fn accepted_client_nonce_survives_echo_reopen_and_rejects_payload_reuse() {
    let root = tempfile::tempdir().unwrap();

    {
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
    sign_in_test_account(&mut host);
        let accepted = host
            .dispatch(
                "feature.execute",
                &json!({"command":{
                    "type":"chat.send",
                    "requestId":"request-first",
                    "clientNonce":"nonce-stable",
                    "agentId":"mahayana-assistant",
                    "text":"one durable prompt"
                }}),
            )
            .unwrap();
        let operation_id = accepted["operationId"].as_str().unwrap();
        let terminal = settle(&mut host, operation_id);
        assert_eq!(terminal["type"], "operation.completed");
        let transcript = host
            .dispatch("feature.transcript.snapshot", &json!({}))
            .unwrap();
        assert_eq!(transcript.as_array().unwrap().len(), 2);
    }

    let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
    sign_in_test_account(&mut reopened);
    let replay = reopened
        .dispatch(
            "feature.execute",
            &json!({"command":{
                "type":"chat.send",
                "requestId":"request-replay",
                "clientNonce":"nonce-stable",
                "agentId":"mahayana-assistant",
                "text":"one durable prompt"
            }}),
        )
        .unwrap();
    let replay_operation = replay["operationId"].as_str().unwrap();
    let terminal = settle(&mut reopened, replay_operation);
    assert_eq!(terminal["type"], "operation.completed");
    assert_eq!(terminal["deduped"], true);
    let transcript = reopened
        .dispatch("feature.transcript.snapshot", &json!({}))
        .unwrap();
    assert_eq!(
        transcript.as_array().unwrap().len(),
        2,
        "accepted nonce replay must not append a second user prompt or run"
    );

    let mismatch = reopened
        .dispatch(
            "feature.execute",
            &json!({"command":{
                "type":"chat.send",
                "requestId":"request-mismatch",
                "clientNonce":"nonce-stable",
                "agentId":"mahayana-assistant",
                "text":"different payload"
            }}),
        )
        .unwrap_err();
    assert!(mismatch.contains("NONCE_DIGEST_MISMATCH"));
    let transcript = reopened
        .dispatch("feature.transcript.snapshot", &json!({}))
        .unwrap();
    assert_eq!(transcript.as_array().unwrap().len(), 2);
}
