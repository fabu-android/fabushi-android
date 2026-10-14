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
fn shipping_android_host_freezes_auto_review_policy_before_every_routed_side_effect() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let runtime = std::fs::read_to_string(root.join("src/android_json_runtime.rs"))
        .expect("shipping Android Host runtime");

    // Android does not ship the Desktop src/main.rs production-composition graph. Its canonical
    // shipping boundary is AndroidJsonHost, so this contract verifies the Android production
    // adaptation rather than source-text names from the Desktop bootstrap. Policy is read from the
    // account-scoped durable owner exactly once for the frozen turn, then the same frozen modes and
    // instructions feed every routed side-effect preflight.
    for required in [
        "DurableAutoReviewSettings::open(",
        ".snapshot(&account_fence)?",
        "resolve_sand_auto_review_modes(",
        "std::env::var(\"SAND_AUTO_REVIEW_MODE\")",
        "let auto_review_instructions = auto_review_snapshot.instructions.clone();",
        "with_local_file_tools(",
        "with_remote_routed_tools(",
        "RemoteAutoReviewPreflight",
        "AndroidHostInferenceProvider::run_auto_review_target(",
        "\"Shell\" | \"Write\" => review_modes.box_shell",
        "\"ExternalShell\" => review_modes.host_shell",
        "\"Computer\" | \"Screenshot\" => review_modes.computer",
        "\"CloudAgent\" => review_modes.cloud_agent",
        "_ if tool_name.starts_with(\"browser_\") => review_modes.computer",
        "AndroidHostInferenceProvider::run_subagent_review(",
    ] {
        assert!(
            runtime.contains(required),
            "missing shipping Android Auto-review policy/preflight wiring: {required}"
        );
    }
}
