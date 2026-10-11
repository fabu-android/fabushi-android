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

#[test]
fn staged_attachment_flows_through_acknowledgement_and_canonical_echo() {
    let root = tempfile::tempdir().unwrap();
    let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);

    let begun_attachment = host
        .dispatch(
            "feature.composerDraft.attachment.begin",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "name":"notes.txt",
                "mimeType":"text/plain",
                "sizeBytes":5
            }),
        )
        .unwrap();
    let attachment_id = begun_attachment["attachmentId"].as_str().unwrap().to_string();
    let appended = host
        .dispatch(
            "feature.composerDraft.attachment.append",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "attachmentId":attachment_id,
                "offset":0,
                "dataBase64":"aGVsbG8="
            }),
        )
        .unwrap();
    assert_eq!(appended["nextOffset"], 5);
    let finished = host
        .dispatch(
            "feature.composerDraft.attachment.finish",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "attachmentId":attachment_id
            }),
        )
        .unwrap();
    let attachment = finished["attachment"].clone();
    assert_eq!(attachment["name"], "notes.txt");
    assert_eq!(finished["draft"]["attachments"].as_array().unwrap().len(), 1);

    let nonce = "attachment-contract-nonce";
    let began = host
        .dispatch(
            "feature.sendAcknowledgement.begin",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "nonce":nonce,
                "text":"",
                "attachments":[attachment],
                "entries":[{
                    "id":nonce,
                    "kind":"message",
                    "clientNonce":nonce
                }]
            }),
        )
        .unwrap();
    assert_eq!(began["phase"], "pending");
    assert_eq!(began["attachments"][0]["name"], "notes.txt");

    let dispatched = host
        .dispatch(
            "feature.sendAcknowledgement.dispatch",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "nonce":nonce,
                "generation":began["generation"]
            }),
        )
        .unwrap();
    assert_eq!(dispatched["acknowledgement"]["phase"], "accepted-awaiting-echo");

    let transcript = host.dispatch("feature.transcript.snapshot", &json!({})).unwrap();
    let user = transcript
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == nonce)
        .expect("attachment-only canonical user echo");
    assert_eq!(user["content"], "");
    assert_eq!(user["attachments"][0]["name"], "notes.txt");
    assert_eq!(user["attachments"][0]["size"], 5);

    let projection = host
        .dispatch(
            "feature.sendAcknowledgement.snapshot",
            &json!({"accountSlot":"slot-a","agentId":"agent-a"}),
        )
        .unwrap();
    assert!(projection["records"].as_array().unwrap().is_empty());
}



#[test]
fn queued_send_is_durable_idempotent_and_only_queued_may_be_cancelled() {
    let root = tempfile::tempdir().unwrap();
    let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
    let nonce = "queued-contract-nonce";
    let begun = host
        .dispatch(
            "feature.sendAcknowledgement.begin",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "nonce":nonce,
                "text":"queue me",
                "entries":[{"id":nonce,"kind":"message","clientNonce":nonce}]
            }),
        )
        .unwrap();
    let generation = begun["generation"].as_u64().unwrap();

    let queued = host
        .dispatch(
            "feature.sendAcknowledgement.queue",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "nonce":nonce,
                "generation":generation
            }),
        )
        .unwrap();
    assert_eq!(queued["phase"], "queued");

    let duplicate = host
        .dispatch(
            "feature.sendAcknowledgement.queue",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "nonce":nonce,
                "generation":generation
            }),
        )
        .unwrap();
    assert_eq!(duplicate["phase"], "queued");

    let snapshot = host
        .dispatch(
            "feature.sendAcknowledgement.snapshot",
            &json!({"accountSlot":"slot-a","agentId":"agent-a"}),
        )
        .unwrap();
    assert_eq!(snapshot["records"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["records"][0]["phase"], "queued");

    let cancelled = host
        .dispatch(
            "feature.sendAcknowledgement.cancelQueued",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "nonce":nonce,
                "generation":generation
            }),
        )
        .unwrap();
    assert_eq!(cancelled["cancelled"], true);
    assert_eq!(cancelled["acknowledgement"]["phase"], "queued");

    let after = host
        .dispatch(
            "feature.sendAcknowledgement.snapshot",
            &json!({"accountSlot":"slot-a","agentId":"agent-a"}),
        )
        .unwrap();
    assert!(after["records"].as_array().unwrap().is_empty());
    assert!(host
        .dispatch(
            "feature.sendAcknowledgement.cancelQueued",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "nonce":nonce,
                "generation":generation
            }),
        )
        .is_err());
}

#[test]
fn cancel_queued_rejects_pending_and_stale_generation_without_mutation() {
    let root = tempfile::tempdir().unwrap();
    let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
    let nonce = "pending-contract-nonce";
    let begun = host
        .dispatch(
            "feature.sendAcknowledgement.begin",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "nonce":nonce,
                "text":"pending",
                "entries":[{"id":nonce,"kind":"message","clientNonce":nonce}]
            }),
        )
        .unwrap();
    let generation = begun["generation"].as_u64().unwrap();

    assert!(host
        .dispatch(
            "feature.sendAcknowledgement.cancelQueued",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "nonce":nonce,
                "generation":generation
            }),
        )
        .is_err());

    host.dispatch(
        "feature.sendAcknowledgement.queue",
        &json!({
            "accountSlot":"slot-a",
            "agentId":"agent-a",
            "nonce":nonce,
            "generation":generation
        }),
    )
    .unwrap();
    assert!(host
        .dispatch(
            "feature.sendAcknowledgement.cancelQueued",
            &json!({
                "accountSlot":"slot-a",
                "agentId":"agent-a",
                "nonce":nonce,
                "generation":generation + 1
            }),
        )
        .is_err());
    let projection = host
        .dispatch(
            "feature.sendAcknowledgement.snapshot",
            &json!({"accountSlot":"slot-a","agentId":"agent-a"}),
        )
        .unwrap();
    assert_eq!(projection["records"][0]["phase"], "queued");
}
