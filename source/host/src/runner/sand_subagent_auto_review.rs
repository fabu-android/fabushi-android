use serde_json::{Value, json};

use super::sand_auto_review::{
    SandAutoReviewController, SandAutoReviewDecision, SandAutoReviewMode,
    SandAutoReviewRequest, SandAutoReviewRequestOutcome, SandAutoReviewSurface,
    fingerprint_sand_auto_review_target, sand_auto_review_approval_expiry_policy,
};
use super::sand_auto_review_classifier_run::{
    AutoReviewClassifierDecision, AutoReviewClassifierError,
};
use super::sand_auto_review_summaries::summarize_sand_subagent_action;
use super::sand_computer_auto_review::{
    InstructionPermissions, build_project_permissions_context,
};

pub const SAND_SUBAGENT_CLASSIFIER_TARGET_ACTION: &str = "sand_subagent";
pub const SAND_SUBAGENT_CLASSIFIER_ERROR_REASON: &str =
    "An error occurred while classifying this task action. Please review manually.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentReviewTarget {
    pub action: String,
    pub prompt: String,
    pub subagent_id: Option<String>,
    pub subagent_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentReviewOutcome {
    Allowed,
    Blocked(String),
    Cancelled,
}

pub fn build_sand_subagent_launch_review_target(
    prompt: &str,
    subagent_type: Option<&str>,
) -> Option<SubagentReviewTarget> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return None;
    }
    Some(SubagentReviewTarget {
        action: "launch".into(),
        prompt: prompt.into(),
        subagent_id: None,
        subagent_type: subagent_type
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    })
}

pub fn build_sand_subagent_steer_review_target(
    subagent_id: &str,
    prompt: &str,
) -> Option<SubagentReviewTarget> {
    let prompt = prompt.trim();
    let subagent_id = subagent_id.trim();
    if prompt.is_empty() || subagent_id.is_empty() {
        return None;
    }
    Some(SubagentReviewTarget {
        action: "steer".into(),
        prompt: prompt.into(),
        subagent_id: Some(subagent_id.into()),
        subagent_type: None,
    })
}

pub fn build_sand_subagent_risk_target(
    target: &SubagentReviewTarget,
    personal: Option<&InstructionPermissions>,
    user: Option<&InstructionPermissions>,
    project: Option<&InstructionPermissions>,
) -> Value {
    json!({
        "action": SAND_SUBAGENT_CLASSIFIER_TARGET_ACTION,
        "arguments": {
            "action": target.action,
            "prompt": target.prompt,
            "subagent_id": target.subagent_id,
            "subagent_type": target.subagent_type,
            "permissions": build_project_permissions_context(personal, user, project),
        }
    })
}

pub fn review_sand_subagent_action<F, C>(
    mode: SandAutoReviewMode,
    target: &SubagentReviewTarget,
    controller: Option<&SandAutoReviewController>,
    request_source: &str,
    cancelled: C,
    mut classify: F,
) -> Result<SubagentReviewOutcome, AutoReviewClassifierError>
where
    F: FnMut(&Value, &str) -> Result<AutoReviewClassifierDecision, AutoReviewClassifierError>,
    C: Fn() -> bool,
{
    if mode == SandAutoReviewMode::Off {
        return Ok(SubagentReviewOutcome::Allowed);
    }
    let risk_target = build_sand_subagent_risk_target(target, None, None, None);
    if mode == SandAutoReviewMode::Shadow {
        let _ = classify(&risk_target, "shadow");
        return Ok(SubagentReviewOutcome::Allowed);
    }
    if cancelled() {
        return Ok(SubagentReviewOutcome::Cancelled);
    }
    let decision = classify(&risk_target, "enforce")?;
    if cancelled() {
        return Ok(SubagentReviewOutcome::Cancelled);
    }
    let (reason, proposed_rule) = match decision {
        AutoReviewClassifierDecision::Allow => return Ok(SubagentReviewOutcome::Allowed),
        AutoReviewClassifierDecision::Reject { reason } => {
            return Ok(SubagentReviewOutcome::Blocked(reason));
        }
        AutoReviewClassifierDecision::Block { reason, proposed_rule } => (reason, proposed_rule),
    };
    let Some(controller) = controller else {
        return Ok(SubagentReviewOutcome::Blocked(reason));
    };
    let request = SandAutoReviewRequest {
        agent_id: None,
        surface: SandAutoReviewSurface::SubagentLaunch,
        fingerprint: fingerprint_sand_auto_review_target(&risk_target),
        reason: reason.clone(),
        summary: summarize_sand_subagent_action(&target.action, &target.prompt),
        command: None,
        proposed_rule,
        expiry_policy: Some(sand_auto_review_approval_expiry_policy(request_source)),
    };
    let decision = match controller.request_approval(request) {
        SandAutoReviewRequestOutcome::Immediate(decision) => decision,
        SandAutoReviewRequestOutcome::Pending(pending) => pending.wait().map_err(|error| {
            AutoReviewClassifierError::Failed(format!(
                "Auto-review approval channel closed: {error}"
            ))
        })?,
    };
    if cancelled() {
        return Ok(SubagentReviewOutcome::Cancelled);
    }
    Ok(match decision {
        SandAutoReviewDecision::Approved => SubagentReviewOutcome::Allowed,
        SandAutoReviewDecision::Denied { reason } => SubagentReviewOutcome::Blocked(reason),
    })
}
