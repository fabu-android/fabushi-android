use std::collections::BTreeSet;

use serde_json::{json, Value};

pub const GATEWAY_PREPARE_UPGRADE_PATH: &str = "/prepare-upgrade";

pub use crate::host_gateway_api::FROZEN_HOST_GATEWAY_METHODS as GROK_GATEWAY_COMMANDS;
use crate::host_gateway_api::host_gateway_owner;

pub const FABUSHI_GATEWAY_COMPAT_COMMANDS: &[&str] = &["resumeAfterRecreate"];

pub fn is_grok_gateway_command(method: &str) -> bool {
    host_gateway_owner(method).is_some() || FABUSHI_GATEWAY_COMPAT_COMMANDS.contains(&method)
}

/// Backward-compatible Android helper now derived from the canonical registry instead of a
/// second hand-maintained command table.
pub fn sand_gateway_commands() -> BTreeSet<&'static str> {
    GROK_GATEWAY_COMMANDS
        .iter()
        .copied()
        .chain(FABUSHI_GATEWAY_COMPAT_COMMANDS.iter().copied())
        .collect()
}

pub fn is_known_gateway_command(method: &str) -> bool {
    is_grok_gateway_command(method)
}

pub fn parse_command_args(body: &[u8]) -> Result<Value, serde_json::Error> {
    if body.is_empty() { Ok(json!({})) } else { serde_json::from_slice(body) }
}

pub fn validate_command_payload(method: &str, payload: &str) -> Result<(), &'static str> {
    if !is_known_gateway_command(method) {
        return Err("unknown gateway command");
    }
    match serde_json::from_str::<Value>(payload) {
        Ok(Value::Object(_)) => Ok(()),
        Ok(_) => Err("gateway command payload must be a JSON object"),
        Err(_) => Err("gateway command payload is invalid JSON"),
    }
}

pub fn slim_command_result(method: &str, mut value: Value) -> Value {
    fn strip_summary(summary: &mut Value) {
        if let Some(object) = summary.as_object_mut() {
            if object.contains_key("avatarDataUrl") {
                object.insert("avatarDataUrl".into(), Value::Null);
            }
        }
    }
    match method {
        "listAgents" => {
            if let Some(rows) = value.as_array_mut() {
                for row in rows { strip_summary(row); }
            }
        }
        "updateAgent" | "setGroupMembers" | "setAgentAvatarBytes" => strip_summary(&mut value),
        "createAgent" | "createGroup" | "duplicateAgent" => {
            if let Some(agent) = value.get_mut("agent") { strip_summary(agent); }
        }
        _ => {}
    }
    value
}

pub fn slim_event(mut event: Value) -> Value {
    match event.get("channel").and_then(Value::as_str) {
        Some("agents") => {
            if let Some(rows) = event.get_mut("payload")
                .and_then(|p| p.get_mut("agents"))
                .and_then(Value::as_array_mut)
            {
                for row in rows {
                    if let Some(object) = row.as_object_mut() {
                        if object.contains_key("avatarDataUrl") {
                            object.insert("avatarDataUrl".into(), Value::Null);
                        }
                    }
                }
            }
        }
        Some("agent-upserted") => {
            if let Some(object) = event.get_mut("payload")
                .and_then(|p| p.get_mut("agent"))
                .and_then(Value::as_object_mut)
            {
                if object.contains_key("avatarDataUrl") {
                    object.insert("avatarDataUrl".into(), Value::Null);
                }
            }
        }
        _ => {}
    }
    event
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_registry_exposes_methods_omitted_by_legacy_android_table() {
        assert!(is_known_gateway_command("respondToRoomJoinRequest"));
        assert!(is_known_gateway_command("requestWebAuthnCeremony"));
        assert!(is_known_gateway_command("resumeAfterRecreate"));
        assert!(!is_known_gateway_command("definitelyUnknown"));
    }

    #[test]
    fn payload_validation_parses_real_json_objects() {
        assert!(validate_command_payload("sendPrompt", r#"{"text":"{still valid}"}"#).is_ok());
        assert_eq!(
            validate_command_payload("sendPrompt", "{not-json}"),
            Err("gateway command payload is invalid JSON")
        );
        assert_eq!(
            validate_command_payload("sendPrompt", "[]"),
            Err("gateway command payload must be a JSON object")
        );
    }

    #[test]
    fn avatar_payloads_are_slimmed_without_changing_other_fields() {
        let value = slim_command_result(
            "listAgents",
            json!([{"id":"a","avatarDataUrl":"data:image/png;base64,AA=="}]),
        );
        assert_eq!(value[0]["id"], "a");
        assert!(value[0]["avatarDataUrl"].is_null());

        let event = slim_event(json!({
            "channel":"agent-upserted",
            "payload":{"agent":{"id":"a","avatarDataUrl":"data:image/png;base64,AA=="}}
        }));
        assert!(event["payload"]["agent"]["avatarDataUrl"].is_null());
    }
}
