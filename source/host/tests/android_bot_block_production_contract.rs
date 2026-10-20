use std::collections::BTreeMap;

use fabushi_mahayana_host::android_structured_log_spill::AndroidStructuredLogSpool;
use fabushi_mahayana_host::runner::bot_block_detection::{
    classify_bot_block_page, BotBlockConfidence,
};

#[test]
fn bot_block_classifier_and_durable_log_contract_match_desktop_semantics() {
    let hit = classify_bot_block_page(
        "https://www.google.com/sorry/index?secret=redacted",
        "Automated queries",
    )
    .expect("google sorry");
    assert_eq!(hit.family, "google_sorry");
    assert_eq!(hit.confidence, BotBlockConfidence::High);
    assert_eq!(hit.blocked_host, "google.com");
    assert_eq!(hit.blocked_url, "https://www.google.com/sorry/index");

    let root = std::env::temp_dir().join(format!(
        "fabushi-android-bot-block-contract-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("root");
    let path = root.join("structured.json");
    let mut spool = AndroidStructuredLogSpool::open(&path, "acct-a", "counter-a", 1)
        .expect("spool");
    assert!(spool.claim_recovery_id("browser-audit:abc", 2).expect("claim"));
    assert!(!spool.claim_recovery_id("browser-audit:abc", 3).expect("dedupe"));
    spool
        .record(
            "warn",
            "sand.bot_block",
            BTreeMap::from([
                ("conversation_id".into(), "agent-a".into()),
                ("family".into(), "google_sorry".into()),
                ("confidence".into(), "high".into()),
            ]),
            4,
        )
        .expect("summary");
    spool
        .record(
            "warn",
            "sand.bot_block_detail",
            BTreeMap::from([
                ("conversation_id".into(), "agent-a".into()),
                ("family".into(), "google_sorry".into()),
                ("blocked_host".into(), "google.com".into()),
                ("blocked_url".into(), "https://www.google.com/sorry/index".into()),
            ]),
            5,
        )
        .expect("detail");
    assert_eq!(spool.batch().len(), 2);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn shipping_host_composes_remote_browser_audit_then_bot_block_into_unique_spool_owner() {
    let runtime = include_str!("../src/android_json_runtime.rs");
    let routed = include_str!("../src/runner/remote_routed_tools.rs");
    let spill = include_str!("../src/android_structured_log_spill.rs");

    assert!(runtime.contains("browser_action_audit.clone().map"));
    assert!(runtime.contains("auditor.record(AuditRecord"));
    assert!(runtime.contains("AuditAction::BrowserNavigation"));
    assert!(runtime.contains("classify_bot_block_page("));
    assert!(runtime.contains("bot_block_telemetry(&report)"));
    assert!(runtime.contains("bot_block_detail_telemetry(&report)"));
    assert!(runtime.contains("spool.claim_recovery_id(&recovery_id"));
    assert!(routed.contains("browser_navigation_observer"));
    assert!(routed.contains(".get(\"url\")"));
    assert!(routed.contains(".or_else(|| parsed.get(\"currentUrl\"))"));
    assert!(spill.contains("\"sand.bot_block\""));
    assert!(spill.contains("\"sand.bot_block_detail\""));
    assert!(spill.contains("\"sand.box_copy_in\""));
}
