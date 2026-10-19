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
fn background_completion_formatter_uses_canonical_system_notification_wrapper() {
    let formatter = source("host/src/extensions/transcript/completion_revivals.rs");
    let constants = source("packages/constants/src/system_notification.rs");

    for needle in [
        "use fabushi_constants::system_notification::wrap_system_notification;",
        "neutralize_background_task_detail_close_tags",
        "wrap_system_notification(&neutralize_background_task_detail_close_tags(&body))",
        "wrap_revival_user_query(acknowledgement)",
        "pub fn build_subagent_revival_prompt",
        "pub fn build_shell_revival_prompt",
    ] {
        assert!(
            formatter.contains(needle),
            "background completion formatter must contain {needle}"
        );
    }
    assert!(constants.contains("pub const SYSTEM_NOTIFICATION_TAG: &str = \"system_notification\";"));
    assert!(constants.contains("pub fn wrap_system_notification"));
    assert!(!formatter.contains("ComposeBackgroundCompletion"));
    assert!(!formatter.contains("JniBackgroundCompletion"));
}

#[test]
fn shipping_runtime_routes_completion_prompts_through_jni_host_hidden_turn_queue() {
    let runtime = source("host/src/android_json_runtime.rs");
    let jni = source("android-host-jni/src/lib.rs");

    for needle in [
        "fn queue_shell_completion_revival",
        "\"text\":build_shell_revival_prompt(completions)",
        "\"hidden\":true",
        "\"appendUserMessage\":false",
        "\"requestSource\":\"background-shell-revival\"",
        "\"skipAckObligation\":true",
        "drain_remote_shell_completion_revival_once",
        "fn queue_cloud_agent_completion_revival",
        "\"text\":build_subagent_revival_prompt(completions)",
        "\"requestSource\":\"cloud-agent-revival\"",
        "drain_remote_cloud_agent_completion_revival_once",
        "turn_journal",
    ] {
        assert!(
            runtime.contains(needle),
            "shipping completion revival must contain {needle}"
        );
    }
    assert!(jni.contains("fabushi_mahayana_host::android_json_runtime"));
    assert!(jni.contains("AndroidHostMode, AndroidJsonHost, RuntimeCallCancellationRegistry"));
    assert!(jni.contains("struct CoordinatorHost"));
    assert!(jni.contains("runtime: AndroidJsonHost"));
}
