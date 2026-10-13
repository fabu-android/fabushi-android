use fabushi_mahayana_host::android_json_runtime::{AndroidHostMode, AndroidJsonHost};
use serde_json::{json, Value};

fn sign_in_test_account(host: &mut AndroidJsonHost) {
    let started = host
        .dispatch("feature.auth.oauthStart", &json!({"provider":"google"}))
        .expect("test OAuth start must succeed");
    let attempt_id = started["attemptId"]
        .as_str()
        .expect("test OAuth start must return attemptId")
        .to_string();
    let completed = host
        .dispatch("feature.auth.oauthPoll", &json!({"attemptId":attempt_id}))
        .expect("test OAuth poll must succeed");
    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["auth"]["loggedIn"], true);
}

fn dispatch(host: &mut AndroidJsonHost, method: &str, params: Value) -> Value {
    host.dispatch(method, &params).unwrap()
}

#[test]
fn onepassword_delivery_requires_reconciliation_after_process_reopen() {
    let temp = tempfile::tempdir().unwrap();
    {
        let mut host = AndroidJsonHost::new(temp.path(), AndroidHostMode::Test);
    sign_in_test_account(&mut host);
        let initial = dispatch(
            &mut host,
            "feature.onePassword.provisioning.status",
            json!({}),
        );
        assert_eq!(initial["phase"], "idle");
        assert_eq!(initial["remoteExecutorReady"], false);
        assert_eq!(
            initial["unavailableReason"],
            "authorized_remote_runner_onepassword_executor_not_wired"
        );

        let prepared = dispatch(
            &mut host,
            "feature.onePassword.provisioning.begin",
            json!({
                "accountFence":"session:test:android",
                "accountEpoch":7,
                "requestId":"request-1",
                "operationId":"operation-1"
            }),
        );
        assert_eq!(prepared["phase"], "prepared");

        let delivering = dispatch(
            &mut host,
            "feature.onePassword.provisioning.markDelivering",
            json!({
                "accountFence":"session:test:android",
                "accountEpoch":7,
                "operationId":"operation-1"
            }),
        );
        assert_eq!(delivering["phase"], "delivering");
    }

    let mut reopened = AndroidJsonHost::new(temp.path(), AndroidHostMode::Test);
    sign_in_test_account(&mut reopened);
    let recovered = dispatch(
        &mut reopened,
        "feature.onePassword.provisioning.status",
        json!({}),
    );
    assert_eq!(recovered["phase"], "outcome-unknown");
    assert_eq!(recovered["requiresReconciliation"], true);
    assert_eq!(recovered["operationId"], "operation-1");

    let duplicate = reopened.dispatch(
        "feature.onePassword.provisioning.begin",
        &json!({
            "accountFence":"session:test:android",
            "accountEpoch":7,
            "requestId":"request-2",
            "operationId":"operation-2"
        }),
    );
    assert!(duplicate.unwrap_err().contains("unsettled"));

    let reconciled = dispatch(
        &mut reopened,
        "feature.onePassword.provisioning.reconcileNotDelivered",
        json!({
            "accountFence":"session:test:android",
            "accountEpoch":7,
            "operationId":"operation-1"
        }),
    );
    assert_eq!(reconciled["phase"], "idle");
    assert_eq!(reconciled["requiresReconciliation"], false);
}

#[test]
fn onepassword_provisioning_rejects_stale_account_fence() {
    let temp = tempfile::tempdir().unwrap();
    let mut host = AndroidJsonHost::new(temp.path(), AndroidHostMode::Test);
    sign_in_test_account(&mut host);
    let result = host.dispatch(
        "feature.onePassword.provisioning.begin",
        &json!({
            "accountFence":"session:stale",
            "accountEpoch":7,
            "requestId":"request-1",
            "operationId":"operation-1"
        }),
    );
    assert!(result.unwrap_err().contains("stale account"));
}
