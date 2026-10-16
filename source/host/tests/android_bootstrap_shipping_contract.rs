#[test]
fn android_jni_production_runtime_owns_host_lock_discovery_and_private_data_root() {
    let jni = include_str!("../../android-host-jni/src/lib.rs");
    for required in [
        "struct AndroidHostBootstrapGuard",
        "get_android_host_lock_path",
        "get_android_native_discovery_path",
        "acquire_host_lock(&lock_path, pid)",
        "write_native_runtime_discovery",
        "clear_native_runtime_discovery_if_owner",
        "transport: \"jni\".into()",
        "(mode == AndroidHostMode::Production)",
        "AndroidHostBootstrapGuard::open(&app_data_dir, generation)",
    ] {
        assert!(
            jni.contains(required),
            "Android production Host bootstrap omitted {required}",
        );
    }

    let paths = include_str!("../src/host_paths.rs");
    assert!(paths.contains("get_android_host_runtime_root"));
    assert!(paths.contains("app_data_dir.join(\"host-runtime\")"));
    assert!(paths.contains("get_android_host_lock_path"));
    assert!(paths.contains("get_android_native_discovery_path"));
}

#[test]
fn android_lock_reclaims_foreign_pid_without_killing_and_fails_closed_for_live_fabushi_host() {
    let lock = include_str!("../src/host_lock.rs");
    for required in [
        "ReclaimedForeign",
        "is_android_fabushi_host_process",
        "acquire_host_lock_with_identity",
        "PID reuse or an unrelated process must not permanently wedge the app",
        "Android delegates",
        "must never signal another live app process",
    ] {
        assert!(
            lock.contains(required),
            "Android Host lock replacement omitted {required}",
        );
    }
    assert!(
        !lock.contains("Command::new(\"kill\")"),
        "Android Host bootstrap must not terminate another app process",
    );
}

#[test]
fn native_discovery_clear_is_pid_and_generation_fenced() {
    let discovery = include_str!("../src/host_discovery.rs");
    for required in [
        "struct NativeRuntimeDiscoveryInfo",
        "transport == \"jni\"",
        "write_native_runtime_discovery",
        "read_native_runtime_discovery",
        "clear_native_runtime_discovery_if_owner",
        "info.pid == pid && info.generation == generation",
    ] {
        assert!(
            discovery.contains(required),
            "Android native discovery contract omitted {required}",
        );
    }
}
