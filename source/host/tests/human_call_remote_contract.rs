use fabushi_mahayana_host::android_json_runtime::{AndroidHostMode, AndroidJsonHost};
use serde_json::json;

fn logged_in_host(root: &std::path::Path) -> AndroidJsonHost {
    let mut host = AndroidJsonHost::new(root, AndroidHostMode::Test);
    let oauth = host
        .dispatch("feature.auth.oauthStart", &json!({"provider":"google"}))
        .expect("test OAuth start should be available");
    let completed = host
        .dispatch(
            "feature.auth.oauthPoll",
            &json!({"attemptId":oauth["attemptId"]}),
        )
        .expect("test OAuth poll should complete");
    assert_eq!(completed["status"], "completed");
    host
}

#[test]
fn public_host_dispatch_preserves_remote_call_watermark_and_replay_fencing() {
    let root = tempfile::tempdir().unwrap();
    let mut host = logged_in_host(root.path());

    let created = host
        .dispatch(
            "feature.call.create",
            &json!({
                "callId":"call-remote-contract",
                "requestId":"call-request-remote-contract",
                "scopeId":"conversation-1",
                "participantIds":["human:android-test","human:peer"]
            }),
        )
        .expect("call creation should use the canonical HumanCallRuntime");
    assert_eq!(created["generation"], 0);

    host.dispatch(
        "feature.call.transition",
        &json!({
            "callId":"call-remote-contract",
            "generation":0,
            "action":"accept"
        }),
    )
    .expect("invited call should enter negotiation");

    let remote = host
        .dispatch(
            "feature.call.signal.receiveRemote",
            &json!({
                "callId":"call-remote-contract",
                "generation":0,
                "eventSeq":4,
                "senderDeviceId":"remote-device",
                "kind":"offer",
                "payload":{"sdp":"remote-offer"}
            }),
        )
        .expect("remote sync may resume from a monotonic gap");
    assert_eq!(remote["seq"], 4);

    let watermark = host
        .dispatch(
            "feature.call.signal.remoteWatermark",
            &json!({"callId":"call-remote-contract","generation":0}),
        )
        .expect("watermark should be projected through Host");
    assert_eq!(watermark["eventSeq"], 4);

    let replay = host
        .dispatch(
            "feature.call.signal.receiveRemote",
            &json!({
                "callId":"call-remote-contract",
                "generation":0,
                "eventSeq":4,
                "senderDeviceId":"remote-device",
                "kind":"offer",
                "payload":{"sdp":"remote-offer"}
            }),
        )
        .expect("exact remote replay should be idempotent");
    assert_eq!(replay["seq"], 4);

    let conflict = host
        .dispatch(
            "feature.call.signal.receiveRemote",
            &json!({
                "callId":"call-remote-contract",
                "generation":0,
                "eventSeq":4,
                "senderDeviceId":"remote-device",
                "kind":"offer",
                "payload":{"sdp":"conflicting-offer"}
            }),
        )
        .expect_err("same remote event sequence with a different payload must fail closed");
    assert!(conflict.contains("conflicts"));

    host.dispatch(
        "feature.call.transition",
        &json!({
            "callId":"call-remote-contract",
            "generation":0,
            "action":"connected"
        }),
    )
    .expect("negotiating call should connect");
    let reconnected = host
        .dispatch(
            "feature.call.transition",
            &json!({
                "callId":"call-remote-contract",
                "generation":0,
                "action":"reconnect"
            }),
        )
        .expect("connected call should enter reconnect generation");
    assert_eq!(reconnected["generation"], 1);

    let reset = host
        .dispatch(
            "feature.call.signal.remoteWatermark",
            &json!({"callId":"call-remote-contract","generation":1}),
        )
        .expect("new generation gets a fresh remote watermark");
    assert_eq!(reset["eventSeq"], 0);

    let stale_generation = host
        .dispatch(
            "feature.call.signal.receiveRemote",
            &json!({
                "callId":"call-remote-contract",
                "generation":0,
                "eventSeq":5,
                "senderDeviceId":"remote-device",
                "kind":"candidate",
                "payload":{"candidate":"old-generation"}
            }),
        )
        .expect_err("old-generation callbacks must be fenced");
    assert!(stale_generation.contains("generation"));
}
