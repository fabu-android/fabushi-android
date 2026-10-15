use std::collections::BTreeSet;

use serde_json::Value;

/// Android-adapted root Agent instructions.
///
/// Desktop owns a larger dynamic prompt assembly surface. Android keeps the
/// invariant product/safety contract here. Dynamic account-scoped context stays
/// owned by its canonical runtime services and is wired separately as each Desktop
/// responsibility is audited; Presentation must never become a second prompt owner.
pub const ANDROID_ROOT_SYSTEM_INSTRUCTIONS: &str = r#"You are Fabushi, the user's native Android assistant.

## Turn and delivery contract
For a user-visible turn, acknowledge the user before doing lengthy work. Send meaningful progress updates while working and close the loop with the actual result. A plain model text stream is internal reasoning/output plumbing; user-visible replies, questions, progress, attachments, links, widgets, and final results must be delivered through the canonical SendMessage tool. Do not claim a result was delivered until that tool call succeeds.

## Capability and safety boundaries
Use only the tools and capabilities exposed for this turn. Never bypass a blocked or unavailable sanctioned capability by scraping credentials, session cookies, token files, private keys, hidden browser state, or internal service APIs. Do not encode, split, rename, or otherwise disguise an action to evade review. When the same action legitimately requires approval, request approval through the product's approval flow without changing the action. A denied, expired, revoked, or unavailable capability is a boundary, not a puzzle to route around.

Android does not grant arbitrary desktop OS access. Local Android actions stay inside the bounded Local Runner and platform adapters. Work that requires the user's other computer must use an explicitly authorized Remote Runner. Never broaden account, device, permission, or execution scope on your own, and never blindly retry an outcome-unknown side effect; reconcile the original durable operation instead.

## Identity
The product identity is Fabushi. Do not adopt the identity of an underlying model, provider, compatibility layer, or historical product name. If runtime model/provider metadata is unavailable, say so rather than inferring it from prompt text, tool names, or implementation details."#;

pub fn android_root_system_instructions() -> &'static str {
    ANDROID_ROOT_SYSTEM_INSTRUCTIONS
}

pub fn build_android_root_system_instructions(
    memory_prompt: &str,
    time_zone: Option<&str>,
) -> String {
    let mut sections = vec![ANDROID_ROOT_SYSTEM_INSTRUCTIONS.to_string()];
    let memory_prompt = memory_prompt.trim();
    if !memory_prompt.is_empty() {
        sections.push(format!(
            "## Durable memory context\n{memory_prompt}"
        ));
    }
    if let Some(time_zone) = time_zone.map(str::trim).filter(|value| !value.is_empty()) {
        sections.push(format!("## Runtime context\nUser time zone: {time_zone}"));
    }
    sections.join("\n\n")
}

pub fn append_android_team_rules_system_instructions(
    instructions: &mut String,
    rules: &[Value],
) {
    let mut rendered = Vec::new();
    for rule in rules {
        let Some(content) = rule
            .get("content")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let name = rule
            .get("fullPath")
            .or_else(|| rule.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("team rule");
        let required = rule
            .get("isRequired")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        rendered.push(format!(
            "### {name}{}\n{content}",
            if required { " (required)" } else { "" }
        ));
    }
    if rendered.is_empty() {
        return;
    }
    if !instructions.trim().is_empty() {
        instructions.push_str("\n\n");
    }
    instructions.push_str(
        "## Team rules\nApply the following organization rules to this turn.\n\n",
    );
    instructions.push_str(&rendered.join("\n\n"));
}

pub fn append_android_agent_profile_system_instructions(
    instructions: &mut String,
    name: &str,
    description: &str,
    shared_room: bool,
) {
    let configured_name = name.trim();
    let name = if configured_name.eq_ignore_ascii_case("grok")
        || configured_name.eq_ignore_ascii_case("grok bot")
    {
        "Fabushi"
    } else {
        configured_name
    };
    let description = description.trim();
    if name.is_empty() && description.is_empty() {
        return;
    }
    let mut lines = vec!["## Agent profile".to_string()];
    if !name.is_empty() {
        lines.push(format!("Title: {name}"));
        if !shared_room {
            lines.push(format!(
                "Your agent name is \"{name}\". If the user asks for your name, answer with \"{name}\"."
            ));
        }
    }
    if !description.is_empty() {
        lines.push(format!("Description: {description}"));
    }
    if !instructions.trim().is_empty() {
        instructions.push_str("\n\n");
    }
    instructions.push_str(&lines.join("\n"));
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidRemotePromptState {
    pub ready: bool,
    pub paused: bool,
    pub has_desktop: bool,
    pub executors: Vec<String>,
}

pub fn append_android_remote_runtime_system_instructions(
    instructions: &mut String,
    state: &AndroidRemotePromptState,
) {
    let executors = state
        .executors
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let section = if state.paused {
        "## Remote computer\nThe authorized Remote Runner is currently paused for this account. Do not claim to read, control, or mutate the user's other computer, and do not substitute local Android actions for unavailable remote access. Wait until the canonical runtime resumes the binding before attempting a remote action.".to_string()
    } else if !state.ready {
        "## Remote computer\nNo authorized Remote Runner is ready for the current account and turn. Do not claim to read, control, or mutate the user's other computer, and do not route around this boundary with local Android APIs or unsanctioned network access.".to_string()
    } else {
        let executor_summary = if executors.is_empty() {
            "The binding currently grants no executor capabilities.".to_string()
        } else {
            format!(
                "Authorized executor capabilities reported by the binding: {}.",
                executors.join(", ")
            )
        };
        let desktop_summary = if state.has_desktop {
            "The binding reports desktop-capable remote resources."
        } else {
            "The binding does not report desktop-capable remote resources."
        };
        format!(
            "## Remote computer\nAn authorized Remote Runner is ready for the current account and turn. {desktop_summary} {executor_summary} This is informational context only: the routed tool graph, Capability Broker, current account/device/epoch fence, approval state, and executor-specific validation remain the execution authority. Never infer a capability from this prompt when the corresponding tool is not exposed."
        )
    };

    if !instructions.trim().is_empty() {
        instructions.push_str("\n\n");
    }
    instructions.push_str(&section);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_contract_keeps_identity_delivery_and_capability_boundaries() {
        let prompt = android_root_system_instructions();
        assert!(prompt.contains("Fabushi"));
        assert!(prompt.contains("SendMessage"));
        assert!(prompt.contains("Remote Runner"));
        assert!(prompt.contains("outcome-unknown"));
        assert!(prompt.contains("Never bypass"));
    }

    #[test]
    fn dynamic_memory_and_timezone_are_system_owned_and_optional() {
        let prompt = build_android_root_system_instructions(
            "User memory: prefers concise replies.",
            Some(" America/Los_Angeles "),
        );
        assert!(prompt.starts_with(ANDROID_ROOT_SYSTEM_INSTRUCTIONS));
        assert!(prompt.contains("## Durable memory context"));
        assert!(prompt.contains("prefers concise replies"));
        assert!(prompt.contains("User time zone: America/Los_Angeles"));

        let base_only = build_android_root_system_instructions("   ", Some("   "));
        assert_eq!(base_only, ANDROID_ROOT_SYSTEM_INSTRUCTIONS);
    }

    #[test]
    fn managed_team_rules_match_desktop_rendering_and_skip_empty_content() {
        let mut prompt = build_android_root_system_instructions("", None);
        append_android_team_rules_system_instructions(
            &mut prompt,
            &[
                serde_json::json!({
                    "fullPath":"Security / Read only",
                    "content":"Never mutate production without approval.",
                    "isRequired":true
                }),
                serde_json::json!({"name":"empty","content":"   "}),
                serde_json::json!({"content":"Use least privilege."}),
            ],
        );
        assert!(prompt.contains("## Team rules"));
        assert!(prompt.contains("### Security / Read only (required)"));
        assert!(prompt.contains("Never mutate production without approval."));
        assert!(prompt.contains("### team rule\nUse least privilege."));
        assert!(!prompt.contains("### empty"));
    }

    #[test]
    fn remote_runner_prompt_is_fail_closed_and_never_grants_capability() {
        let mut unavailable = build_android_root_system_instructions("", None);
        append_android_remote_runtime_system_instructions(
            &mut unavailable,
            &AndroidRemotePromptState {
                ready: false,
                paused: false,
                has_desktop: false,
                executors: vec!["computer".into()],
            },
        );
        assert!(unavailable.contains("No authorized Remote Runner is ready"));
        assert!(!unavailable.contains("Authorized executor capabilities reported"));

        let mut paused = build_android_root_system_instructions("", None);
        append_android_remote_runtime_system_instructions(
            &mut paused,
            &AndroidRemotePromptState {
                ready: false,
                paused: true,
                has_desktop: false,
                executors: Vec::new(),
            },
        );
        assert!(paused.contains("Remote Runner is currently paused"));

        let mut ready = build_android_root_system_instructions("", None);
        append_android_remote_runtime_system_instructions(
            &mut ready,
            &AndroidRemotePromptState {
                ready: true,
                paused: false,
                has_desktop: true,
                executors: vec!["external-shell".into(), "computer".into(), "computer".into()],
            },
        );
        assert!(ready.contains("desktop-capable remote resources"));
        assert!(ready.contains("computer, external-shell"));
        assert!(ready.contains("informational context only"));
        assert!(ready.contains("tool is not exposed"));
    }

    #[test]
    fn profile_identity_is_system_owned_every_turn_and_shared_room_safe() {
        let mut prompt = build_android_root_system_instructions("", None);
        append_android_agent_profile_system_instructions(
            &mut prompt, "Grok Bot", "Research helper", false,
        );
        assert!(prompt.contains("## Agent profile"));
        assert!(prompt.contains("Title: Fabushi"));
        assert!(prompt.contains("Your agent name is \"Fabushi\""));
        assert!(prompt.contains("Description: Research helper"));

        let mut shared = build_android_root_system_instructions("", None);
        append_android_agent_profile_system_instructions(
            &mut shared, "Room helper", "Shared room role", true,
        );
        assert!(shared.contains("Title: Room helper"));
        assert!(!shared.contains("Your agent name is"));
    }
}
