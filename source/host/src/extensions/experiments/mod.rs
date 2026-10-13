pub mod extension;
pub mod model_selection;

pub use extension::{
    HostExperimentsExtension, HostExperimentsOptions, SAND_PRODUCT_FEATURE_GATE_DEFAULTS_ENV, SAND_AUTOMATIONS_MODEL_CONFIG_NAME, SAND_BROWSER_USE_CONFIG_NAME,
    SAND_COMPUTER_USE_CONFIG_NAME, SAND_DEFAULT_MODEL_CONFIG_NAME, SAND_MODEL_EXPERIMENT_NAME,
};

pub use model_selection::{configured_model_from_config, read_sand_model_experiment_process_override, select_sand_experiment_turn_model, SandAgentModelParameter, SandAgentModelSelection, SandModelExperimentArm, SandModelExperimentState};
