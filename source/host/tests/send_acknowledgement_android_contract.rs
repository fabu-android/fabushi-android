use fabushi_mahayana_host::android_json_runtime::{AndroidHostMode, AndroidJsonHost};
use serde_json::json;

#[test]
fn acknowledged_send_runs_through_host_and_reconciles_canonical_echo() {
    let root = tempfile::tempdir().unwrap();
    let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
    let nonce = "android-contract-nonce-a";

    let began = host
        .dispatch(
            "feature.sendAcknowledgement.begin",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "nonce":nonce,
                "text":"hello durable acknowledgement",
                "entries":[{
                    "id":nonce,
                    "kind":"message",
                    "clientNonce":nonce
                }]
            }),
        )
        .unwrap();
    assert_eq!(began["phase"], "pending");
    let generation = began["generation"].as_u64().unwrap();

    let dispatched = host
        .dispatch(
            "feature.sendAcknowledgement.dispatch",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "nonce":nonce,
                "generation":generation
            }),
        )
        .unwrap();
    assert_eq!(
        dispatched["acknowledgement"]["phase"],
        "accepted-awaiting-echo"
    );
    assert!(
        dispatched["accepted"]["operationId"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );

    let transcript = host.dispatch("feature.transcript.snapshot", &json!({})).unwrap();
    let user = transcript
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == nonce)
        .expect("canonical user echo");
    assert_eq!(user["clientNonce"], nonce);
    assert_eq!(user["agentId"], "agent-a");

    let projection = host
        .dispatch(
            "feature.sendAcknowledgement.snapshot",
            &json!({"accountSlot":"slot-a","agentId":"agent-a"}),
        )
        .unwrap();
    assert!(
        projection["records"].as_array().unwrap().is_empty(),
        "canonical echo must settle the optimistic acknowledgement without replay"
    );
}

#[test]
fn acknowledgement_generation_and_agent_scope_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
    let began = host
        .dispatch(
            "feature.sendAcknowledgement.begin",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "nonce":"nonce-a",
                "text":"hello",
                "entries":[{"id":"nonce-a","kind":"message","clientNonce":"nonce-a"}]
            }),
        )
        .unwrap();
    let generation = began["generation"].as_u64().unwrap();

    assert!(host
        .dispatch(
            "feature.sendAcknowledgement.dispatch",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-b",
                "nonce":"nonce-a",
                "generation":generation
            }),
        )
        .is_err());
    assert!(host
        .dispatch(
            "feature.sendAcknowledgement.dispatch",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "nonce":"nonce-a",
                "generation":generation + 1
            }),
        )
        .is_err());
}
