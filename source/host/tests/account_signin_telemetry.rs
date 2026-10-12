use fabushi_mahayana_host::android_json_runtime::{AndroidHostMode, AndroidJsonHost};
use serde_json::{json, Value};

fn dispatch(host: &mut AndroidJsonHost, method: &str, params: Value) -> Value {
    host.dispatch(method, &params).unwrap()
}

#[test]
fn signin_funnel_is_wired_through_host_dispatch_without_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let mut host = AndroidJsonHost::new(temp.path(), AndroidHostMode::Test);

    let started = dispatch(&mut host, "feature.auth.browserStart", json!({}));
    let attempt_id = started["attemptId"].as_str().unwrap().to_string();

    let after_start = dispatch(
        &mut host,
        "feature.account.telemetry.snapshot",
        json!({}),
    );
    assert_eq!(after_start.as_array().unwrap().len(), 1);
    assert_eq!(after_start[0]["stream"], "signin");
    assert_eq!(after_start[0]["metadata"]["phase"], "login_started");
    assert!(after_start.to_string().find("accessToken").is_none());

    let completed = dispatch(
        &mut host,
        "feature.auth.browserPoll",
        json!({"attemptId": attempt_id}),
    );
    assert_eq!(completed["status"], "completed");

    let after_complete = dispatch(
        &mut host,
        "feature.account.telemetry.snapshot",
        json!({}),
    );
    assert_eq!(after_complete.as_array().unwrap().len(), 2);
    assert_eq!(after_complete[1]["metadata"]["phase"], "login_completed");

    let logged_out = dispatch(&mut host, "feature.auth.logout", json!({}));
    assert_eq!(logged_out["loggedIn"], false);
    let after_logout = dispatch(
        &mut host,
        "feature.account.telemetry.snapshot",
        json!({}),
    );
    assert_eq!(after_logout[2]["metadata"]["phase"], "signed_out");
    assert_eq!(after_logout[2]["metadata"]["cause"], "user_logout");
}

#[test]
fn cancelled_signin_is_projected_as_forced_signout() {
    let temp = tempfile::tempdir().unwrap();
    let mut host = AndroidJsonHost::new(temp.path(), AndroidHostMode::Test);

    let started = dispatch(&mut host, "feature.auth.browserStart", json!({}));
    let attempt_id = started["attemptId"].as_str().unwrap().to_string();
    let cancelled = dispatch(
        &mut host,
        "feature.auth.browserCancel",
        json!({"attemptId": attempt_id}),
    );
    assert_eq!(cancelled["status"], "cancelled");

    let telemetry = dispatch(
        &mut host,
        "feature.account.telemetry.snapshot",
        json!({}),
    );
    assert_eq!(telemetry[1]["metadata"]["phase"], "signed_out");
    assert_eq!(telemetry[1]["metadata"]["cause"], "forced");
}
