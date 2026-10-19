use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::sand_auto_review::{
    SandAutoReviewController, SandAutoReviewDecision, SandAutoReviewMode,
    SandAutoReviewRequest, SandAutoReviewRequestOutcome, SandAutoReviewSurface,
    fingerprint_sand_auto_review_target, sand_auto_review_approval_expiry_policy,
};
use super::sand_auto_review_classifier_run::{
    AutoReviewClassifierDecision, AutoReviewClassifierError,
};
use super::sand_auto_review_summaries::{
    CloudLifecycleAction, summarize_sand_cloud_agent_action,
    summarize_sand_cloud_agent_lifecycle_action,
};

pub const SAND_CLOUD_AGENT_CLASSIFIER_TARGET_ACTION: &str = "sand_cloud_agent";
pub const SAND_CLOUD_AGENT_CLASSIFIER_ERROR_REASON: &str =
    "An error occurred while classifying this cloud-agent action. Please review manually.";
pub const SAND_CLOUD_AGENT_AUTO_REVIEW_ACTIONS: &[&str] = &["launch", "reply"];
pub const SAND_CLOUD_AGENT_LIFECYCLE_REVIEW_ACTIONS: &[&str] =
    &["rename", "cancel", "archive", "unarchive", "delete"];
pub const LIFECYCLE_REVIEW_UNAVAILABLE_REASON: &str =
    "This action needs Auto-review approval, which isn't available in this conversation. Run it from a direct chat with the assistant.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudAgentReviewImage {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudAgentReviewTarget {
    pub action: String,
    pub prompt: String,
    pub agent_id: Option<String>,
    pub images: Vec<Value>,
    pub interrupt: bool,
    pub repo_url: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudAgentLifecycleReviewTarget {
    pub action: String,
    pub agent_id: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudAgentReviewOutcome {
    Allowed,
    Blocked(String),
    Cancelled,
}

pub fn is_sand_cloud_agent_auto_review_action(action: &str) -> bool {
    SAND_CLOUD_AGENT_AUTO_REVIEW_ACTIONS.contains(&action)
}

pub fn is_sand_cloud_agent_lifecycle_review_action(action: &str) -> bool {
    SAND_CLOUD_AGENT_LIFECYCLE_REVIEW_ACTIONS.contains(&action)
}

pub fn describe_sand_cloud_agent_review_images(images: &[CloudAgentReviewImage]) -> Vec<Value> {
    images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            let mut hash = Sha256::new();
            hash.update(&image.data);
            json!({
                "index": index,
                "name": image.name,
                "bytes": image.data.len(),
                "sha256": format!("{:x}", hash.finalize()),
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn build_sand_cloud_agent_review_target(
    action: &str,
    prompt: &str,
    agent_id: Option<&str>,
    images: &[CloudAgentReviewImage],
    interrupt: bool,
    repo_url: Option<&str>,
    title: Option<&str>,
) -> Option<CloudAgentReviewTarget> {
    if !is_sand_cloud_agent_auto_review_action(action) {
        return None;
    }
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return None;
    }
    Some(CloudAgentReviewTarget {
        action: action.to_string(),
        prompt: prompt.to_string(),
        agent_id: agent_id.map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned),
        images: describe_sand_cloud_agent_review_images(images),
        interrupt,
        repo_url: repo_url.map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned),
        title: title.map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned),
    })
}

pub fn build_sand_cloud_agent_risk_target(target: &CloudAgentReviewTarget) -> Value {
    json!({
        "action": SAND_CLOUD_AGENT_CLASSIFIER_TARGET_ACTION,
        "arguments": {
            "action": target.action,
            "prompt": target.prompt,
            "agent_id": target.agent_id,
            "images": target.images,
            "interrupt": target.interrupt,
            "repo_url": target.repo_url,
            "title": target.title,
        }
    })
}

pub fn review_sand_cloud_agent_action<F, C>(
    mode: SandAutoReviewMode,
    target: &CloudAgentReviewTarget,
    controller: Option<&SandAutoReviewController>,
    request_source: &str,
    cancelled: C,
    mut classify: F,
) -> Result<CloudAgentReviewOutcome, AutoReviewClassifierError>
where
    F: FnMut(&Value, &str) -> Result<AutoReviewClassifierDecision, AutoReviewClassifierError>,
    C: Fn() -> bool,
{
    if mode == SandAutoReviewMode::Off {
        return Ok(CloudAgentReviewOutcome::Allowed);
    }
    let risk_target = build_sand_cloud_agent_risk_target(target);
    if mode == SandAutoReviewMode::Shadow {
        let _ = classify(&risk_target, "shadow");
        return Ok(CloudAgentReviewOutcome::Allowed);
    }
    if cancelled() {
        return Ok(CloudAgentReviewOutcome::Cancelled);
    }
    let decision = classify(&risk_target, "enforce")?;
    if cancelled() {
        return Ok(CloudAgentReviewOutcome::Cancelled);
    }
    let (reason, proposed_rule) = match decision {
        AutoReviewClassifierDecision::Allow => return Ok(CloudAgentReviewOutcome::Allowed),
        AutoReviewClassifierDecision::Reject { reason } => {
            return Ok(CloudAgentReviewOutcome::Blocked(reason));
        }
        AutoReviewClassifierDecision::Block { reason, proposed_rule } => (reason, proposed_rule),
    };
    let Some(controller) = controller else {
        return Ok(CloudAgentReviewOutcome::Blocked(reason));
    };
    let request = SandAutoReviewRequest {
        agent_id: None,
        surface: SandAutoReviewSurface::CloudAgent,
        fingerprint: fingerprint_sand_auto_review_target(&risk_target),
        reason: reason.clone(),
        summary: summarize_sand_cloud_agent_action(
            &target.action,
            &target.prompt,
            target.agent_id.as_deref(),
            target.images.len(),
            target.interrupt,
            target.repo_url.as_deref(),
            target.title.as_deref(),
        ),
        command: None,
        proposed_rule,
        expiry_policy: Some(sand_auto_review_approval_expiry_policy(request_source)),
    };
    let decision = settle(controller.request_approval(request), &reason)?;
    if cancelled() {
        return Ok(CloudAgentReviewOutcome::Cancelled);
    }
    Ok(decision)
}

pub fn build_sand_cloud_agent_lifecycle_review_target(
    action: &str,
    agent_id: &str,
    title: Option<&str>,
) -> Option<CloudAgentLifecycleReviewTarget> {
    if !is_sand_cloud_agent_lifecycle_review_action(action) {
        return None;
    }
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        return None;
    }
    Some(CloudAgentLifecycleReviewTarget {
        action: action.to_string(),
        agent_id: agent_id.to_string(),
        title: title.map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned),
    })
}

pub fn sand_cloud_agent_lifecycle_reason(action: CloudLifecycleAction) -> &'static str {
    match action {
        CloudLifecycleAction::Delete => {
            "Deleting a cloud agent is permanent and cannot be undone, so it needs your approval."
        }
        CloudLifecycleAction::Cancel => {
            "Cancelling a cloud agent's active run stops its work, so it needs your approval."
        }
        CloudLifecycleAction::Archive => "Archiving a cloud agent needs your approval.",
        CloudLifecycleAction::Unarchive => "Unarchiving a cloud agent needs your approval.",
        CloudLifecycleAction::Rename => {
            "Renaming a cloud agent changes how it appears everywhere, so it needs your approval."
        }
    }
}

pub fn review_sand_cloud_agent_lifecycle_action<C>(
    mode: SandAutoReviewMode,
    action: CloudLifecycleAction,
    target: &CloudAgentLifecycleReviewTarget,
    controller: Option<&SandAutoReviewController>,
    request_source: &str,
    cancelled: C,
) -> Result<CloudAgentReviewOutcome, AutoReviewClassifierError>
where
    C: Fn() -> bool,
{
    if mode != SandAutoReviewMode::Enforce {
        return Ok(CloudAgentReviewOutcome::Allowed);
    }
    if cancelled() {
        return Ok(CloudAgentReviewOutcome::Cancelled);
    }
    let Some(controller) = controller else {
        return Ok(CloudAgentReviewOutcome::Blocked(
            LIFECYCLE_REVIEW_UNAVAILABLE_REASON.into(),
        ));
    };
    let reason = sand_cloud_agent_lifecycle_reason(action).to_string();
    let target_json = json!({
        "action": target.action,
        "agent_id": target.agent_id,
        "title": target.title,
    });
    let request = SandAutoReviewRequest {
        agent_id: None,
        surface: SandAutoReviewSurface::CloudAgent,
        fingerprint: fingerprint_sand_auto_review_target(&target_json),
        reason: reason.clone(),
        summary: summarize_sand_cloud_agent_lifecycle_action(
            action,
            &target.agent_id,
            target.title.as_deref(),
        ),
        command: None,
        proposed_rule: None,
        expiry_policy: Some(sand_auto_review_approval_expiry_policy(request_source)),
    };
    let decision = settle(controller.request_approval(request), &reason)?;
    if cancelled() {
        Ok(CloudAgentReviewOutcome::Cancelled)
    } else {
        Ok(decision)
    }
}

fn settle(
    outcome: SandAutoReviewRequestOutcome,
    fallback: &str,
) -> Result<CloudAgentReviewOutcome, AutoReviewClassifierError> {
    let decision = match outcome {
        SandAutoReviewRequestOutcome::Immediate(decision) => decision,
        SandAutoReviewRequestOutcome::Pending(pending) => pending.wait().map_err(|error| {
            AutoReviewClassifierError::Failed(format!(
                "Auto-review approval channel closed: {error}"
            ))
        })?,
    };
    Ok(match decision {
        SandAutoReviewDecision::Approved => CloudAgentReviewOutcome::Allowed,
        SandAutoReviewDecision::Denied { reason } => CloudAgentReviewOutcome::Blocked(
            if reason.is_empty() { fallback.into() } else { reason },
        ),
    })
}
