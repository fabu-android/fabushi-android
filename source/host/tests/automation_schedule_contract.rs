use chrono::DateTime;
use mahayana_host_runtime::automations::automation_schedule::{
    compile_cron_matcher, compute_next_run_at, parse_every_interval_ms,
};

#[test]
fn frozen_next_run_contract_supports_every_alias_timezone_and_explicit_cron_timezone() {
    assert_eq!(parse_every_interval_ms("@every 2h"), Some(7_200_000));
    assert!(compile_cron_matcher("@daily").is_some());

    let anchor = DateTime::parse_from_rfc3339("2026-09-24T06:00:00Z")
        .expect("anchor")
        .timestamp_millis() as f64;
    let los_angeles = compute_next_run_at(
        "0 9 * * *",
        anchor,
        Some("America/Los_Angeles"),
    )
    .expect("next LA run");
    let expected_la = DateTime::parse_from_rfc3339("2026-09-24T16:00:00Z")
        .expect("expected")
        .timestamp_millis() as f64;
    assert_eq!(los_angeles, expected_la);

    let explicit_utc = compute_next_run_at(
        "CRON_TZ=UTC 0 9 * * *",
        anchor,
        Some("America/Los_Angeles"),
    )
    .expect("explicit UTC run");
    let expected_utc = DateTime::parse_from_rfc3339("2026-09-24T09:00:00Z")
        .expect("expected")
        .timestamp_millis() as f64;
    assert_eq!(explicit_utc, expected_utc);

    assert_eq!(
        compute_next_run_at("@every 2h", anchor, Some("America/Los_Angeles")),
        Some(anchor + 7_200_000.0)
    );
}
