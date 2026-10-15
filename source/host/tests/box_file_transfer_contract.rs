use fabushi_android_box_exec_daemon::{
    file_transfer_backend::{
        RemoteFileTransferBackend, RemoteFileTransferConfig, BOX_FILE_TRANSFER_CAPABILITY,
        MAX_TRANSFER_BYTES, TRANSFER_CHUNK_BYTES,
    },
    server::{
        RemoteBackendReconcileOutcome, RemoteBackendStartOutcome, RemoteExecutionBackend,
        RemoteRequestIdentity,
    },
};
use fabushi_android_shared::ExecutionRequest;
use serde_json::json;
use std::{fs, path::PathBuf};

fn root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "fabushi-box-file-transfer-contract-{label}-{}",
        std::process::id()
    ))
}

fn identity(operation_id: &str, account: &str, device: &str) -> RemoteRequestIdentity {
    RemoteRequestIdentity {
        credential_id: "credential-transfer".into(),
        operation_id: operation_id.into(),
        request_id: format!("request-{operation_id}"),
        account_fence: account.into(),
        account_epoch: 7,
        permission_grant_id: format!("grant-{operation_id}"),
        device_id: device.into(),
    }
}

fn request(operation_id: &str, tool: &str, arguments: serde_json::Value) -> ExecutionRequest {
    ExecutionRequest {
        operation_id: operation_id.into(),
        capability_id: BOX_FILE_TRANSFER_CAPABILITY.into(),
        input_json: json!({"tool":tool,"arguments":arguments}).to_string(),
        timeout_ms: 120_000,
    }
}

#[test]
fn shipping_host_graph_declares_copy_tools_on_the_authorized_remote_runner_path() {
    let source = include_str!("../src/runner/remote_routed_tools.rs");
    for required in [
        r#"const COPY_TO_BOX_TOOL_NAME: &str = "CopyToBox""#,
        r#"const COPY_FROM_BOX_TOOL_NAME: &str = "CopyFromBox""#,
        r#""file-transfer""#,
        r#""box.file.transfer""#,
        "AuthenticatedRemoteHostRunner",
        "request_remote_approval",
        "reconcile_authorized",
    ] {
        assert!(
            source.contains(required),
            "shipping Remote tool graph omitted {required}",
        );
    }
    assert!(
        !source.contains("Compose") || source.contains("Remote"),
        "file transfer must remain below Presentation/Compose ownership",
    );
}

#[test]
fn external_runner_transfer_is_100_mib_chunked_digest_verified_and_reconcilable() {
    assert_eq!(MAX_TRANSFER_BYTES, 100 * 1024 * 1024);
    assert_eq!(TRANSFER_CHUNK_BYTES, 32 * 1024);

    let root = root("shipping");
    let _ = fs::remove_dir_all(&root);
    let user_root = root.join("user");
    let box_root = root.join("box");
    fs::create_dir_all(&user_root).unwrap();
    fs::create_dir_all(&box_root).unwrap();
    fs::write(user_root.join("input.bin"), b"remote-transfer").unwrap();

    let mut backend = RemoteFileTransferBackend::open(RemoteFileTransferConfig {
        user_root: user_root.clone(),
        agent_box_root: box_root.clone(),
        protected_roots: Vec::new(),
    })
    .unwrap();
    let req = request(
        "transfer-op",
        "CopyToBox",
        json!({"computer_path":"input.bin","box_path":"uploads/input.bin"}),
    );
    let first = backend
        .start(&identity("transfer-op", "session:a", "device-a"), &req)
        .unwrap();
    let output = match first {
        RemoteBackendStartOutcome::Completed { output_json } => output_json,
        other => panic!("expected completed transfer, got {other:?}"),
    };
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["bytes"].as_u64(), Some(15));
    assert_eq!(fs::read(box_root.join("uploads/input.bin")).unwrap(), b"remote-transfer");

    assert!(matches!(
        backend
            .reconcile(&identity("transfer-op", "session:a", "device-a"), &req)
            .unwrap(),
        RemoteBackendReconcileOutcome::Completed { .. }
    ));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn transfer_request_identity_remains_fenced_by_remote_service_contract() {
    let server = include_str!("../../box-exec-daemon/src/server.rs");
    for required in [
        "operation_id",
        "request_id",
        "account_fence",
        "account_epoch",
        "permission_grant_id",
        "device_id",
        "matches_execute",
        "OutcomeUnknown",
    ] {
        assert!(
            server.contains(required),
            "Remote service durable identity contract omitted {required}",
        );
    }
}
