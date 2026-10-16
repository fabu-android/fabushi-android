use super::{
    agent_error_telemetry::{
        agent_error_telemetry, AgentErrorReport, AGENT_ERROR_EVENT,
    },
    agent_open_telemetry::{agent_open_telemetry, AgentOpenReport, AGENT_OPEN_EVENT},
    auto_review_approval_telemetry::{
        auto_review_approval_telemetry, AutoReviewApprovalReport,
    },
    automation_fire_telemetry::{
        automation_fire_dropped_telemetry, automation_run_telemetry,
        AutomationFireDroppedReport, AutomationRunReport, AUTOMATION_FIRE_DROPPED_EVENT,
        AUTOMATION_RUN_EVENT,
    },
};

#[test]
fn agent_error_projection_preserves_desktop_error_tags() {
    let projection = agent_error_telemetry(&AgentErrorReport {
        source: "turn".into(),
        conversation_id: "agent-a".into(),
        request_id: Some("request-a".into()),
        error: super::sand_error_tags::SandErrorValue::new("SAND-E0101")
            .with_string("errno", "ECONNRESET"),
        detail: None,
    });
    assert_eq!(projection.level, Some("error"));
    assert_eq!(projection.event, Some(AGENT_ERROR_EVENT));
    assert_eq!(projection.metadata.get("source").map(String::as_str), Some("turn"));
    assert_eq!(
        projection.metadata.get("conversation_id").map(String::as_str),
        Some("agent-a")
    );
    assert_eq!(
        projection.metadata.get("request_id").map(String::as_str),
        Some("request-a")
    );
    assert_eq!(
        projection.metadata.get("error_code").map(String::as_str),
        Some("SAND-E0101")
    );
    assert_eq!(
        projection.metadata.get("error_retryable").map(String::as_str),
        Some("true")
    );
}

#[test]
fn agent_open_projection_preserves_desktop_contract() {
    let projection = agent_open_telemetry(&AgentOpenReport {
        conversation_id: "agent-a".into(),
        duration_ms: 42,
        entry_count: 7,
        was_active: true,
    });
    assert_eq!(projection.level, Some("info"));
    assert_eq!(projection.event, Some(AGENT_OPEN_EVENT));
    assert_eq!(projection.metadata.get("conversation_id").map(String::as_str), Some("agent-a"));
    assert_eq!(projection.metadata.get("entry_count").map(String::as_str), Some("7"));
}

#[test]
fn auto_review_projection_clamps_negative_age_and_keeps_optional_context() {
    let projection = auto_review_approval_telemetry(&AutoReviewApprovalReport {
        event_type: "expired".into(),
        conversation_id: "agent-a".into(),
        approval_id: "approval-a".into(),
        surface: "android".into(),
        status: "pending".into(),
        age_ms: -10.0,
        ttl_ms: Some(30_000.4),
        cause: Some("timeout".into()),
    });
    assert_eq!(projection.event, Some("sand.auto_review.approval"));
    assert_eq!(projection.metadata.get("age_ms").map(String::as_str), Some("0"));
    assert_eq!(projection.metadata.get("ttl_ms").map(String::as_str), Some("30000"));
    assert_eq!(projection.metadata.get("cause").map(String::as_str), Some("timeout"));
}

#[test]
fn late_automation_run_is_warn_and_drop_retains_reconciliation_context() {
    let run = automation_run_telemetry(&AutomationRunReport {
        conversation_id: "agent-a".into(),
        automation_id: "automation-a".into(),
        trigger: "schedule".into(),
        outcome: "ok".into(),
        is_group: false,
        duration_ms: 12.0,
        lateness_ms: Some(300_000.0),
        scheduled_for_ms: Some(1_000.0),
        sent_message_count: Some(1),
        event_batch_size: None,
    });
    assert_eq!(run.level, Some("warn"));
    assert_eq!(run.event, Some(AUTOMATION_RUN_EVENT));
    assert_eq!(run.metadata.get("late").map(String::as_str), Some("true"));

    let dropped = automation_fire_dropped_telemetry(&AutomationFireDroppedReport {
        conversation_id: "agent-a".into(),
        trigger: "schedule".into(),
        reason: "outcome-unknown".into(),
        scheduled_for_ms: Some(1_000.0),
        lateness_ms: Some(20.0),
        error_type: Some("transport".into()),
        error_code: Some("SAND-E0001".into()),
        run_uuid: Some("run-a".into()),
        fire_age_ms: Some(50.0),
        has_definition_revision: Some(true),
        box_uptime_ms: Some(99.0),
    });
    assert_eq!(dropped.level, Some("warn"));
    assert_eq!(dropped.event, Some(AUTOMATION_FIRE_DROPPED_EVENT));
    assert_eq!(dropped.metadata.get("reason").map(String::as_str), Some("outcome-unknown"));
    assert_eq!(dropped.metadata.get("run_uuid").map(String::as_str), Some("run-a"));
}


#[test]
fn disk_pressure_projection_maps_platform_severity() {
    let projection = super::disk_pressure_telemetry::disk_pressure_telemetry(
        &super::disk_pressure_telemetry::DiskPressureReport {
            level: "hard".into(),
            volume: "app-private".into(),
            trigger: "write".into(),
            total_bytes: 1000.0,
            available_bytes: 50.0,
            used_percent: 95.0,
        },
    );
    assert_eq!(projection.level, Some("error"));
    assert_eq!(projection.metadata.get("pressure_level").map(String::as_str), Some("hard"));
    assert_eq!(projection.metadata.get("used_percent").map(String::as_str), Some("95.0"));
}

#[test]
fn host_diagnostic_projection_keeps_degraded_send_as_error() {
    let projection = super::host_diagnostic_telemetry::host_diagnostic_telemetry(
        &super::host_diagnostic_telemetry::HostDiagnostic {
            kind: "send_ledger_degraded".into(),
            stage: Some("restore".into()),
            agent_id: Some("agent-a".into()),
            reason: Some("corrupt".into()),
            error_class: Some("InvalidData".into()),
        },
    );
    assert_eq!(projection.level, Some("error"));
    assert_eq!(projection.event, Some(super::host_diagnostic_telemetry::HOST_DIAGNOSTIC_EVENT));
    assert_eq!(projection.metadata.get("stage").map(String::as_str), Some("restore"));
}

#[test]
fn search_index_health_projection_warns_only_for_bounded_retry_kinds() {
    let retry = super::search_index_health_telemetry::search_index_health_telemetry(
        &super::search_index_health_telemetry::SearchIndexHealthReport {
            kind: "job_retry".into(),
            stage: Some("reconcile".into()),
            error_class: None,
            count: Some(2),
        },
    );
    assert_eq!(retry.level, Some("warn"));
    assert_eq!(retry.metadata.get("count").map(String::as_str), Some("2"));

    let corrupt = super::search_index_health_telemetry::search_index_health_telemetry(
        &super::search_index_health_telemetry::SearchIndexHealthReport {
            kind: "corrupt".into(),
            ..Default::default()
        },
    );
    assert_eq!(corrupt.level, Some("error"));
}


#[test]
fn conversation_gc_unresolved_refs_remain_warning() {
    let projection = super::conversation_gc_telemetry::conversation_gc_telemetry(
        &super::conversation_gc_telemetry::ConversationGcReport::Skipped {
            trigger: "soft-cap".into(),
            agent_id: "agent-a".into(),
            skip_reason: "unresolved-refs".into(),
            unresolved_proto_refs: Some(2.4),
        },
    );
    assert_eq!(projection.level, Some("warn"));
    assert_eq!(projection.metadata.get("outcome").map(String::as_str), Some("skipped"));
    assert_eq!(
        projection.metadata.get("unresolved_proto_refs").map(String::as_str),
        Some("2")
    );
}

#[test]
fn bot_block_summary_does_not_expose_blocked_url_but_detail_does() {
    let report = super::bot_block_telemetry::BotBlockReport {
        conversation_id: "agent-a".into(),
        family: "navigation".into(),
        confidence: "high".into(),
        blocked_host: "example.invalid".into(),
        blocked_url: "https://example.invalid/private".into(),
    };
    let summary = super::bot_block_telemetry::bot_block_telemetry(&report);
    assert!(!summary.metadata.contains_key("blocked_url"));
    let detail = super::bot_block_telemetry::bot_block_detail_telemetry(&report);
    assert_eq!(
        detail.metadata.get("blocked_url").map(String::as_str),
        Some("https://example.invalid/private")
    );
}

#[test]
fn mcp_discovery_failure_preserves_stale_fallback_signal() {
    let projection = super::mcp_discovery_telemetry::mcp_discovery_failed_telemetry(
        &super::mcp_discovery_telemetry::McpDiscoveryFailedReport {
            error_class: "timeout".into(),
            elapsed_ms: 1499.6,
            served_stale: true,
        },
    );
    assert_eq!(projection.level, Some("warn"));
    assert_eq!(projection.metadata.get("elapsed_ms").map(String::as_str), Some("1500"));
    assert_eq!(projection.metadata.get("served_stale").map(String::as_str), Some("true"));
}

#[test]
fn automation_shadow_prune_failure_is_warn() {
    let projection =
        super::automation_shadow_prune_telemetry::automation_shadow_prune_telemetry(
            &super::automation_shadow_prune_telemetry::AutomationShadowPruneReport {
                outcome: "failed".into(),
                conversation_id: "agent-a".into(),
                automation_id: "automation-a".into(),
                local_definition_state: "present".into(),
                local_definition_count: 1,
                desired_count: 1,
                remote_shadow_count: 2,
                box_uptime_ms: Some(900),
            },
        );
    assert_eq!(projection.level, Some("warn"));
    assert_eq!(projection.metadata.get("remote_shadow_count").map(String::as_str), Some("2"));
}


#[test]
fn sand_error_tags_preserve_registry_fallback_and_retryability() {
    use super::sand_error_tags::{sand_error_tags, SandErrorValue};

    let known = sand_error_tags(&SandErrorValue::new("SAND-E0101").with_string("errno", "ECONNRESET"));
    assert_eq!(known.get("error_code").map(String::as_str), Some("SAND-E0101"));
    assert_eq!(known.get("error_domain").map(String::as_str), Some("transport"));
    assert_eq!(known.get("error_retryable").map(String::as_str), Some("true"));
    assert_eq!(known.get("errno").map(String::as_str), Some("ECONNRESET"));

    let unknown = sand_error_tags(&SandErrorValue::new("SAND-E9999"));
    assert_eq!(unknown.get("error_code").map(String::as_str), Some("SAND-E0001"));
    assert_eq!(unknown.get("error_domain").map(String::as_str), Some("registry"));
    assert_eq!(unknown.get("error_retryable").map(String::as_str), Some("false"));
}

#[test]
fn connector_auth_bounds_identity_metadata_and_unknown_connector_names() {
    use super::connector_auth_telemetry::{
        bounded_connector_tag, connector_auth_telemetry, ConnectorAuthReport,
    };

    assert_eq!(bounded_connector_tag(Some("GitHub")), "github");
    assert_eq!(bounded_connector_tag(Some("private-corporate-plugin")), "other");

    let projection = connector_auth_telemetry(
        &ConnectorAuthReport {
            phase: "callback".into(),
            outcome: "failed".into(),
            server_name: Some("Private Corporate Plugin".into()),
            server_id: Some("bad server id with spaces".into()),
            reauth: Some(true),
            error: None,
        },
        "android",
    );
    assert_eq!(projection.level, Some("warn"));
    assert_eq!(projection.metadata.get("connector").map(String::as_str), Some("other"));
    assert!(!projection.metadata.contains_key("server_id"));
    assert_eq!(projection.metadata.get("reauth").map(String::as_str), Some("true"));
}

#[test]
fn journal_outcome_warns_for_corrupt_tail_and_errors_for_failed_outcome() {
    use super::journal_outcome_telemetry::{
        journal_outcome_telemetry, JournalOutcomeReport,
    };
    use super::sand_error_tags::SandErrorValue;

    let corrupt = journal_outcome_telemetry(&JournalOutcomeReport {
        outcome: "ok".into(),
        op: "restore".into(),
        conversation_id: "agent-a".into(),
        entry_count: Some(3),
        bytes: Some(99),
        duration_ms: 10.6,
        cause: Some(SandErrorValue::new("SAND-E0701").with_string("tail", "corrupt")),
    });
    assert_eq!(corrupt.level, Some("warn"));
    assert_eq!(corrupt.metadata.get("duration_ms").map(String::as_str), Some("11"));

    let failed = journal_outcome_telemetry(&JournalOutcomeReport {
        outcome: "failed".into(),
        op: "append".into(),
        conversation_id: "agent-a".into(),
        entry_count: None,
        bytes: None,
        duration_ms: 1.0,
        cause: None,
    });
    assert_eq!(failed.level, Some("error"));
}
