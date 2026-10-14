use std::time::Instant;

use serde::Serialize;
use serde_json::Value;

pub const SAND_AUTO_REVIEW_BLOCK_REASON: &str = "Blocked by Auto-review";
pub const SAND_AUTO_REVIEW_CLASSIFIER_MAX_ATTEMPTS: u32 = 1;
pub const SAND_AUTO_REVIEW_CLASSIFIER_TIMEOUT_MS: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoReviewClassifierDecision {
    Allow,
    Block {
        reason: String,
        proposed_rule: Option<String>,
    },
    Reject {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmartModeClassifierDecision {
    Allow,
    Block,
    Unspecified,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmartModeClassifierSuccess {
    pub decision: SmartModeClassifierDecision,
    pub block_reason: Option<String>,
    pub proposed_allow_rule: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmartModeClassifierResult {
    Success(SmartModeClassifierSuccess),
    Error {
        failure_reason: Option<String>,
        retryable: Option<bool>,
    },
    Missing,
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoReviewClassifierError {
    Aborted(String),
    Timeout(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmartModeClassifierMeasurementKind {
    Started,
    Call,
    Exception,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SmartModeClassifierMeasurement {
    pub kind: SmartModeClassifierMeasurementKind,
    pub mode: String,
    pub action_kind: String,
    pub surface_label: Option<String>,
    pub timeout_ms: Option<u64>,
    pub has_target: Option<bool>,
    pub has_target_arguments: Option<bool>,
    pub outcome: Option<String>,
    pub decision: Option<String>,
    pub has_reason: Option<bool>,
    pub latency_ms: Option<f64>,
    pub retry_count: Option<u32>,
    pub failure_reason: Option<String>,
    pub retryable: Option<bool>,
}

pub struct AutoReviewClassifierRequest<'a, Target, Message> {
    pub tool_call_id: &'a str,
    pub parent_conversation_id: &'a str,
    pub mode: &'a str,
    pub target: Target,
    pub conversation_context: Vec<Message>,
    pub workspace_paths: &'a [String],
    pub suppress_tool_call_id_logging: bool,
    pub max_attempts: u32,
}

pub trait SandAutoReviewClassifierExecutor<Target, Message> {
    fn execute(
        &mut self,
        request: AutoReviewClassifierRequest<'_, Target, Message>,
    ) -> Result<SmartModeClassifierResult, AutoReviewClassifierError>;

    fn record_measurement(&mut self, _measurement: SmartModeClassifierMeasurement) {}
}

fn target_measurement<Target: Serialize>(target: &Target) -> (String, Option<String>, bool) {
    let Ok(value) = serde_json::to_value(target) else {
        return ("unknown".into(), None, false);
    };
    let Some(object) = value.as_object() else {
        return ("unknown".into(), None, false);
    };
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "unknown".into());
    let action_kind = match action.as_str() {
        "shell" | "mcp" | "sand_computer" | "web_fetch" | "fetch_mcp_resource" => action,
        _ => "other".into(),
    };
    let arguments = object.get("arguments").filter(|value| !value.is_null());
    let surface_label = arguments
        .and_then(Value::as_object)
        .and_then(|arguments| arguments.get("execution_surface"))
        .and_then(Value::as_str)
        .filter(|surface| matches!(*surface, "host_machine" | "isolated_box"))
        .map(str::to_string);
    (action_kind, surface_label, arguments.is_some())
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}

pub fn run_sand_auto_review_classifier<Target: Serialize, Message>(
    executor: &mut dyn SandAutoReviewClassifierExecutor<Target, Message>,
    tool_call_id: &str,
    parent_conversation_id: &str,
    mode: &str,
    build_target: impl FnOnce() -> Target,
    load_conversation_context: impl FnOnce() -> Result<Vec<Message>, AutoReviewClassifierError>,
    workspace_paths: &[String],
    error_reason: &str,
) -> Result<AutoReviewClassifierDecision, AutoReviewClassifierError> {
    let conversation_context = match load_conversation_context() {
        Ok(context) => context,
        Err(AutoReviewClassifierError::Aborted(reason)) => {
            return Err(AutoReviewClassifierError::Aborted(reason));
        }
        Err(AutoReviewClassifierError::Timeout(_))
        | Err(AutoReviewClassifierError::Failed(_)) => {
            return Ok(AutoReviewClassifierDecision::Reject {
                reason: error_reason.to_string(),
            });
        }
    };
    let target = build_target();
    let (action_kind, surface_label, has_target_arguments) = target_measurement(&target);
    executor.record_measurement(SmartModeClassifierMeasurement {
        kind: SmartModeClassifierMeasurementKind::Started,
        mode: mode.to_string(),
        action_kind: action_kind.clone(),
        surface_label: surface_label.clone(),
        timeout_ms: Some(SAND_AUTO_REVIEW_CLASSIFIER_TIMEOUT_MS),
        has_target: Some(true),
        has_target_arguments: Some(has_target_arguments),
        outcome: None,
        decision: None,
        has_reason: None,
        latency_ms: None,
        retry_count: None,
        failure_reason: None,
        retryable: None,
    });

    let started = Instant::now();
    let result = match executor.execute(AutoReviewClassifierRequest {
        tool_call_id,
        parent_conversation_id,
        mode,
        target,
        conversation_context,
        workspace_paths,
        suppress_tool_call_id_logging: true,
        max_attempts: SAND_AUTO_REVIEW_CLASSIFIER_MAX_ATTEMPTS,
    }) {
        Ok(result) => result,
        Err(AutoReviewClassifierError::Aborted(reason)) => {
            executor.record_measurement(SmartModeClassifierMeasurement {
                kind: SmartModeClassifierMeasurementKind::Exception,
                mode: mode.to_string(),
                action_kind,
                surface_label,
                timeout_ms: None,
                has_target: None,
                has_target_arguments: None,
                outcome: Some("exception".into()),
                decision: Some("unknown".into()),
                has_reason: Some(false),
                latency_ms: Some(elapsed_ms(started)),
                retry_count: Some(0),
                failure_reason: None,
                retryable: None,
            });
            return Err(AutoReviewClassifierError::Aborted(reason));
        }
        Err(AutoReviewClassifierError::Timeout(_)) => {
            executor.record_measurement(SmartModeClassifierMeasurement {
                kind: SmartModeClassifierMeasurementKind::Exception,
                mode: mode.to_string(),
                action_kind,
                surface_label,
                timeout_ms: None,
                has_target: None,
                has_target_arguments: None,
                outcome: Some("exception".into()),
                decision: Some("unknown".into()),
                has_reason: Some(false),
                latency_ms: Some(elapsed_ms(started)),
                retry_count: Some(0),
                failure_reason: Some("timeout_exception".into()),
                retryable: None,
            });
            return Ok(AutoReviewClassifierDecision::Reject {
                reason: error_reason.to_string(),
            });
        }
        Err(AutoReviewClassifierError::Failed(_)) => {
            executor.record_measurement(SmartModeClassifierMeasurement {
                kind: SmartModeClassifierMeasurementKind::Exception,
                mode: mode.to_string(),
                action_kind,
                surface_label,
                timeout_ms: None,
                has_target: None,
                has_target_arguments: None,
                outcome: Some("exception".into()),
                decision: Some("unknown".into()),
                has_reason: Some(false),
                latency_ms: Some(elapsed_ms(started)),
                retry_count: Some(0),
                failure_reason: Some("unknown_exception".into()),
                retryable: None,
            });
            return Ok(AutoReviewClassifierDecision::Reject {
                reason: error_reason.to_string(),
            });
        }
    };

    let (outcome, decision_label, has_reason, failure_reason, retryable) = match &result {
        SmartModeClassifierResult::Success(success) => match success.decision {
            SmartModeClassifierDecision::Allow => ("allow", "allow", success.block_reason.is_some(), None, Some(false)),
            SmartModeClassifierDecision::Block => ("block", "block", success.block_reason.is_some(), None, Some(false)),
            SmartModeClassifierDecision::Unspecified => (
                "missing",
                "unspecified",
                success.block_reason.is_some(),
                Some("unspecified_decision".to_string()),
                Some(true),
            ),
            SmartModeClassifierDecision::Unknown => (
                "missing",
                "unknown",
                success.block_reason.is_some(),
                Some("unknown_decision".to_string()),
                Some(true),
            ),
        },
        SmartModeClassifierResult::Error {
            failure_reason,
            retryable,
        } => (
            "error",
            "unknown",
            false,
            Some(
                failure_reason
                    .clone()
                    .unwrap_or_else(|| "classifier_result_error".into()),
            ),
            Some(retryable.unwrap_or(true)),
        ),
        SmartModeClassifierResult::Missing | SmartModeClassifierResult::Failure => (
            "missing",
            "unknown",
            false,
            Some("missing_result".into()),
            Some(true),
        ),
    };
    executor.record_measurement(SmartModeClassifierMeasurement {
        kind: SmartModeClassifierMeasurementKind::Call,
        mode: mode.to_string(),
        action_kind,
        surface_label,
        timeout_ms: None,
        has_target: None,
        has_target_arguments: None,
        outcome: Some(outcome.into()),
        decision: Some(decision_label.into()),
        has_reason: Some(has_reason),
        latency_ms: Some(elapsed_ms(started)),
        retry_count: Some(0),
        failure_reason,
        retryable,
    });

    let SmartModeClassifierResult::Success(success) = result else {
        return Ok(AutoReviewClassifierDecision::Reject {
            reason: error_reason.to_string(),
        });
    };

    match success.decision {
        SmartModeClassifierDecision::Allow => Ok(AutoReviewClassifierDecision::Allow),
        SmartModeClassifierDecision::Block => {
            let reason = success
                .block_reason
                .as_deref()
                .map(str::trim)
                .filter(|reason| !reason.is_empty())
                .unwrap_or(SAND_AUTO_REVIEW_BLOCK_REASON)
                .to_string();
            let proposed_rule = success
                .proposed_allow_rule
                .as_deref()
                .map(str::trim)
                .filter(|rule| !rule.is_empty())
                .map(ToOwned::to_owned);
            Ok(AutoReviewClassifierDecision::Block {
                reason,
                proposed_rule,
            })
        }
        SmartModeClassifierDecision::Unspecified | SmartModeClassifierDecision::Unknown => {
            Ok(AutoReviewClassifierDecision::Reject {
                reason: error_reason.to_string(),
            })
        }
    }
}
