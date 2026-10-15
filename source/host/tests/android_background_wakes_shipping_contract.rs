use std::fs;
use std::path::PathBuf;

fn source_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("host crate must live below source/")
        .to_path_buf()
}

fn source(path: &str) -> String {
    fs::read_to_string(source_root().join(path)).expect("shipping source must be readable")
}

#[test]
fn shipping_send_message_routes_channel_side_effects_through_host_owned_delivery() {
    let send = source("host/src/runner/send_message.rs");
    let runtime = source("host/src/android_json_runtime.rs");

    for needle in [
        "HostChannelDelivery",
        "channel-send-attempt",
        "channel-send-success",
        "channel-send-failure",
        "Channel delivery outcome is unknown after process recovery",
        "with_send_message_tools_counted_to_target_with_channel_delivery",
    ] {
        assert!(send.contains(needle), "SendMessage shipping owner must contain {needle}");
    }
    assert!(runtime.contains("channel_delivery: HostChannelDelivery"));
    assert!(runtime.contains("background_wakes: Arc<Mutex<BackgroundWakes<Value>>>"));
    assert!(runtime.contains("channel_failure_sink: ChannelDeliveryFailureSink"));
    assert!(runtime.contains("with_send_message_tools_counted_to_target_with_channel_delivery"));
    assert!(!runtime.contains("ComposeChannelDelivery"));
    assert!(!runtime.contains("JniChannelDelivery"));
}

#[test]
fn failed_channel_delivery_rebuilds_a_durable_hidden_revival_without_blind_retry() {
    let runtime = source("host/src/android_json_runtime.rs");
    for needle in [
        "recover_channel_failure_wakes",
        "channel-failure-revival-claim",
        "build_channel_delivery_failure_wake_prompt",
        "requestSource\":\"channel-failure",
        "skipAckObligation\":true",
        "turn_journal",
        "BackgroundWakes::enqueue",
        "drain_channel_failure_revival_once",
    ] {
        assert!(runtime.contains(needle), "shipping revival must contain {needle}");
    }
    assert!(runtime.contains("self.drain_channel_failure_revival_once()?"));
}

#[test]
fn background_shell_watcher_is_shipping_only_with_durable_remote_owner_and_revival() {
    let runtime = source("host/src/android_json_runtime.rs");
    let owner = source("host/src/remote_background_work.rs");
    let backend = source("box-exec-daemon/src/background_shell_backend.rs");

    for needle in [
        "RunnerBackgroundShellWatches::new",
        "remote-background-work.json",
        "rearm_remote_background_shell_watches",
        "reconcile_authorized",
        "drain_remote_shell_completion_revival_once",
        "build_shell_revival_prompt",
        "background-shell-revival",
    ] {
        assert!(
            runtime.contains(needle),
            "background-shell shipping runtime must contain {needle}"
        );
    }
    for needle in [
        "RemoteBackgroundWorkState",
        "Prepared",
        "Watching",
        "interrupted_by_recreate",
        "permission_grant_id",
        "remove_settled",
    ] {
        assert!(
            owner.contains(needle),
            "durable Remote background-work owner must contain {needle}"
        );
    }
    for needle in [
        "BACKGROUND_SHELL_CAPABILITY",
        "background_shell_id",
        "RemoteBackendStartOutcome::Accepted",
        "RemoteBackendReconcileOutcome::Pending",
        "RemoteBackendCancelOutcome::OutcomeUnknown",
    ] {
        assert!(
            backend.contains(needle),
            "Authorized External Runner background-shell backend must contain {needle}"
        );
    }
}

#[test]
fn cloud_agent_watcher_is_not_claimed_as_shipping_until_host_await_owner_is_wired() {
    let runtime = source("host/src/android_json_runtime.rs");
    assert!(
        !runtime.contains("RunnerCloudAgentWatches::new"),
        "CloudAgent watcher must remain non-shipping until Host owns production await_completion"
    );
}
