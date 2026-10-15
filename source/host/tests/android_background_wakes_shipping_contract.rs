use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("host crate must live below source/")
        .to_path_buf()
}

fn source(path: &str) -> String {
    fs::read_to_string(repo_root().join(path)).expect("shipping source must be readable")
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
fn background_work_watchers_are_not_claimed_as_shipping_until_external_await_owners_exist() {
    let runtime = source("host/src/android_json_runtime.rs");
    assert!(
        !runtime.contains("RunnerCloudAgentWatches::new"),
        "CloudAgent watcher must not be presented as shipping before a production external executor owns await_completion"
    );
    assert!(
        !runtime.contains("RunnerBackgroundShellWatches::new"),
        "background-shell watcher must not be presented as shipping before a production Remote terminal owner exists"
    );
}
