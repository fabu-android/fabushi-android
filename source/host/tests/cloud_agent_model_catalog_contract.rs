use std::fs;
use std::path::PathBuf;

fn source_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("host crate must live below source/")
        .to_path_buf()
}

fn source(path: &str) -> String {
    fs::read_to_string(source_root().join(path)).expect("shipping source must be readable")
}

#[test]
fn runner_wire_reuses_the_single_desktop_compatible_available_models_codec() {
    let shared_wire = source("shared/src/cloud_agent_model_catalog_wire.rs");
    let runner_wire = source("box-exec-daemon/src/cloud_agent_wire.rs");

    assert!(runner_wire.contains(
        "pub use fabushi_android_shared::cloud_agent_model_catalog_wire"
    ));
    assert!(!runner_wire.contains("struct AvailableModels"));
    for needle in [
        "AVAILABLE_MODELS_SCOPE_USER_AVAILABLE",
        "encode_cloud_agent_available_models_request",
        "decode_cloud_agent_available_models_response",
        "(17, 2)",
        "(29, 2)",
        "(30, 2)",
        "(37, 2)",
        "vec![0x28, 0x01, 0x38, 0x01, 0x50, 0x01]",
    ] {
        assert!(shared_wire.contains(needle), "canonical model wire must contain {needle}");
    }
}

#[test]
fn production_models_action_and_launch_validation_share_catalog_truth() {
    let production = source("box-exec-daemon/src/cloud_agent_production.rs");
    let backend = source("box-exec-daemon/src/cloud_agent_backend.rs");
    let catalog = source("shared/src/model_catalog.rs");

    for needle in [
        "/aiserver.v1.AiService/AvailableModels",
        "&encode_cloud_agent_available_models_request()",
        "decode_cloud_agent_available_models_response",
        "CloudAgentAction::Models =>",
        "models.iter().map(model_json)",
        "validate_model_selection",
        "self.validate_model_selection(identity, request)?",
        "aliases",
        "variants",
    ] {
        assert!(production.contains(needle), "production CloudAgent owner must contain {needle}");
    }
    assert!(backend.contains("CloudAgentAction::Models"));
    assert!(backend.contains("model_params"));
    assert!(backend.contains("model is required when model_params are provided"));
    assert!(catalog.contains("entry.aliases.iter().any"));
    assert!(catalog.contains("validate_model_params"));
    assert!(catalog.contains("validate_param_combination"));
}

#[test]
fn catalog_wire_remains_runner_owned_and_not_a_ui_or_kotlin_truth() {
    let production = source("box-exec-daemon/src/cloud_agent_production.rs");
    let runner_wire = source("box-exec-daemon/src/cloud_agent_wire.rs");
    assert!(production.contains("use crate::cloud_agent_wire::*;"));
    assert!(!production.contains("fabushi_android_shared::cloud_agent_model_catalog_wire::{"));
    assert!(runner_wire.contains("fabushi_android_shared::cloud_agent_model_catalog_wire"));
}
