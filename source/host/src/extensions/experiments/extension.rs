use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;

pub const SAND_MODEL_EXPERIMENT_NAME: &str = "sand_model_selection";
pub const SAND_DEFAULT_MODEL_CONFIG_NAME: &str = "sand_default_model";
pub const SAND_AUTOMATIONS_MODEL_CONFIG_NAME: &str = "sand_automations_model";
pub const SAND_COMPUTER_USE_CONFIG_NAME: &str = "sand_computer_use_playwright_config";
pub const SAND_BROWSER_USE_CONFIG_NAME: &str = "sand_browser_use_model";
pub const SAND_PRODUCT_FEATURE_GATE_DEFAULTS_ENV: &str = "SAND_PRODUCT_FEATURE_GATE_DEFAULTS";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostExperimentsOptions {
    pub is_dev_build: bool,
    pub env_gate_overrides: Option<String>,
}

impl HostExperimentsOptions {
    pub fn from_process_env() -> Self {
        let is_dev_build = std::env::var("SAND_PACKAGED").as_deref() != Ok("1")
            || std::env::var("SAND_HOST_DEV_ERROR_DETAIL").as_deref() == Ok("1");
        Self {
            is_dev_build,
            env_gate_overrides: std::env::var("SAND_FEATURE_GATE_OVERRIDES")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct ExperimentsState {
    options: HostExperimentsOptions,
    settings_overrides: BTreeMap<String, bool>,
    network_feature_gates: BTreeMap<String, bool>,
    network_dynamic_configs: BTreeMap<String, BTreeMap<String, Value>>,
    hydrated_user_id: Option<String>,
    authenticated_bootstrap: bool,
}

/// Canonical Android Host owner for experiment/gate state.
///
/// Network transport is intentionally outside this foundation. A future authenticated Statsig
/// bootstrap adapter must hydrate this owner rather than giving Presentation or a second runtime
/// independent feature-gate truth.
pub struct HostExperimentsExtension {
    state: Mutex<ExperimentsState>,
}

impl Default for HostExperimentsExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl HostExperimentsExtension {
    pub fn new() -> Self {
        Self::with_options(HostExperimentsOptions::from_process_env())
    }

    pub fn with_options(options: HostExperimentsOptions) -> Self {
        Self {
            state: Mutex::new(ExperimentsState {
                options,
                ..ExperimentsState::default()
            }),
        }
    }

    pub fn check_feature_gate(&self, name: &str) -> bool {
        let state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(value) = state.settings_overrides.get(name).copied() {
            return value;
        }
        if state.options.is_dev_build {
            if let Some(value) = env_gate_override(name, state.options.env_gate_overrides.as_deref()) {
                return value;
            }
        }
        if state.authenticated_bootstrap {
            if let Some(value) = state.network_feature_gates.get(name).copied() {
                return value;
            }
        }
        if let Some(value) = std::env::var(SAND_PRODUCT_FEATURE_GATE_DEFAULTS_ENV)
            .ok()
            .as_deref()
            .and_then(|raw| env_gate_override(name, Some(raw)))
        {
            return value;
        }
        bundled_feature_gate_default(name)
    }

    pub fn dynamic_config(&self, name: &str) -> BTreeMap<String, Value> {
        let state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.authenticated_bootstrap {
            if let Some(config) = state.network_dynamic_configs.get(name) {
                return config.clone();
            }
        }
        bundled_dynamic_config_default(name)
    }

    pub fn set_settings_override(&self, name: impl Into<String>, value: Option<bool>) {
        let name = name.into();
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match value {
            Some(value) => {
                state.settings_overrides.insert(name, value);
            }
            None => {
                state.settings_overrides.remove(&name);
            }
        }
    }

    /// Atomically accepts a bootstrap only when it carries a non-empty authenticated user id.
    /// Invalid/anonymous payloads leave the previous canonical state untouched.
    pub fn hydrate_authenticated_statsig(&self, config: &str) -> Result<(), String> {
        let parsed = parse_statsig_bootstrap(config)?;
        let user_id = parsed
            .hydrated_user_id
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Statsig bootstrap has no authenticated user id".to_string())?;
        let mut state = self.state.lock().map_err(|_| "experiments state lock poisoned".to_string())?;
        state.network_feature_gates = parsed.feature_gates;
        state.network_dynamic_configs = parsed.dynamic_configs;
        state.hydrated_user_id = Some(user_id);
        state.authenticated_bootstrap = true;
        Ok(())
    }

    pub fn has_authenticated_bootstrap(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .authenticated_bootstrap
    }

    pub fn hydrated_user_id(&self) -> Option<String> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .hydrated_user_id
            .clone()
    }

    /// Account switch/logout must fence all account-scoped experiment state. Settings overrides
    /// are installation-local and intentionally survive.
    pub fn clear_account_scope(&self) {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.network_feature_gates.clear();
        state.network_dynamic_configs.clear();
        state.hydrated_user_id = None;
        state.authenticated_bootstrap = false;
    }
}

struct ParsedBootstrap {
    hydrated_user_id: Option<String>,
    feature_gates: BTreeMap<String, bool>,
    dynamic_configs: BTreeMap<String, BTreeMap<String, Value>>,
}

fn parse_statsig_bootstrap(config: &str) -> Result<ParsedBootstrap, String> {
    let parsed: Value =
        serde_json::from_str(config).map_err(|error| format!("invalid Statsig bootstrap: {error}"))?;
    let hydrated_user_id = parsed
        .get("user")
        .and_then(Value::as_object)
        .and_then(|user| user.get("userID"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let feature_gates = parsed
        .get("feature_gates")
        .and_then(Value::as_object)
        .map(|gates| {
            gates
                .iter()
                .filter_map(|(name, raw)| statsig_gate_value(raw).map(|value| (name.clone(), value)))
                .collect()
        })
        .unwrap_or_default();

    let dynamic_configs = parsed
        .get("dynamic_configs")
        .and_then(Value::as_object)
        .map(|configs| {
            configs
                .iter()
                .filter_map(|(name, raw)| {
                    let values = raw.get("value")?.as_object()?;
                    Some((
                        name.clone(),
                        values
                            .iter()
                            .map(|(key, value)| (key.clone(), value.clone()))
                            .collect(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(ParsedBootstrap {
        hydrated_user_id,
        feature_gates,
        dynamic_configs,
    })
}

fn statsig_gate_value(value: &Value) -> Option<bool> {
    value.as_bool().or_else(|| {
        value.get("value").and_then(|value| {
            value
                .as_bool()
                .or_else(|| value.get("value").and_then(Value::as_bool))
        })
    })
}

fn env_gate_override(name: &str, raw: Option<&str>) -> Option<bool> {
    let raw = raw?;
    for pair in raw.split(',') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or("").trim();
        let value = parts.next().unwrap_or("").trim();
        if key == name {
            return Some(matches!(value, "1" | "true" | "on"));
        }
    }
    None
}

fn bundled_feature_gate_default(name: &str) -> bool {
    matches!(
        name,
        "enable_sparse_plugin_clones"
            | "sand_multitask"
            | "sand_spotlight"
            | "sand_computer_use_playwright"
    )
}

fn bundled_dynamic_config_default(name: &str) -> BTreeMap<String, Value> {
    match name {
        "grok_bot_conversation_size_limits" => BTreeMap::from([
            ("soft_limit_mb".into(), serde_json::json!(256)),
            ("hard_limit_mb".into(), serde_json::json!(1024)),
        ]),
        SAND_DEFAULT_MODEL_CONFIG_NAME | SAND_AUTOMATIONS_MODEL_CONFIG_NAME => BTreeMap::from([
            ("modelId".into(), serde_json::json!("default")),
            ("maxMode".into(), serde_json::json!(false)),
            ("parameters".into(), serde_json::json!([])),
        ]),
        SAND_COMPUTER_USE_CONFIG_NAME | SAND_BROWSER_USE_CONFIG_NAME => BTreeMap::from([
            ("modelId".into(), serde_json::json!("claude-opus-4-8")),
            ("maxMode".into(), serde_json::json!(false)),
            (
                "parameters".into(),
                serde_json::json!([
                    {"id":"thinking","value":"false"},
                    {"id":"effort","value":"low"}
                ]),
            ),
        ]),
        "sand_model_filter" => BTreeMap::from([
            ("allowedModelIds".into(), serde_json::json!([])),
            ("defaultParameters".into(), serde_json::json!({})),
        ]),
        _ => BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bootstrap(user_id: Option<&str>, gate: bool, model: &str) -> String {
        serde_json::json!({
            "user": user_id.map(|value| serde_json::json!({"userID":value})),
            "feature_gates": {
                "sand_multiplayer": {"value": gate}
            },
            "dynamic_configs": {
                SAND_DEFAULT_MODEL_CONFIG_NAME: {
                    "value": {"modelId":model,"maxMode":false,"parameters":[]}
                }
            }
        })
        .to_string()
    }

    #[test]
    fn dev_override_precedes_authenticated_network_gate() {
        let experiments = HostExperimentsExtension::with_options(HostExperimentsOptions {
            is_dev_build: true,
            env_gate_overrides: Some("sand_multiplayer=on".into()),
        });
        experiments
            .hydrate_authenticated_statsig(&bootstrap(Some("user-a"), false, "model-a"))
            .unwrap();
        assert!(experiments.check_feature_gate("sand_multiplayer"));
        experiments.set_settings_override("sand_multiplayer", Some(false));
        assert!(!experiments.check_feature_gate("sand_multiplayer"));
    }

    #[test]
    fn production_mode_ignores_dev_override_string() {
        let experiments = HostExperimentsExtension::with_options(HostExperimentsOptions {
            is_dev_build: false,
            env_gate_overrides: Some("sand_multiplayer=on".into()),
        });
        assert!(!experiments.check_feature_gate("sand_multiplayer"));
    }

    #[test]
    fn anonymous_bootstrap_cannot_replace_canonical_gate_state() {
        let experiments = HostExperimentsExtension::new();
        assert!(experiments.check_feature_gate("sand_multitask"));
        assert!(experiments
            .hydrate_authenticated_statsig(&bootstrap(None, false, "remote"))
            .is_err());
        assert!(!experiments.has_authenticated_bootstrap());
        assert!(experiments.check_feature_gate("sand_multitask"));
    }

    #[test]
    fn authenticated_bootstrap_owns_network_gates_and_dynamic_config() {
        let experiments = HostExperimentsExtension::new();
        experiments
            .hydrate_authenticated_statsig(&bootstrap(Some("user-a"), false, "model-a"))
            .unwrap();
        assert!(experiments.has_authenticated_bootstrap());
        assert_eq!(experiments.hydrated_user_id().as_deref(), Some("user-a"));
        assert!(!experiments.check_feature_gate("sand_multiplayer"));
        assert_eq!(
            experiments
                .dynamic_config(SAND_DEFAULT_MODEL_CONFIG_NAME)
                .get("modelId")
                .and_then(Value::as_str),
            Some("model-a")
        );
    }

    #[test]
    fn account_switch_clears_network_truth_but_keeps_local_override() {
        let experiments = HostExperimentsExtension::new();
        experiments.set_settings_override("sand_multiplayer", Some(true));
        experiments
            .hydrate_authenticated_statsig(&bootstrap(Some("user-a"), false, "model-a"))
            .unwrap();
        experiments.clear_account_scope();
        assert!(!experiments.has_authenticated_bootstrap());
        assert_eq!(experiments.hydrated_user_id(), None);
        assert!(experiments.check_feature_gate("sand_multiplayer"));
        assert_eq!(
            experiments
                .dynamic_config(SAND_DEFAULT_MODEL_CONFIG_NAME)
                .get("modelId")
                .and_then(Value::as_str),
            Some("default")
        );
    }

    #[test]
    fn malformed_refresh_does_not_destroy_last_good_state() {
        let experiments = HostExperimentsExtension::new();
        experiments
            .hydrate_authenticated_statsig(&bootstrap(Some("user-a"), false, "model-a"))
            .unwrap();
        assert!(experiments.hydrate_authenticated_statsig("{").is_err());
        assert_eq!(experiments.hydrated_user_id().as_deref(), Some("user-a"));
        assert!(!experiments.check_feature_gate("sand_multiplayer"));
    }
}
