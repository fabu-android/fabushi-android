use serde_json::{Map, Value, json};

use crate::automations::automation::describe_trigger;
use crate::automations::automation_trigger::{
    serialize_stored_trigger, trigger_schedule,
};

use super::sand_auto_review::{
    SandAutoReviewController, SandAutoReviewDecision, SandAutoReviewExpiryPolicy,
    SandAutoReviewMode, SandAutoReviewRequest, SandAutoReviewRequestOutcome,
    SandAutoReviewSurface, fingerprint_sand_auto_review_target,
};
use super::sand_auto_review_classifier_run::{
    AutoReviewClassifierDecision, AutoReviewClassifierError,
};
use super::sand_auto_review_summaries::summarize_sand_automation_write_action;
use super::sand_computer_auto_review::{
    InstructionPermissions, build_project_permissions_context,
};

pub const SAND_AUTOMATION_WRITE_CLASSIFIER_TARGET_ACTION: &str = "sand_automation_write";
pub const SAND_AUTOMATION_WRITE_CLASSIFIER_ERROR_REASON: &str =
    "An error occurred while reviewing this routine. Please review manually.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomationReference {
    pub id: String,
    pub name: String,
    pub body: Option<String>,
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutomationWriteSpec {
    pub name: String,
    pub prompt: String,
    pub trigger: Value,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutomationWriteTarget {
    pub operation: String,
    pub id: String,
    pub spec: AutomationWriteSpec,
    pub referenced_workflows: Vec<AutomationReference>,
    pub referencing_routines: Vec<AutomationReference>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutomationReviewOutcome {
    Allowed,
    Blocked(String),
}

pub fn build_sand_automation_write_risk_target(
    target: &AutomationWriteTarget,
) -> Value {
    build_sand_automation_write_risk_target_with_permissions(
        target,
        None,
        None,
        None,
    )
}

pub fn build_sand_automation_write_risk_target_with_permissions(
    target: &AutomationWriteTarget,
    personal: Option<&InstructionPermissions>,
    user: Option<&InstructionPermissions>,
    project: Option<&InstructionPermissions>,
) -> Value {
    let mut arguments = Map::new();
    arguments.insert("surface".into(), Value::String("automation_write".into()));
    arguments.insert("operation".into(), Value::String(target.operation.clone()));
    arguments.insert("automation_id".into(), Value::String(target.id.clone()));
    arguments.insert("name".into(), Value::String(target.spec.name.clone()));
    arguments.insert("prompt".into(), Value::String(target.spec.prompt.clone()));
    arguments.insert(
        "trigger".into(),
        serialize_stored_trigger(&target.spec.trigger),
    );
    if let Some(schedule) = trigger_schedule(&target.spec.trigger) {
        arguments.insert("schedule".into(), Value::String(schedule));
    }
    arguments.insert(
        "trigger_description".into(),
        Value::String(describe_trigger(&target.spec.trigger)),
    );
    arguments.insert("enabled".into(), Value::Bool(target.spec.is_enabled));
    arguments.insert(
        "referenced_workflows".into(),
        Value::Array(
            target
                .referenced_workflows
                .iter()
                .map(|workflow| {
                    let mut value = Map::new();
                    value.insert("id".into(), Value::String(workflow.id.clone()));
                    value.insert("name".into(), Value::String(workflow.name.clone()));
                    if let Some(body) = &workflow.body {
                        value.insert("body".into(), Value::String(body.clone()));
                    }
                    Value::Object(value)
                })
                .collect(),
        ),
    );
    arguments.insert(
        "referencing_routines".into(),
        Value::Array(
            target
                .referencing_routines
                .iter()
                .map(|routine| {
                    let mut value = Map::new();
                    value.insert("id".into(), Value::String(routine.id.clone()));
                    value.insert("name".into(), Value::String(routine.name.clone()));
                    if let Some(prompt) = &routine.prompt {
                        value.insert("prompt".into(), Value::String(prompt.clone()));
                    }
                    Value::Object(value)
                })
                .collect(),
        ),
    );
    if let Some(permissions) = build_project_permissions_context(personal, user, project) {
        arguments.insert("project_permissions".into(), permissions);
    }
    json!({
        "action": SAND_AUTOMATION_WRITE_CLASSIFIER_TARGET_ACTION,
        "arguments": Value::Object(arguments),
    })
}

pub fn review_sand_automation_write<F>(
    mode: SandAutoReviewMode,
    target: &AutomationWriteTarget,
    agent_id: &str,
    controller: Option<&SandAutoReviewController>,
    request_source: &str,
    mut classify: F,
) -> Result<AutomationReviewOutcome, AutoReviewClassifierError>
where
    F: FnMut(&Value, &str) -> Result<AutoReviewClassifierDecision, AutoReviewClassifierError>,
{
    if mode == SandAutoReviewMode::Off {
        return Ok(AutomationReviewOutcome::Allowed);
    }
    let risk_target = build_sand_automation_write_risk_target(target);
    if mode == SandAutoReviewMode::Shadow {
        let _ = classify(&risk_target, "shadow");
        return Ok(AutomationReviewOutcome::Allowed);
    }
    let decision = classify(&risk_target, "enforce")?;
    match decision {
        AutoReviewClassifierDecision::Allow => Ok(AutomationReviewOutcome::Allowed),
        AutoReviewClassifierDecision::Reject { reason } => {
            Ok(AutomationReviewOutcome::Blocked(reason))
        }
        AutoReviewClassifierDecision::Block {
            reason,
            proposed_rule,
        } => {
            let Some(controller) = controller else {
                return Ok(AutomationReviewOutcome::Blocked(reason));
            };
            let summary = summarize_sand_automation_write_action(
                &target.operation,
                &target.spec.name,
                &describe_trigger(&target.spec.trigger),
                &target.spec.prompt,
                Some(target.spec.is_enabled),
                &target
                    .referencing_routines
                    .iter()
                    .map(|routine| routine.name.clone())
                    .collect::<Vec<_>>(),
            );
            let request = SandAutoReviewRequest {
                agent_id: Some(agent_id.to_string()),
                surface: SandAutoReviewSurface::AutomationWrite,
                fingerprint: fingerprint_sand_auto_review_target(&risk_target),
                reason: reason.clone(),
                summary,
                command: None,
                proposed_rule,
                expiry_policy: Some(
                    super::sand_auto_review::sand_auto_review_approval_expiry_policy(
                        request_source,
                    ),
                ),
            };
            settle(controller.request_approval(request), reason)
        }
    }
}

fn settle(
    outcome: SandAutoReviewRequestOutcome,
    classifier_reason: String,
) -> Result<AutomationReviewOutcome, AutoReviewClassifierError> {
    let decision = match outcome {
        SandAutoReviewRequestOutcome::Immediate(decision) => decision,
        SandAutoReviewRequestOutcome::Pending(pending) => pending.wait().map_err(|error| {
            AutoReviewClassifierError::Failed(format!(
                "Auto-review approval channel closed: {error}"
            ))
        })?,
    };
    Ok(match decision {
        SandAutoReviewDecision::Approved => AutomationReviewOutcome::Allowed,
        SandAutoReviewDecision::Denied { reason } => {
            AutomationReviewOutcome::Blocked(if reason.is_empty() {
                classifier_reason
            } else {
                reason
            })
        }
    })
}

pub fn default_automation_approval_expiry_policy(source: &str) -> SandAutoReviewExpiryPolicy {
    super::sand_auto_review::sand_auto_review_approval_expiry_policy(source)
}
