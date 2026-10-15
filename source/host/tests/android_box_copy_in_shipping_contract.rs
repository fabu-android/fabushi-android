#[test]
fn android_shipping_box_copy_in_routes_through_remote_box_owner_and_unique_structured_log_sink() {
    let runtime = include_str!("../src/android_json_runtime.rs");
    assert!(runtime.contains("fn remote_box_store_sync_dispatch"));
    assert!(runtime.contains("dispatch_via_authorized_runner"));
    assert!(runtime.contains("fn remote_box_store_sync_reconcile"));
    assert!(runtime.contains("reconcile_via_authorized_runner"));
    assert!(runtime.contains("fn record_remote_box_copy_in_telemetry"));
    assert!(runtime.contains("box_copy_in_telemetry("));
    assert!(runtime.contains("self.record_structured_log(level, event, projection.metadata, now_ms())"));
    assert!(runtime.contains("\"started\""));
    assert!(runtime.contains("\"hydrate-started\""));
    assert!(runtime.contains("\"outcome_unknown\""));
    assert!(runtime.contains("\"remote-outcome-unknown\""));
    assert!(runtime.contains("\"reconcile-completed\""));

    let mapper = include_str!("../src/extensions/telemetry/box_copy_in_telemetry.rs");
    assert_eq!(mapper.matches("sand.box_copy_in").count(), 1);
    assert!(mapper.contains("event: Some(BOX_COPY_IN_EVENT)"));

    let spill = include_str!("../src/android_structured_log_spill.rs");
    assert!(spill.contains("\"sand.box_copy_in\""));
}

#[test]
fn external_box_runner_hydrates_frozen_manifests_with_legacy_reconciliation_pack_fallback_and_runtime_fences() {
    let backend = include_str!("../../box-exec-daemon/src/box_store_backend.rs");
    for required in [
        "fn try_hydrate_frozen_manifest",
        "read_frozen_manifest_snapshot",
        "legacy_store_root",
        "store_db_agent_ids",
        "primary V2 store.db coverage is below the legacy source",
        "legacy source has no store.db identities to validate V2",
        "mark_primary_legacy_hydration_incomplete",
        "read_pack_blob(&source_dir, sha, *size)",
        "remove_hydration_handoff_marker",
        "write_hydration_handoff_marker",
        "write_copy_in_status_throttled",
        "build_copy_in_watchdog_event",
        "resolve_copy_in_download_owner",
        "apply_copy_in_download_owner",
    ] {
        assert!(
            backend.contains(required),
            "shipping external Box runner omitted frozen copy-in responsibility: {required}",
        );
    }

    let hydration = include_str!("../../box-exec-daemon/src/box_store_hydration.rs");
    for required in [
        "COPY_IN_STATUS_THROTTLE_MS: u64 = 750",
        "COPY_IN_STUCK_THRESHOLD_MS: u64 = 5 * 60_000",
        "pub fn write_copy_in_status_throttled",
        "pub fn build_copy_in_watchdog_event",
        "pub fn resolve_copy_in_download_owner_values",
        "pub fn apply_copy_in_download_owner",
    ] {
        assert!(
            hydration.contains(required),
            "frozen copy-in runtime contract omitted {required}",
        );
    }

    let pack = include_str!("../../box-exec-daemon/src/box_store_pack_restore.rs");
    assert!(pack.contains("pub fn read_pack_blob"));
    assert!(pack.contains("BOX_STORE_PACK_INDEX_KEY"));
    assert!(pack.contains("GzDecoder"));
}
