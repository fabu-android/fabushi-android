use fabushi_android_box_exec_daemon::deployment_contract::{
    AndroidRemoteExecutionPlacement, ANDROID_APP_LISTENER_APPLICABLE,
    ANDROID_REMOTE_EXECUTION_PLACEMENT, SECURITY_BOUNDARY, USER_CAPABILITY_REPLACEMENT,
};

#[test]
fn desktop_sibling_loopback_daemon_is_not_an_android_app_component() {
    assert!(
        !ANDROID_APP_LISTENER_APPLICABLE,
        "Android must never start the Desktop sibling loopback execution daemon inside the APK",
    );
    assert_eq!(
        ANDROID_REMOTE_EXECUTION_PLACEMENT,
        AndroidRemoteExecutionPlacement::AuthorizedExternalRunner,
        "Desktop co-resident daemon topology must be replaced by the authorized external Runner",
    );
}

#[test]
fn authorized_external_runner_preserves_user_capability_and_security_fences() {
    let replacement = USER_CAPABILITY_REPLACEMENT.to_ascii_lowercase();
    for expected in ["shell", "read", "computer", "external remote runner"] {
        assert!(
            replacement.contains(expected),
            "replacement contract must preserve {expected} capability instead of deleting it",
        );
    }

    let security = SECURITY_BOUNDARY.to_ascii_lowercase();
    for expected in ["account", "device", "executor", "one-time", "independently"] {
        assert!(
            security.contains(expected),
            "replacement security boundary must retain {expected} fencing",
        );
    }
}
