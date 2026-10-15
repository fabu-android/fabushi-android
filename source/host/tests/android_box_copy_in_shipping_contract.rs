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
fn external_box_runner_hydrates_frozen_manifests_with_pack_fallback_and_handoff_marker() {
    let backend = include_str!("../../box-exec-daemon/src/box_store_backend.rs");
    assert!(backend.contains("fn try_hydrate_frozen_manifest"));
    assert!(backend.contains("parse_box_store_manifest"));
    assert!(backend.contains("read_pack_blob(&self.store_dir(&input.store_id), sha, *size)"));
    assert!(backend.contains("remove_hydration_handoff_marker"));
    assert!(backend.contains("write_hydration_handoff_marker"));
    assert!(backend.contains("manifest.fully_hydrated == Some(false)"));
    assert!(backend.contains("legacy source reconciliation before restore"));

    let pack = include_str!("../../box-exec-daemon/src/box_store_pack_restore.rs");
    assert!(pack.contains("pub fn read_pack_blob"));
    assert!(pack.contains("BOX_STORE_PACK_INDEX_KEY"));
    assert!(pack.contains("GzDecoder"));
}
