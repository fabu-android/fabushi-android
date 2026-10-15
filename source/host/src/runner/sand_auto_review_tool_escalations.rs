use serde_json::Value;

use super::sand_auto_review::{
    SandAutoReviewController, SandAutoReviewDecision, SandAutoReviewExpiryPolicy,
    SandAutoReviewRequest, SandAutoReviewRequestOutcome, SandAutoReviewSurface,
};
use super::sand_auto_review_summaries::{
    describe_sand_mcp_auto_review_action, describe_sand_shell_auto_review_action,
    summarize_sand_mcp_auto_review_action,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellApprovalTarget {
    pub surface: SandAutoReviewSurface,
    pub description: Option<String>,
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellApprovalRequest {
    pub target: ShellApprovalTarget,
    pub fingerprint: String,
    pub reason: String,
    pub command: String,
    pub proposed_rule: Option<String>,
    pub expiry_policy: SandAutoReviewExpiryPolicy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpApprovalRequest {
    pub fingerprint: String,
    pub reason: String,
    pub server_display_name: String,
    pub tool_name: String,
    pub mcp_arguments: Option<Value>,
    pub description: Option<String>,
    pub proposed_rule: Option<String>,
    pub expiry_policy: SandAutoReviewExpiryPolicy,
}

pub fn request_sand_shell_approval<F, R>(
    controller: &SandAutoReviewController,
    request: &ShellApprovalRequest,
    mut before_approval: F,
    mut recheck: R,
) -> Result<SandAutoReviewDecision, String>
where
    F: FnMut(&ShellApprovalRequest) -> Result<(), String>,
    R: FnMut(&ShellApprovalRequest) -> Result<(), String>,
{
    before_approval(request)?;
    let surface = match request.target.surface {
        SandAutoReviewSurface::HostShell => SandAutoReviewSurface::HostShell,
        _ => SandAutoReviewSurface::BoxShell,
    };
    let outcome = controller.request_approval(SandAutoReviewRequest {
        agent_id: None,
        surface,
        fingerprint: request.fingerprint.clone(),
        reason: request.reason.clone(),
        summary: describe_sand_shell_auto_review_action(
            match request.target.surface {
                SandAutoReviewSurface::HostShell => "host_shell",
                _ => "box_shell",
            },
            request.target.description.as_deref(),
            request.target.working_directory.as_deref(),
        ),
        command: Some(request.command.clone()),
        proposed_rule: request.proposed_rule.clone(),
        expiry_policy: Some(request.expiry_policy),
    });
    let decision = await_decision(outcome)?;
    if matches!(decision, SandAutoReviewDecision::Approved) {
        recheck(request)?;
    }
    Ok(decision)
}

pub fn request_sand_mcp_approval(
    controller: &SandAutoReviewController,
    request: &McpApprovalRequest,
) -> Result<SandAutoReviewDecision, String> {
    let outcome = controller.request_approval(SandAutoReviewRequest {
        agent_id: None,
        surface: SandAutoReviewSurface::Mcp,
        fingerprint: request.fingerprint.clone(),
        reason: request.reason.clone(),
        summary: describe_sand_mcp_auto_review_action(
            request.description.as_deref(),
            &request.server_display_name,
            &request.tool_name,
            request.mcp_arguments.as_ref(),
        ),
        command: Some(summarize_sand_mcp_auto_review_action(
            &request.server_display_name,
            &request.tool_name,
            request.mcp_arguments.as_ref(),
        )),
        proposed_rule: request.proposed_rule.clone(),
        expiry_policy: Some(request.expiry_policy),
    });
    await_decision(outcome)
}

fn await_decision(outcome: SandAutoReviewRequestOutcome) -> Result<SandAutoReviewDecision, String> {
    match outcome {
        SandAutoReviewRequestOutcome::Immediate(decision) => Ok(decision),
        SandAutoReviewRequestOutcome::Pending(pending) => {
            pending.wait().map_err(|error| error.to_string())
        }
    }
}
