use serde_json::{Value, json};

use super::sand_auto_review::{
    SandAutoReviewController, SandAutoReviewDecision, SandAutoReviewMode,
    SandAutoReviewRequest, SandAutoReviewRequestOutcome, SandAutoReviewSurface,
    fingerprint_sand_auto_review_target, sand_auto_review_approval_expiry_policy,
};
use super::sand_auto_review_classifier_run::{
    AutoReviewClassifierDecision, AutoReviewClassifierError,
};
use super::sand_auto_review_summaries::{
    SandBrowserSummaryArgs, summarize_sand_browser_auto_review_action,
};
use super::sand_computer_auto_review::{
    BoxIdentity, InstructionPermissions, SAND_COMPUTER_CLASSIFIER_TARGET_ACTION,
    build_project_permissions_context,
};

pub const SAND_BROWSER_AUTO_REVIEW_MAX_ELEMENT_CHARS: usize = 500;
pub const SAND_BROWSER_AUTO_REVIEW_MAX_TEXT_CHARS: usize = 2_000;
pub const SAND_BROWSER_AUTO_REVIEW_MAX_URL_CHARS: usize = 2_000;
pub const SAND_BROWSER_AUTO_REVIEW_MAX_KEY_CHARS: usize = 256;
pub const SAND_BROWSER_AUTO_REVIEW_MAX_CDP_PARAMS_CHARS: usize = 2_000;
pub const SAND_BROWSER_AUTO_REVIEW_MAX_FILE_PATH_CHARS: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SandBrowserAutoReviewBlockedError(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandBrowserReviewState {
    pub display_state_identity: String,
    pub target_page_url: Option<String>,
}

pub fn is_sand_browser_auto_review_mutating_action(action: &Value) -> bool {
    let op = action.get("op").and_then(Value::as_str).unwrap_or_default();
    if op == "tabs" {
        return matches!(
            action.get("tabsAction").and_then(Value::as_str),
            Some("new" | "close")
        );
    }
    !matches!(
        op,
        "screenshot" | "snapshot" | "get_page_text" | "wait" | "scroll" | "tabs"
    )
}

pub fn normalize_sand_browser_exact_action_args(
    action: &Value,
) -> Result<Value, SandBrowserAutoReviewBlockedError> {
    let object = action
        .as_object()
        .ok_or_else(|| SandBrowserAutoReviewBlockedError("Browser action must be an object.".into()))?;
    bounded(object.get("url").and_then(Value::as_str), "url", SAND_BROWSER_AUTO_REVIEW_MAX_URL_CHARS)?;
    bounded(object.get("text").and_then(Value::as_str), "text", SAND_BROWSER_AUTO_REVIEW_MAX_TEXT_CHARS)?;
    bounded(object.get("value").and_then(Value::as_str), "value", SAND_BROWSER_AUTO_REVIEW_MAX_TEXT_CHARS)?;
    bounded(object.get("key").and_then(Value::as_str), "key", SAND_BROWSER_AUTO_REVIEW_MAX_KEY_CHARS)?;
    bounded(object.get("cdpParams").and_then(Value::as_str), "params", SAND_BROWSER_AUTO_REVIEW_MAX_CDP_PARAMS_CHARS)?;
    bounded(object.get("filePath").and_then(Value::as_str), "filePath", SAND_BROWSER_AUTO_REVIEW_MAX_FILE_PATH_CHARS)?;
    if object
        .get("values")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
                .chars()
                .count()
        })
        .unwrap_or(0)
        > SAND_BROWSER_AUTO_REVIEW_MAX_TEXT_CHARS
    {
        return Err(reject("values", SAND_BROWSER_AUTO_REVIEW_MAX_TEXT_CHARS));
    }
    let mut normalized = action.clone();
    if let Some(element) = normalize_sand_browser_element(
        object.get("element").and_then(Value::as_str),
    )? {
        normalized["element"] = Value::String(element);
    }
    Ok(normalized)
}

pub fn normalize_sand_browser_element(
    element: Option<&str>,
) -> Result<Option<String>, SandBrowserAutoReviewBlockedError> {
    let Some(element) = element.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    bounded(
        Some(element),
        "element",
        SAND_BROWSER_AUTO_REVIEW_MAX_ELEMENT_CHARS,
    )?;
    Ok(Some(element.to_string()))
}

pub fn build_sand_browser_auto_review_canonical_target(
    exact_action: &Value,
    box_identity: &BoxIdentity,
    review_state: &SandBrowserReviewState,
) -> Result<Value, SandBrowserAutoReviewBlockedError> {
    Ok(json!({
        "exact_action": normalize_sand_browser_exact_action_args(exact_action)?,
        "box_identity": {
            "box_id": box_identity.box_id,
            "window_generation": box_identity.window_generation,
        },
        "display_state_identity": review_state.display_state_identity,
        "target_page_url": review_state.target_page_url,
    }))
}

pub fn fingerprint_sand_browser_auto_review_target(target: &Value) -> String {
    fingerprint_sand_auto_review_target(&json!({
        "exact_action": target.get("exact_action"),
        "window_generation": target.pointer("/box_identity/window_generation"),
        "box_id": target.pointer("/box_identity/box_id"),
        "display_state_identity": target.get("display_state_identity"),
        "target_page_url": target.get("target_page_url"),
    }))
}

pub fn build_sand_browser_classifier_risk_target(
    canonical_target: &Value,
    personal: Option<&InstructionPermissions>,
    user: Option<&InstructionPermissions>,
    project: Option<&InstructionPermissions>,
) -> Value {
    json!({
        "action": SAND_COMPUTER_CLASSIFIER_TARGET_ACTION,
        "arguments": {
            "exact_action": canonical_target.get("exact_action").cloned().unwrap_or(Value::Null),
            "target_page_url": canonical_target.get("target_page_url").cloned().unwrap_or(Value::Null),
            "box_id": canonical_target.pointer("/box_identity/box_id").cloned().unwrap_or(Value::Null),
            "window_generation": canonical_target.pointer("/box_identity/window_generation").cloned().unwrap_or(Value::Null),
            "display_state_identity": canonical_target.get("display_state_identity").cloned().unwrap_or(Value::Null),
            "permissions": build_project_permissions_context(personal, user, project),
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_sand_browser_auto_review_preflight<F, C>(
    mode: SandAutoReviewMode,
    exact_action: &Value,
    box_identity: &BoxIdentity,
    agent_id: &str,
    request_source: &str,
    controller: Option<&SandAutoReviewController>,
    mut capture_review_state: C,
    mut classify: F,
) -> Result<(), SandBrowserAutoReviewBlockedError>
where
    F: FnMut(&Value, &str) -> Result<AutoReviewClassifierDecision, AutoReviewClassifierError>,
    C: FnMut() -> Result<SandBrowserReviewState, String>,
{
    if mode == SandAutoReviewMode::Off || !is_sand_browser_auto_review_mutating_action(exact_action) {
        return Ok(());
    }
    let op = exact_action.get("op").and_then(Value::as_str).unwrap_or_default();
    if mode == SandAutoReviewMode::Enforce
        && matches!(op, "click" | "mouse_click_xy" | "drag" | "download" | "upload_file")
        && normalize_sand_browser_element(exact_action.get("element").and_then(Value::as_str))?.is_none()
    {
        return Err(SandBrowserAutoReviewBlockedError(
            "Browser click, drag, download, and upload actions require an element description in Auto-review enforce mode.".into(),
        ));
    }
    let initial_state = capture_review_state().map_err(SandBrowserAutoReviewBlockedError)?;
    let canonical = build_sand_browser_auto_review_canonical_target(
        exact_action,
        box_identity,
        &initial_state,
    )?;
    let risk_target = build_sand_browser_classifier_risk_target(&canonical, None, None, None);
    if mode == SandAutoReviewMode::Shadow {
        let _ = classify(&risk_target, "shadow");
        return Ok(());
    }
    let decision = classify(&risk_target, "enforce")
        .map_err(|error| SandBrowserAutoReviewBlockedError(format!("{error:?}")))?;
    let (reason, proposed_rule) = match decision {
        AutoReviewClassifierDecision::Allow => {
            let next = capture_review_state().map_err(SandBrowserAutoReviewBlockedError)?;
            if next.display_state_identity != initial_state.display_state_identity {
                if let Some(controller) = controller {
                    controller.report_display_recheck_failed(Some(agent_id));
                }
                return Err(SandBrowserAutoReviewBlockedError(
                    "The page changed after review; take a fresh browser_snapshot and retry the action.".into(),
                ));
            }
            return Ok(());
        }
        AutoReviewClassifierDecision::Reject { reason } => {
            return Err(SandBrowserAutoReviewBlockedError(reason));
        }
        AutoReviewClassifierDecision::Block {
            reason,
            proposed_rule,
        } => (reason, proposed_rule),
    };
    let Some(controller) = controller else {
        return Err(SandBrowserAutoReviewBlockedError(reason));
    };
    let fingerprint = fingerprint_sand_browser_auto_review_target(&canonical);
    let action = canonical.get("exact_action").cloned().unwrap_or_else(|| json!({}));
    let summary = summarize_sand_browser_auto_review_action(&SandBrowserSummaryArgs {
        op: op.to_string(),
        element: action.get("element").and_then(Value::as_str).map(ToOwned::to_owned),
        target_page_url: initial_state.target_page_url.clone(),
        url: action.get("url").and_then(Value::as_str).map(ToOwned::to_owned),
        text: action.get("text").and_then(Value::as_str).map(ToOwned::to_owned),
        value: action.get("value").and_then(Value::as_str).map(ToOwned::to_owned),
        values: action
            .get("values")
            .and_then(Value::as_array)
            .map(|values| values.iter().filter_map(Value::as_str).map(ToOwned::to_owned).collect())
            .unwrap_or_default(),
        key: action.get("key").and_then(Value::as_str).map(ToOwned::to_owned),
        cdp_method: action.get("cdpMethod").and_then(Value::as_str).map(ToOwned::to_owned),
        cdp_params: action.get("cdpParams").and_then(Value::as_str).map(ToOwned::to_owned),
        tabs_action: action.get("tabsAction").and_then(Value::as_str).map(ToOwned::to_owned),
        tab_index: action.get("tabIndex").and_then(Value::as_u64).and_then(|value| usize::try_from(value).ok()),
        file_path: action.get("filePath").and_then(Value::as_str).map(ToOwned::to_owned),
    });
    let request = SandAutoReviewRequest {
        agent_id: Some(agent_id.to_string()),
        surface: SandAutoReviewSurface::Computer,
        fingerprint,
        reason: reason.clone(),
        summary,
        command: None,
        proposed_rule,
        expiry_policy: Some(sand_auto_review_approval_expiry_policy(request_source)),
    };
    let decision = match controller.request_approval(request) {
        SandAutoReviewRequestOutcome::Immediate(decision) => decision,
        SandAutoReviewRequestOutcome::Pending(pending) => pending
            .wait()
            .map_err(|error| SandBrowserAutoReviewBlockedError(error.to_string()))?,
    };
    match decision {
        SandAutoReviewDecision::Denied { reason } => Err(SandBrowserAutoReviewBlockedError(reason)),
        SandAutoReviewDecision::Approved => {
            let next = capture_review_state().map_err(SandBrowserAutoReviewBlockedError)?;
            if next.display_state_identity != initial_state.display_state_identity {
                controller.report_display_recheck_failed(Some(agent_id));
                return Err(SandBrowserAutoReviewBlockedError(
                    "The page changed after review; take a fresh browser_snapshot and retry the action.".into(),
                ));
            }
            Ok(())
        }
    }
}

fn bounded(value: Option<&str>, field: &str, max: usize) -> Result<(), SandBrowserAutoReviewBlockedError> {
    if value.is_some_and(|value| value.chars().count() > max) {
        Err(reject(field, max))
    } else {
        Ok(())
    }
}

fn reject(field: &str, max: usize) -> SandBrowserAutoReviewBlockedError {
    SandBrowserAutoReviewBlockedError(format!(
        "Browser Auto-review rejected oversized {field} (max {max} characters)."
    ))
}
