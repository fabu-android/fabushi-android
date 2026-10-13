use super::{
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
