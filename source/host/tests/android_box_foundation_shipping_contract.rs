#[test]
fn remote_box_readiness_projection_preserves_desktop_taxonomy() {
    let readiness = include_str!("../src/box_readiness.rs");
    for required in [
        "SAND_BOX_NOT_READY_MESSAGE",
        "SAND_BOX_NOT_RESPONDING_MESSAGE",
        "SAND_BOX_NO_MONITOR_AVAILABLE_MESSAGE",
        "SAND_BOX_FIRST_FORK_WINDOW_INDEX",
        "remote_box_not_ready_message",
        "is_no_monitor_capability_error",
    ] {
        assert!(
            readiness.contains(required),
            "Android Box readiness contract omitted {required}",
        );
    }

    let routed = include_str!("../src/runner/remote_routed_tools.rs");
    assert!(
        routed.contains("remote_box_not_ready_message(&error)"),
        "shipping Remote execution errors must project the Box readiness taxonomy",
    );
}

#[test]
fn authorized_external_runner_file_transfer_enforces_protected_roots() {
    let backend = include_str!("../../box-exec-daemon/src/file_transfer_backend.rs");
    let guard = include_str!("../../box-exec-daemon/src/protected_path_guard.rs");
    for required in [
        "protected_roots",
        "assert_path_outside_protected_roots",
        "file-transfer protected root must be absolute",
    ] {
        assert!(
            backend.contains(required),
            "shipping file transfer omitted protected-path wiring: {required}",
        );
    }
    for required in [
        "lexical_normalize",
        "realpath_nearest_existing",
        "Path is inside a protected host-only store and was refused",
    ] {
        assert!(
            guard.contains(required),
            "protected-path guard omitted {required}",
        );
    }
}
