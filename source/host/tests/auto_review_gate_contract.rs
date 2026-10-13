use std::sync::Arc;

use fabushi_mahayana_host::runner::auto_review_gate::{
    AutoReviewGate, AutoReviewGateDependencies, AutoReviewInstructions,
    ShellApprovalSurface,
};
use fabushi_mahayana_host::runner::sand_auto_review::{
    SandAutoReviewController, SandAutoReviewMode, SandAutoReviewModes,
    SandAutoReviewRequest, SandAutoReviewRequestOutcome, SandAutoReviewSurface,
};

struct Deps {
    controller: Arc<SandAutoReviewController>,
}

impl AutoReviewGateDependencies for Deps {
    fn base_modes(&self) -> SandAutoReviewModes {
        SandAutoReviewModes {
            host_shell: SandAutoReviewMode::Enforce,
            box_shell: SandAutoReviewMode::Shadow,
            mcp: SandAutoReviewMode::Off,
            computer: SandAutoReviewMode::Enforce,
            automation_write: SandAutoReviewMode::Off,
            cloud_agent: SandAutoReviewMode::Enforce,
            subagent_launch: SandAutoReviewMode::Off,
        }
    }

    fn controller(&self) -> Option<Arc<SandAutoReviewController>> {
        Some(self.controller.clone())
    }

    fn resolve_box_id(&self) -> String {
        "box-7".into()
    }

    fn instructions(&self) -> Option<AutoReviewInstructions> {
        Some(AutoReviewInstructions {
            allow_instructions: vec!["allow signed releases".into()],
            block_instructions: vec!["block destructive reset".into()],
        })
    }
}

fn request_pending(
    controller: &SandAutoReviewController,
    surface: SandAutoReviewSurface,
    fingerprint: &str,
) {
    let pending = controller.request_approval(SandAutoReviewRequest {
        agent_id: None,
        surface,
        fingerprint: fingerprint.into(),
        reason: "needs review".into(),
        summary: "sensitive action".into(),
        command: None,
        proposed_rule: None,
        expiry_policy: None,
    });
    assert!(matches!(pending, SandAutoReviewRequestOutcome::Pending(_)));
}

#[test]
fn auto_review_gate_expires_every_non_enforcing_surface_and_keeps_enforcing_ones() {
    let expiring = Arc::new(SandAutoReviewController::new("agent-expiring", "host-1"));
    for (surface, fingerprint) in [
        (SandAutoReviewSurface::BoxShell, "box-shell"),
        (SandAutoReviewSurface::Mcp, "mcp"),
        (SandAutoReviewSurface::AutomationWrite, "automation-write"),
        (SandAutoReviewSurface::SubagentLaunch, "subagent-launch"),
    ] {
        request_pending(expiring.as_ref(), surface, fingerprint);
    }
    let expiring_gate = AutoReviewGate::new(Arc::new(Deps {
        controller: expiring.clone(),
    }));
    let modes = expiring_gate.current_modes();
    assert_eq!(modes.box_shell, SandAutoReviewMode::Shadow);
    assert_eq!(modes.mcp, SandAutoReviewMode::Off);
    assert_eq!(modes.automation_write, SandAutoReviewMode::Off);
    assert_eq!(modes.subagent_launch, SandAutoReviewMode::Off);
    assert!(
        expiring.get_pending_approvals().is_empty(),
        "every non-enforcing frozen surface must expire its pending approval"
    );

    let enforcing = Arc::new(SandAutoReviewController::new("agent-enforcing", "host-1"));
    for (surface, fingerprint) in [
        (SandAutoReviewSurface::HostShell, "host-shell"),
        (SandAutoReviewSurface::Computer, "computer"),
        (SandAutoReviewSurface::CloudAgent, "cloud-agent"),
    ] {
        request_pending(enforcing.as_ref(), surface, fingerprint);
    }
    let enforcing_gate = AutoReviewGate::new(Arc::new(Deps {
        controller: enforcing.clone(),
    }));
    let modes = enforcing_gate.current_modes();
    assert_eq!(modes.host_shell, SandAutoReviewMode::Enforce);
    assert_eq!(modes.computer, SandAutoReviewMode::Enforce);
    assert_eq!(modes.cloud_agent, SandAutoReviewMode::Enforce);

    let remaining = enforcing
        .get_pending_approvals()
        .into_iter()
        .map(|approval| approval.surface)
        .collect::<Vec<_>>();
    assert_eq!(remaining.len(), 3);
    for surface in [
        SandAutoReviewSurface::HostShell,
        SandAutoReviewSurface::Computer,
        SandAutoReviewSurface::CloudAgent,
    ] {
        assert!(
            remaining.contains(&surface),
            "enforcing surface was expired: {surface:?}"
        );
    }
}

#[test]
fn auto_review_gate_blocks_new_side_effect_when_approval_is_pending() {
    let controller = Arc::new(SandAutoReviewController::new("agent-1", "host-1"));
    let pending = controller.request_approval(SandAutoReviewRequest {
        agent_id: None,
        surface: SandAutoReviewSurface::HostShell,
        fingerprint: "fingerprint".into(),
        reason: "needs review".into(),
        summary: "sensitive action".into(),
        command: None,
        proposed_rule: None,
        expiry_policy: None,
    });
    assert!(matches!(pending, SandAutoReviewRequestOutcome::Pending(_)));
    let gate = AutoReviewGate::new(Arc::new(Deps { controller }));

    let error = gate
        .assert_no_pending_approval()
        .expect_err("pending approval blocks the next side effect");
    assert_eq!(
        error.to_string(),
        "Another action is waiting for Auto-review approval; no new side effect may start yet."
    );
}

#[test]
fn shell_approval_identity_changes_only_after_side_effect_start() {
    let controller = Arc::new(SandAutoReviewController::new("agent-1", "host-1"));
    let gate = AutoReviewGate::new(Arc::new(Deps { controller }));

    assert_eq!(
        gate.shell_approval_identity(ShellApprovalSurface::HostShell),
        "host_shell:0"
    );
    assert_eq!(
        gate.shell_approval_identity(ShellApprovalSurface::BoxShell),
        "box_shell:box-7:0"
    );

    gate.mark_shell_side_effect_start(ShellApprovalSurface::BoxShell);

    assert_eq!(
        gate.shell_approval_identity(ShellApprovalSurface::HostShell),
        "host_shell:0"
    );
    assert_eq!(
        gate.shell_approval_identity(ShellApprovalSurface::BoxShell),
        "box_shell:box-7:1"
    );
}

#[test]
fn user_instructions_are_cloned_from_live_dependencies() {
    let controller = Arc::new(SandAutoReviewController::new("agent-1", "host-1"));
    let gate = AutoReviewGate::new(Arc::new(Deps { controller }));

    let instructions = gate.user_instructions().expect("instructions");
    assert_eq!(instructions.allow_instructions, vec!["allow signed releases"]);
    assert_eq!(
        instructions.block_instructions,
        vec!["block destructive reset"]
    );
}

#[test]
fn shipping_host_composes_shared_gate_into_every_frozen_side_effect_surface() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let main = std::fs::read_to_string(root.join("src/main.rs")).expect("shipping host main");
    let composition = std::fs::read_to_string(root.join("src/host_runner_composition.rs"))
        .expect("canonical Host Runner composition");
    let bridge = std::fs::read_to_string(root.join("src/runner_production_bridge.rs"))
        .expect("canonical production Runner bridge");

    for required in [
        "ProductionAutoReviewGateDeps",
        "AutoReviewGate::new",
        "assert_no_pending_approval()",
        "current_modes().mcp",
        "current_modes().box_shell",
        "current_modes().host_shell",
        "current_modes().computer",
        "current_modes().automation_write",
        "current_modes().cloud_agent",
        "current_modes().subagent_launch",
        "request_sand_mcp_approval(",
        "request_sand_shell_approval(",
        "run_sand_computer_auto_review_preflight(",
        "review_sand_automation_write(",
        "review_sand_cloud_agent_action(",
        "review_sand_cloud_agent_lifecycle_action(",
        "review_sand_subagent_action(",
        "shell_approval_identity(ShellApprovalSurface::BoxShell)",
        "shell_approval_identity(ShellApprovalSurface::HostShell)",
        "mark_shell_side_effect_start(",
        "mcp_review: Some(mcp_review)",
        "worker_host_runner_composition.compose_production_turn(",
        "ProductionTurnCompositionHooks {",
        "box_shell_review,",
        "routine_auto_review,",
        "external_shell_review: Some(external_shell_review)",
        "review: Some(cloud_agent_review)",
        "subagent_task_review,",
    ] {
        assert!(
            main.contains(required),
            "missing shipping Host auto-review gate input/delegation: {required}"
        );
    }

    assert!(
        composition.contains(
            "ProductionRunnerCompositionHooks as ProductionTurnCompositionHooks"
        ),
        "HostRunnerComposition must expose the bridge-owned turn hook contract"
    );
    assert!(
        composition.contains("create_production_runner_composition_with_hooks(input, hooks)"),
        "HostRunnerComposition must delegate immutable hook projection to the production bridge"
    );

    for required in [
        "pub struct ProductionRunnerCompositionHooks",
        "pub routine_auto_review: RoutineAutoReviewCallback",
        "pub box_shell_review: BoxShellAutoReviewCallback",
        ".with_routine_auto_review(hooks.routine_auto_review)",
        ".with_box_shell_review(hooks.box_shell_review)",
        "composition = composition.with_subagent_task_review(subagent_task_review)",
    ] {
        assert!(
            bridge.contains(required),
            "missing canonical production Runner bridge auto-review wiring: {required}"
        );
    }
}
