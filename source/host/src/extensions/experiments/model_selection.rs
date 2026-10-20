use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandAgentModelParameter {
    pub id: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandAgentModelSelection {
    pub model_id: String,
    pub max_mode: bool,
    pub parameters: Vec<SandAgentModelParameter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandModelExperimentArm {
    Control,
    Treatment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SandModelExperimentState {
    pub active: bool,
    pub arm: SandModelExperimentArm,
}

pub const SAND_AUTOMATION_REQUEST_SOURCE: &str = "automation";

pub fn read_sand_model_experiment_env_override(raw: Option<&str>) -> Option<SandModelExperimentState> {
    match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("control") => Some(SandModelExperimentState { active: true, arm: SandModelExperimentArm::Control }),
        Some("treatment") | Some("test") => Some(SandModelExperimentState { active: true, arm: SandModelExperimentArm::Treatment }),
        _ => None,
    }
}

pub fn read_sand_model_experiment_process_override() -> Option<SandModelExperimentState> {
    let raw = std::env::var("SAND_MODEL_EXPERIMENT_OVERRIDE").ok();
    read_sand_model_experiment_env_override(raw.as_deref())
}

pub fn sand_model_experiment_opus_medium_selection() -> SandAgentModelSelection {
    SandAgentModelSelection {
        model_id: "claude-opus-4-8".into(),
        max_mode: true,
        parameters: vec![
            SandAgentModelParameter { id: "thinking".into(), value: "true".into() },
            SandAgentModelParameter { id: "context".into(), value: "1m".into() },
            SandAgentModelParameter { id: "effort".into(), value: "medium".into() },
            SandAgentModelParameter { id: "fast".into(), value: "false".into() },
        ],
    }
}

pub fn configured_model_from_config(config: BTreeMap<String, Value>, authenticated: bool) -> Option<SandAgentModelSelection> {
    if !authenticated { return None; }
    let model_id = config.get("modelId")?.as_str()?.to_string();
    let max_mode = config.get("maxMode")?.as_bool()?;
    let raw_parameters = config.get("parameters")?.as_array()?;
    if model_id.is_empty() || model_id.len() > 128 || model_id.chars().any(|c| c.is_whitespace() || c.is_control()) || raw_parameters.len() > 16 {
        return None;
    }
    let mut parameter_ids = BTreeSet::new();
    let mut parameters = Vec::with_capacity(raw_parameters.len());
    for raw in raw_parameters {
        let raw = raw.as_object()?;
        let id = raw.get("id")?.as_str()?;
        let value = raw.get("value")?.as_str()?;
        if id.is_empty() || value.len() > 64 || !parameter_ids.insert(id.to_string()) { return None; }
        parameters.push(SandAgentModelParameter { id: id.to_string(), value: value.to_string() });
    }
    let routed = matches!(model_id.as_str(), "default" | "premium" | "auto-low" | "auto-medium" | "auto-high" | "auto-smart");
    if routed && (max_mode || !parameters.is_empty()) { return None; }
    Some(SandAgentModelSelection { model_id, max_mode, parameters })
}

pub fn select_sand_model_experiment_model(state: Option<&SandModelExperimentState>, configured: Option<SandAgentModelSelection>) -> Option<SandAgentModelSelection> {
    let state = state?;
    if !state.active { return None; }
    if state.arm == SandModelExperimentArm::Control { return Some(sand_model_experiment_opus_medium_selection()); }
    Some(configured.unwrap_or_else(sand_model_experiment_opus_medium_selection))
}

pub fn select_sand_experiment_turn_model<DefaultModel, AutomationsModel>(
    state: Option<&SandModelExperimentState>,
    request_source: Option<&str>,
    mut read_default: DefaultModel,
    mut read_automations: AutomationsModel,
) -> Option<SandAgentModelSelection>
where
    DefaultModel: FnMut() -> Option<SandAgentModelSelection>,
    AutomationsModel: FnMut() -> Option<SandAgentModelSelection>,
{
    let state = state?;
    if !state.active || state.arm == SandModelExperimentArm::Control {
        return select_sand_model_experiment_model(Some(state), None);
    }
    let configured = if request_source == Some(SAND_AUTOMATION_REQUEST_SOURCE) {
        read_automations().or_else(&mut read_default)
    } else {
        read_default()
    };
    select_sand_model_experiment_model(Some(state), configured)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_is_fixed_opus_medium_and_treatment_uses_request_source_config() {
        let control = SandModelExperimentState { active: true, arm: SandModelExperimentArm::Control };
        let selected = select_sand_experiment_turn_model(Some(&control), None, || None, || None).unwrap();
        assert_eq!(selected.model_id, "claude-opus-4-8");
        assert!(selected.max_mode);
        assert!(selected.parameters.iter().any(|p| p.id == "effort" && p.value == "medium"));

        let treatment = SandModelExperimentState { active: true, arm: SandModelExperimentArm::Treatment };
        let selected = select_sand_experiment_turn_model(
            Some(&treatment), Some("automation"),
            || Some(SandAgentModelSelection { model_id: "default-model".into(), max_mode: false, parameters: vec![] }),
            || Some(SandAgentModelSelection { model_id: "automation-model".into(), max_mode: false, parameters: vec![] }),
        ).unwrap();
        assert_eq!(selected.model_id, "automation-model");
    }

    #[test]
    fn configured_model_validation_rejects_routed_model_parameters_and_duplicates() {
        let invalid = BTreeMap::from([
            ("modelId".into(), serde_json::json!("default")),
            ("maxMode".into(), serde_json::json!(true)),
            ("parameters".into(), serde_json::json!([])),
        ]);
        assert!(configured_model_from_config(invalid, true).is_none());
        assert!(configured_model_from_config(BTreeMap::new(), false).is_none());
    }
}
