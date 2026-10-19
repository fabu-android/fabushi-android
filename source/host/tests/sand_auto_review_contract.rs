use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use fabushi_mahayana_host::runner::sand_auto_review::{
    SandAutoReviewApprovalStatus, SandAutoReviewController, SandAutoReviewDecision,
    SandAutoReviewEvent, SandAutoReviewExpiryCause, SandAutoReviewExpiryPolicy,
    SandAutoReviewMode, SandAutoReviewRequest, SandAutoReviewRequestOutcome,
    SandAutoReviewResolution, SandAutoReviewSurface,
    fingerprint_sand_auto_review_target, format_sand_auto_review_denied_reason,
    resolve_sand_auto_review_modes, sand_auto_review_approval_expiry_policy,
};

fn request(
    surface: SandAutoReviewSurface,
    expiry_policy: Option<SandAutoReviewExpiryPolicy>,
) -> SandAutoReviewRequest {
    SandAutoReviewRequest {
        agent_id: None,
        surface,
        fingerprint: "fingerprint".into(),
        reason: "classifier reason".into(),
        summary: "sensitive action".into(),
        command: Some(" echo test ".into()),
        proposed_rule: Some("  allow   this  ".into()),
        expiry_policy,
    }
}

#[test]
fn modes_match_frozen_off_shadow_enforce_resolution() {
    let off = resolve_sand_auto_review_modes(false, true, None);
    assert_eq!(off.host_shell, SandAutoReviewMode::Off);
    assert_eq!(off.automation_write, SandAutoReviewMode::Off);

    let shadow = resolve_sand_auto_review_modes(true, false, None);
    assert_eq!(shadow.host_shell, SandAutoReviewMode::Shadow);
    assert_eq!(shadow.cloud_agent, SandAutoReviewMode::Shadow);
    assert_eq!(shadow.automation_write, SandAutoReviewMode::Off);

    let enforce = resolve_sand_auto_review_modes(true, true, None);
    assert_eq!(enforce.computer, SandAutoReviewMode::Enforce);
    assert_eq!(enforce.subagent_launch, SandAutoReviewMode::Enforce);
    assert_eq!(enforce.automation_write, SandAutoReviewMode::Off);

    let override_modes =
        resolve_sand_auto_review_modes(true, true, Some(SandAutoReviewMode::Shadow));
    assert_eq!(override_modes.mcp, SandAutoReviewMode::Shadow);
    assert_eq!(override_modes.automation_write, SandAutoReviewMode::Off);
}

#[test]
fn turn_and_handoff_resume_use_park_while_other_sources_use_ttl() {
    assert_eq!(
        sand_auto_review_approval_expiry_policy("turn"),
        SandAutoReviewExpiryPolicy::Park
    );
    assert_eq!(
        sand_auto_review_approval_expiry_policy("handoff-resume"),
        SandAutoReviewExpiryPolicy::Park
    );
    assert_eq!(
        sand_auto_review_approval_expiry_policy("automation"),
        SandAutoReviewExpiryPolicy::Ttl
    );
}

#[test]
fn controller_creates_sanitized_pending_approval_and_resolves_it() {
    let controller = SandAutoReviewController::with_options(
        "agent-1",
        "host-1",
        60_000,
        4,
        true,
        Arc::new(|| 1234),
        Arc::new(|| "approval-1".into()),
        None,
    );
    let events = Arc::new(Mutex::new(Vec::<SandAutoReviewEvent>::new()));
    let event_sink = Arc::clone(&events);
    controller.subscribe(Arc::new(move |event| {
        event_sink.lock().expect("events").push(event.clone());
    }));

    let pending = match controller.request_approval(request(
        SandAutoReviewSurface::Mcp,
        Some(SandAutoReviewExpiryPolicy::Park),
    )) {
        SandAutoReviewRequestOutcome::Pending(pending) => pending,
        SandAutoReviewRequestOutcome::Immediate(_) => panic!("expected pending"),
    };

    assert_eq!(pending.approval.id, "approval-1");
    assert_eq!(pending.approval.created_at_ms, 1234);
    assert_eq!(pending.approval.expires_at_ms, None);
    assert_eq!(pending.approval.command.as_deref(), Some("echo test"));
    assert_eq!(
        pending.approval.proposed_rule.as_deref(),
        Some("allow this")
    );
    assert_eq!(pending.approval.status, SandAutoReviewApprovalStatus::Pending);

    let resolved = controller
        .resolve_approval("approval-1", SandAutoReviewResolution::Approved)
        .expect("resolved");
    assert_eq!(resolved.status, SandAutoReviewApprovalStatus::Approved);
    assert_eq!(pending.wait().expect("decision"), SandAutoReviewDecision::Approved);

    let events = events.lock().expect("events");
    assert!(matches!(events[0], SandAutoReviewEvent::Created(_)));
    assert!(matches!(events[1], SandAutoReviewEvent::Resolved(_)));
}

#[test]
fn user_message_epoch_retires_pending_and_stale_resolution_is_rejected() {
    let controller = SandAutoReviewController::new("agent-1", "host-1");
    let pending = match controller.request_approval(request(
        SandAutoReviewSurface::Computer,
        Some(SandAutoReviewExpiryPolicy::Park),
    )) {
        SandAutoReviewRequestOutcome::Pending(pending) => pending,
        SandAutoReviewRequestOutcome::Immediate(_) => panic!("expected pending"),
    };
    let approval_id = pending.approval.id.clone();

    controller.begin_user_message_epoch();

    assert_eq!(controller.epoch(), 1);
    assert!(controller.get_pending_approvals().is_empty());
    assert!(controller
        .resolve_approval(&approval_id, SandAutoReviewResolution::Approved)
        .is_none());
    let decision = pending.wait().expect("decision");
    assert_eq!(
        decision,
        SandAutoReviewDecision::Denied {
            reason: format_sand_auto_review_denied_reason("classifier reason"),
        }
    );
}

#[test]
fn max_pending_and_unresolvable_conversations_fail_closed() {
    let controller = SandAutoReviewController::with_options(
        "agent-1",
        "host-1",
        60_000,
        1,
        true,
        Arc::new(|| 1),
        Arc::new(|| "approval".into()),
        None,
    );
    assert!(matches!(
        controller.request_approval(request(
            SandAutoReviewSurface::HostShell,
            Some(SandAutoReviewExpiryPolicy::Park)
        )),
        SandAutoReviewRequestOutcome::Pending(_)
    ));
    let second = controller.request_approval(request(
        SandAutoReviewSurface::BoxShell,
        Some(SandAutoReviewExpiryPolicy::Park),
    ));
    assert!(matches!(
        second,
        SandAutoReviewRequestOutcome::Immediate(SandAutoReviewDecision::Denied { .. })
    ));

    let unavailable = SandAutoReviewController::with_options(
        "agent-1",
        "host-1",
        60_000,
        4,
        false,
        Arc::new(|| 1),
        Arc::new(|| "approval".into()),
        None,
    );
    assert!(matches!(
        unavailable.request_approval(request(
            SandAutoReviewSurface::Mcp,
            Some(SandAutoReviewExpiryPolicy::Park)
        )),
        SandAutoReviewRequestOutcome::Immediate(SandAutoReviewDecision::Denied { .. })
    ));
}

#[test]
fn settings_surface_expiry_and_quiesce_preserve_distinct_reasons() {
    let controller = SandAutoReviewController::new("agent-1", "host-1");
    let pending = match controller.request_approval(request(
        SandAutoReviewSurface::Mcp,
        Some(SandAutoReviewExpiryPolicy::Park),
    )) {
        SandAutoReviewRequestOutcome::Pending(pending) => pending,
        _ => panic!("expected pending"),
    };
    controller.expire_surfaces(&std::collections::HashSet::from([
        SandAutoReviewSurface::Mcp,
    ]));
    assert_eq!(
        pending.wait().expect("settings decision"),
        SandAutoReviewDecision::Denied {
            reason: "Auto-review settings changed; retry the action.".into(),
        }
    );

    let pending = match controller.request_approval(request(
        SandAutoReviewSurface::Computer,
        Some(SandAutoReviewExpiryPolicy::Park),
    )) {
        SandAutoReviewRequestOutcome::Pending(pending) => pending,
        _ => panic!("expected pending"),
    };
    controller.expire_for_quiesce();
    let decision = pending.wait().expect("quiesce decision");
    match decision {
        SandAutoReviewDecision::Denied { reason } => {
            assert!(reason.contains("user did NOT deny it"));
            assert!(reason.contains("classifier reason"));
        }
        SandAutoReviewDecision::Approved => panic!("quiesce cannot approve"),
    }
    assert!(matches!(
        controller.request_approval(request(
            SandAutoReviewSurface::HostShell,
            Some(SandAutoReviewExpiryPolicy::Park)
        )),
        SandAutoReviewRequestOutcome::Immediate(SandAutoReviewDecision::Denied { .. })
    ));
    controller.cancel_quiesce();
}

#[test]
fn already_cancelled_turn_does_not_enqueue_an_approval() {
    let controller = SandAutoReviewController::new("agent-1", "host-1");
    controller.set_cancellation_probe(Arc::new(|| true));

    assert!(matches!(
        controller.request_approval(request(
            SandAutoReviewSurface::Mcp,
            Some(SandAutoReviewExpiryPolicy::Park),
        )),
        SandAutoReviewRequestOutcome::Immediate(SandAutoReviewDecision::Denied { reason })
            if reason == "The action was cancelled."
    ));
    assert!(controller.get_pending_approvals().is_empty());
}

#[test]
fn turn_cancellation_retires_pending_approval_with_frozen_reason() {
    let controller = SandAutoReviewController::new("agent-1", "host-1");
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancellation_probe = Arc::clone(&cancelled);
    controller.set_cancellation_probe(Arc::new(move || {
        cancellation_probe.load(Ordering::SeqCst)
    }));
    let events = Arc::new(Mutex::new(Vec::<SandAutoReviewEvent>::new()));
    let event_sink = Arc::clone(&events);
    controller.subscribe(Arc::new(move |event| {
        event_sink.lock().expect("events").push(event.clone());
    }));

    let pending = match controller.request_approval(request(
        SandAutoReviewSurface::Mcp,
        Some(SandAutoReviewExpiryPolicy::Park),
    )) {
        SandAutoReviewRequestOutcome::Pending(pending) => pending,
        SandAutoReviewRequestOutcome::Immediate(_) => panic!("expected pending"),
    };
    cancelled.store(true, Ordering::SeqCst);

    assert_eq!(
        pending.wait().expect("cancelled decision"),
        SandAutoReviewDecision::Denied {
            reason: "The action was cancelled.".into(),
        }
    );
    assert!(controller.get_pending_approvals().is_empty());
    assert!(matches!(
        events.lock().expect("events").last(),
        Some(SandAutoReviewEvent::Expired {
            cause: SandAutoReviewExpiryCause::Cancelled,
            ..
        })
    ));
}

#[test]
fn explicit_expiry_emits_expired_event_with_cause() {
    let controller = SandAutoReviewController::new("agent-1", "host-1");
    let events = Arc::new(Mutex::new(Vec::<SandAutoReviewEvent>::new()));
    let event_sink = Arc::clone(&events);
    controller.subscribe(Arc::new(move |event| {
        event_sink.lock().expect("events").push(event.clone());
    }));
    let pending = match controller.request_approval(request(
        SandAutoReviewSurface::CloudAgent,
        Some(SandAutoReviewExpiryPolicy::Park),
    )) {
        SandAutoReviewRequestOutcome::Pending(pending) => pending,
        _ => panic!("expected pending"),
    };

    controller.expire(SandAutoReviewExpiryCause::SessionEnd);
    let _ = pending.wait().expect("expired decision");

    let events = events.lock().expect("events");
    assert!(matches!(
        events.last(),
        Some(SandAutoReviewEvent::Expired {
            cause: SandAutoReviewExpiryCause::SessionEnd,
            ..
        })
    ));
}

#[test]
fn target_fingerprint_is_stable_sha256_of_json_serialization() {
    let target = serde_json::json!({"action":"write","path":"/tmp/a"});
    let first = fingerprint_sand_auto_review_target(&target);
    let second = fingerprint_sand_auto_review_target(&target);
    assert_eq!(first, second);
    assert_eq!(first.len(), 64);
}
