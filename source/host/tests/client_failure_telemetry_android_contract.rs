use fabushi_mahayana_host::android_json_runtime::{AndroidHostMode, AndroidJsonHost};
use serde_json::{json, Value};
use std::fs;

fn spill_records(root: &std::path::Path) -> Vec<Value> {
    let path = root.join("android-structured-log-spill.v1.json");
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => panic!("failed to read structured-log spill {}: {error}", path.display()),
    };
    let value: Value = serde_json::from_str(&raw).unwrap();
    value["checkpoint"]["records"].as_array().unwrap().clone()
}

#[test]
fn client_failure_reports_use_host_validator_and_durable_structured_log_owner() {
    let temp = tempfile::tempdir().unwrap();
    let mut host = AndroidJsonHost::new(temp.path(), AndroidHostMode::Test);

    let recorded = host
        .dispatch(
            "feature.telemetry.clientFailure",
            &json!({
                "kind":"client_persistence",
                "slice":"send-journal",
                "op":"load",
                "outcome":"io_error",
                "error":{"code":"SAND-E0701","errno":"ENOENT"},
                "bytes":42
            }),
        )
        .unwrap();
    assert_eq!(recorded["recorded"], true);

    let records = spill_records(temp.path());
    let matching = records
        .iter()
        .filter(|record| record["message"] == "sand.client_persistence")
        .collect::<Vec<_>>();
    assert_eq!(
        matching.len(),
        1,
        "the validated client persistence event must be durably recorded exactly once",
    );
    let record = matching[0];
    assert_eq!(record["message"], "sand.client_persistence");
    assert_eq!(record["level"], "warn");
    assert_eq!(record["metadata"]["slice"], "send-journal");
    assert_eq!(record["metadata"]["error_code"], "SAND-E0701");
    assert_eq!(record["metadata"]["errno"], "ENOENT");
}

#[test]
fn malformed_client_failure_reports_fail_closed_without_spooling() {
    let temp = tempfile::tempdir().unwrap();
    let mut host = AndroidJsonHost::new(temp.path(), AndroidHostMode::Test);

    let rejected = host.dispatch(
        "feature.telemetry.clientFailure",
        &json!({
            "kind":"send_queued_flush",
            "queuedMs":1,
            "queueDepth":1,
            "outcome":"flush_failed",
            "cause":"SAND-E9999"
        }),
    );
    assert!(rejected.is_err());
    assert!(
        spill_records(temp.path())
            .iter()
            .all(|record| record["message"] != "sand.send.queued_flush"),
        "rejected client telemetry must not create its business event",
    );
}
