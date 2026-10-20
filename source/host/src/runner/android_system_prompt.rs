use std::collections::BTreeSet;

use fabushi_constants::cloud_agent::{CLOUD_AGENT_ARTIFACTS_DIR, ENV_SETUP_SKILL_ID};
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
    sections.push(render_android_walkthrough_artifacts_system_prompt(None));
    sections.join("\n\n")
}

pub fn resolve_android_prompt_artifacts_dir(reported: Option<&str>) -> String {
    let value = reported
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(CLOUD_AGENT_ARTIFACTS_DIR);
    if value.ends_with('/') || value.ends_with('\\') {
        value.to_string()
    } else if value.contains('\\') && !value.contains('/') {
        format!("{value}\\")
    } else {
        format!("{value}/")
    }
}

fn render_android_walkthrough_artifacts_system_prompt(
    reported_artifacts_dir: Option<&str>,
) -> String {
    let artifacts_dir = resolve_android_prompt_artifacts_dir(reported_artifacts_dir);
    let display_dir = artifacts_dir.trim_end_matches(|ch| ch == '/' || ch == '\\');
    format!(
        "## Walkthrough artifacts\nWhen testing or computer-use capabilities are exposed for this turn and a user-facing walkthrough is warranted, save the selected final screenshots or recordings under {display_dir}. This directory contract does not grant filesystem, computer-use, recording, or upload capability; use only capabilities exposed for this turn."
    )
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


const INITIAL_SKILL_CATALOG_CONTEXT_PERCENT: usize = 2;
const MIN_TRUNCATED_SKILL_DESCRIPTION_LENGTH: usize = 24;
const MAX_TRUNCATED_SKILL_DESCRIPTION_LENGTH: usize = 480;
const SHORT_SKILL_DESCRIPTION_PATH_ONLY_THRESHOLD: usize = 80;
const MAX_OMITTED_SKILL_DIRECTORY_COUNT: usize = 5;
const AGENT_DIRECTORY_PROMPT_LIMIT: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidAutomationPromptItem {
    pub id: String,
    pub name: String,
    pub schedule: String,
    pub trigger_description: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidSkillPromptItem {
    pub full_path: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidChannelPromptItem {
    pub platform: String,
    pub label: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidAgentPromptAddress {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_group: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidAgentPromptGroup {
    pub address: AndroidAgentPromptAddress,
    pub members: Vec<AndroidAgentPromptAddress>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidMcpPromptServer {
    pub name: String,
    pub connected: bool,
    pub custom_instructions: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AndroidDynamicPromptSnapshot {
    pub automations_location: Option<String>,
    pub automations: Vec<AndroidAutomationPromptItem>,
    pub workflows_location: Option<String>,
    pub skills: Vec<AndroidSkillPromptItem>,
    pub channels_location: Option<String>,
    pub channels: Vec<AndroidChannelPromptItem>,
    pub agents_location: Option<String>,
    pub other_agents: Vec<AndroidAgentPromptAddress>,
    pub groups: Vec<AndroidAgentPromptGroup>,
    pub mcp_servers: Vec<AndroidMcpPromptServer>,
    pub mcp_discovery_unavailable: bool,
}

pub fn ensure_android_prompt_account_snapshot_current(
    expected_account_fence: &str,
    current_account_fence: &str,
) -> Result<(), String> {
    if expected_account_fence == current_account_fence {
        Ok(())
    } else {
        Err("runtime prompt snapshot is stale after account/epoch transition".into())
    }
}

fn append_unique_android_system_section(
    instructions: &mut String,
    section: &str,
    marker: &str,
) {
    let section = section.trim();
    if section.is_empty() || instructions.contains(marker) {
        return;
    }
    if !instructions.trim().is_empty() {
        instructions.push_str("\n\n");
    }
    instructions.push_str(section);
}

fn render_android_automations_system_prompt(
    automations: &[AndroidAutomationPromptItem],
    location: Option<&str>,
    time_zone: Option<&str>,
) -> String {
    let Some(location) = location.map(str::trim).filter(|value| !value.is_empty()) else {
        return String::new();
    };
    let zone_note = time_zone
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|zone| format!("the user's local time ({zone})"))
        .unwrap_or_else(|| "the user's local time".to_string());
    let mut lines = vec![
        "## Routines".to_string(),
        "Routines are durable standing orders: a saved prompt plus either a schedule or an outside-event listener. They can run while the user is away.".to_string(),
        format!("Definitions are owned by the canonical Android Host store at {location}. Use the routine state tool when it is exposed for this turn; this prompt is context only and never grants mutation authority."),
        "Treat recurring, time-based, monitoring, reminder, digest, and \"tell me when\" requests as routine candidates. Create one when intent is unambiguous; otherwise propose it briefly instead of pretending to remain awake.".to_string(),
        "Write saved prompts as durable intent, not frozen tool-call arguments. Connector/MCP schemas may change between runs, so discover the current tool contract when a run fires.".to_string(),
        format!("Cron schedules use five fields (minute hour day-of-month month day-of-week) interpreted in {zone_note}. @hourly/@daily/@weekly/@monthly and @every intervals are supported; CRON_TZ=<IANA zone> pins a schedule to a fixed zone."),
        "A hidden [routine] wake is the routine firing, not a fresh user message. Carry out the saved prompt and surface useful results naturally; if the saved instruction explicitly says to stay quiet when nothing changed, silence is a valid outcome.".to_string(),
        "Never blindly retry an outcome-unknown routine side effect. Reconcile the durable operation first.".to_string(),
    ];
    if automations.is_empty() {
        lines.push("No routines yet.".into());
    } else {
        lines.push("Current routines:".into());
        for automation in automations {
            let state = if automation.enabled { "enabled" } else { "paused" };
            let schedule = if automation.schedule.trim().is_empty() {
                String::new()
            } else {
                format!(" ({})", automation.schedule.trim())
            };
            let trigger = if automation.trigger_description.trim().is_empty() {
                "Unknown trigger"
            } else {
                automation.trigger_description.trim()
            };
            lines.push(format!(
                "- {} [{}] — {}{}; id {}",
                automation.name.trim(),
                state,
                trigger,
                schedule,
                automation.id.trim(),
            ));
        }
    }
    lines.join("\n")
}

fn render_android_workflows_system_prompt(location: Option<&str>) -> String {
    let Some(location) = location.map(str::trim).filter(|value| !value.is_empty()) else {
        return String::new();
    };
    format!(
        "Workflows are a GLOBAL, shared library across all of the user's assistants. User-created skills live as files at {location}: one subfolder per workflow, each holding a SKILL.md. Prefer the canonical workflow state tool when it is exposed for this turn to save, rewrite, and delete them. This prompt describes state; it does not grant a tool or filesystem capability."
    )
}

fn estimate_prompt_tokens(value: &str) -> usize {
    (value.chars().count() + 2) / 4
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn skill_name(full_path: &str) -> String {
    let normalized = full_path.replace('\\', "/");
    let parts = normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.last().copied() == Some("SKILL.md") && parts.len() >= 2 {
        parts[parts.len() - 2].to_string()
    } else {
        parts.last().copied().unwrap_or("").to_string()
    }
}

fn is_protected_skill(item: &AndroidSkillPromptItem) -> bool {
    let name = skill_name(&item.full_path);
    name == "canvas" || name == ENV_SETUP_SKILL_ID
}

fn skill_directory_hint(full_path: &str) -> String {
    let normalized = full_path.replace('\\', "/");
    normalized
        .rsplit_once('/')
        .map(|(directory, _)| directory.to_string())
        .unwrap_or(normalized)
}

fn render_android_skill_section(
    items: &[AndroidSkillPromptItem],
    omitted: &[AndroidSkillPromptItem],
) -> String {
    let mut out = String::from(
        "<agent_skills>\nWhen the user names a skill, use it faithfully as part of the current task. Read the skill file using an exposed read capability before following its instructions. The user's instructions take precedence over skill guidance, and this catalog never grants filesystem access.\n<available_skills>\n",
    );
    for item in items {
        out.push_str("<agent_skill fullPath=\"");
        out.push_str(&xml_escape(&item.full_path));
        out.push_str("\">");
        if let Some(description) = item
            .description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            out.push_str(&xml_escape(description));
        }
        out.push_str("</agent_skill>\n");
    }
    if !omitted.is_empty() {
        let mut directories = Vec::<String>::new();
        for item in omitted {
            let directory = skill_directory_hint(&item.full_path);
            if !directories.contains(&directory) {
                directories.push(directory);
                if directories.len() == MAX_OMITTED_SKILL_DIRECTORY_COUNT {
                    break;
                }
            }
        }
        out.push_str(&format!(
            "Additional skills omitted from this initial list ({}). Directories containing omitted skills: {}.\n",
            omitted.len(),
            directories.join(", ")
        ));
    }
    out.push_str("</available_skills>\n</agent_skills>");
    out
}

fn truncate_skill_description(description: Option<&str>, max_length: usize) -> Option<String> {
    let description = description?;
    if description.chars().count() <= max_length {
        return Some(description.to_string());
    }
    let content_length = max_length.saturating_sub(3);
    Some(format!(
        "{}...",
        description
            .chars()
            .take(content_length)
            .collect::<String>()
            .trim_end()
    ))
}

pub fn render_budgeted_android_skills_prompt(
    skills: &[AndroidSkillPromptItem],
    agent_token_limit: usize,
) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut skills = skills.to_vec();
    skills.sort_by(|left, right| left.full_path.cmp(&right.full_path));
    skills.dedup_by(|left, right| left.full_path == right.full_path);

    let uncapped = render_android_skill_section(&skills, &[]);
    let budget_tokens = agent_token_limit
        .saturating_mul(INITIAL_SKILL_CATALOG_CONTEXT_PERCENT)
        / 100;
    if estimate_prompt_tokens(&uncapped) <= budget_tokens {
        return uncapped;
    }

    let max_description_length = skills
        .iter()
        .filter(|item| !is_protected_skill(item))
        .filter_map(|item| item.description.as_deref())
        .map(|description| description.chars().count())
        .max()
        .unwrap_or(0);
    if max_description_length > SHORT_SKILL_DESCRIPTION_PATH_ONLY_THRESHOLD {
        let mut lower = MIN_TRUNCATED_SKILL_DESCRIPTION_LENGTH;
        let mut upper = max_description_length
            .saturating_sub(1)
            .min(MAX_TRUNCATED_SKILL_DESCRIPTION_LENGTH);
        let mut best = None::<String>;
        while lower <= upper {
            let midpoint = lower + (upper - lower) / 2;
            let candidate = skills
                .iter()
                .map(|item| AndroidSkillPromptItem {
                    full_path: item.full_path.clone(),
                    description: if is_protected_skill(item) {
                        item.description.clone()
                    } else {
                        truncate_skill_description(item.description.as_deref(), midpoint)
                    },
                })
                .collect::<Vec<_>>();
            let prompt = render_android_skill_section(&candidate, &[]);
            if estimate_prompt_tokens(&prompt) <= budget_tokens {
                best = Some(prompt);
                lower = midpoint.saturating_add(1);
            } else if midpoint == 0 {
                break;
            } else {
                upper = midpoint - 1;
            }
        }
        if let Some(prompt) = best {
            return prompt;
        }
    }

    let path_only = skills
        .iter()
        .map(|item| AndroidSkillPromptItem {
            full_path: item.full_path.clone(),
            description: is_protected_skill(item)
                .then(|| item.description.clone())
                .flatten(),
        })
        .collect::<Vec<_>>();
    let path_only_prompt = render_android_skill_section(&path_only, &[]);
    if estimate_prompt_tokens(&path_only_prompt) <= budget_tokens {
        return path_only_prompt;
    }

    let droppable = skills
        .iter()
        .enumerate()
        .filter_map(|(index, item)| (!is_protected_skill(item)).then_some(index))
        .collect::<Vec<_>>();
    for retained_droppable_count in (0..=droppable.len()).rev() {
        let retained_indices = droppable[..retained_droppable_count]
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let mut retained = Vec::new();
        let mut omitted = Vec::new();
        for (index, original) in skills.iter().enumerate() {
            if is_protected_skill(original) || retained_indices.contains(&index) {
                retained.push(path_only[index].clone());
            } else {
                omitted.push(original.clone());
            }
        }
        let prompt = render_android_skill_section(&retained, &omitted);
        if estimate_prompt_tokens(&prompt) <= budget_tokens || retained_droppable_count == 0 {
            return prompt;
        }
    }
    unreachable!("skill omission loop must return")
}

fn channel_display_name(platform: &str) -> &str {
    match platform {
        "discord" => "Discord",
        "slack" => "Slack",
        other => other,
    }
}

fn render_android_channels_system_prompt(
    connections: &[AndroidChannelPromptItem],
    location: Option<&str>,
) -> String {
    let Some(location) = location.map(str::trim).filter(|value| !value.is_empty()) else {
        return String::new();
    };
    if connections.is_empty() {
        return String::new();
    }
    let mut connections = connections.to_vec();
    connections.sort_by(|left, right| {
        left.platform
            .cmp(&right.platform)
            .then_with(|| left.label.cmp(&right.label))
    });
    let mut lines = vec![
        "Channels: outside messaging surfaces you can talk on, beyond this Fabushi chat.".to_string(),
        format!("Canonical connection metadata is owned by the Android Host at {location}; credentials stay in the separate secret owner and are never prompt data."),
        "Never ask the user to paste a token, API key, or password into chat or into a readable file. Use only the product's masked secret/authorization flow when it is exposed.".to_string(),
        "Every channel conversation has an address shaped like platform:chat. Reply to an inbound channel on that same address only through the routed messaging tool exposed for the turn.".to_string(),
        "The list below is informational context. It never grants channel delivery capability and must not be used to bypass the tool graph or approval boundary.".to_string(),
        "Currently connected:".to_string(),
    ];
    for connection in connections {
        lines.push(format!(
            "- {} \"{}\" [{}]. Address people on it as {}:<chat id>",
            channel_display_name(connection.platform.trim()),
            connection.label.trim(),
            connection.status.trim(),
            connection.platform.trim(),
        ));
    }
    lines.join("\n")
}

fn describe_agent_address(address: &AndroidAgentPromptAddress) -> String {
    let description = address
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!(" — {}", value.chars().take(120).collect::<String>()))
        .unwrap_or_default();
    let group_tag = if address.is_group { " (group)" } else { "" };
    format!(
        "- {} (id: {}){}{}",
        address.name.trim(),
        address.id.trim(),
        group_tag,
        description
    )
}

fn render_android_agent_directory_system_prompt(
    others: &[AndroidAgentPromptAddress],
    groups: &[AndroidAgentPromptGroup],
    location: Option<&str>,
) -> String {
    let mut others = others.to_vec();
    others.sort_by(|left, right| left.id.cmp(&right.id));
    let mut groups = groups.to_vec();
    groups.sort_by(|left, right| left.address.id.cmp(&right.address.id));
    let mut lines = vec![
        "Your teammates: the other agents this user runs. Each is its own assistant with its own chat, persona, and memory; you can message any of them by id and they can message you back.".to_string(),
        "Messaging is asynchronous. Use SendToAgent only when that routed tool is exposed for this turn; this directory is context and never grants cross-agent messaging authority. Do not wait or poll for a reply in this turn.".to_string(),
        "Use inter-agent messaging deliberately and sparingly. Never relay the user's private or unfiltered words verbatim; paraphrase the actionable substance when relay is warranted.".to_string(),
    ];
    if let Some(location) = location.map(str::trim).filter(|value| !value.is_empty()) {
        lines.push(format!("The canonical Android Agent roster is owned at {location}; do not create a second roster from this prompt."));
    }
    if others.is_empty() && groups.is_empty() {
        lines.push("This user has no other agents currently addressable from this root Agent.".into());
        return lines.join("\n");
    }
    if !others.is_empty() {
        lines.push("Teammates you can address when the corresponding tool is exposed:".into());
        for address in others.iter().take(AGENT_DIRECTORY_PROMPT_LIMIT) {
            lines.push(describe_agent_address(address));
        }
        if others.len() > AGENT_DIRECTORY_PROMPT_LIMIT {
            lines.push("…and more in the canonical roster.".into());
        }
    }
    if !groups.is_empty() {
        lines.push("Group chats you're in:".into());
        for group in groups.iter().take(AGENT_DIRECTORY_PROMPT_LIMIT) {
            let member_names = group
                .members
                .iter()
                .map(|member| member.name.trim())
                .filter(|name| !name.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            let with_clause = if member_names.is_empty() {
                String::new()
            } else {
                format!(" — with {member_names}")
            };
            lines.push(format!(
                "- {} (id: {}){}",
                group.address.name.trim(),
                group.address.id.trim(),
                with_clause,
            ));
        }
    }
    lines.join("\n")
}

fn render_android_mcp_custom_instructions_system_prompt(
    servers: &[AndroidMcpPromptServer],
) -> String {
    let mut entries = servers
        .iter()
        .filter(|server| server.connected)
        .filter_map(|server| {
            let name = server.name.trim();
            let instructions = server.custom_instructions.trim();
            (!name.is_empty() && !instructions.is_empty())
                .then(|| (name.to_string(), instructions.to_string()))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries.dedup_by(|left, right| left.0 == right.0);
    if entries.is_empty() {
        return String::new();
    }
    let mut lines = vec![
        "## Connector custom instructions".to_string(),
        "Custom instructions are configured for some connected MCP tools. Follow the matching instruction when that connector's routed tool is actually exposed; this section never authorizes a connector or tool by itself:".to_string(),
    ];
    lines.extend(
        entries
            .into_iter()
            .map(|(name, instructions)| format!("- {name}: {instructions}")),
    );
    lines.join("\n")
}

fn render_android_mcp_discovery_status_system_prompt(discovery_unavailable: bool) -> String {
    if !discovery_unavailable {
        return String::new();
    }
    "<mcp_status>\nYour MCP tools are temporarily unavailable: discovering the user's MCP connectors from the backend failed this turn. This does NOT mean the user has no MCP connectors. Do not claim they have none or that a connector is missing; if the user needs an MCP tool, tell them MCP is temporarily unavailable and to retry shortly.\n</mcp_status>".to_string()
}

pub fn append_android_dynamic_runtime_system_instructions(
    instructions: &mut String,
    snapshot: &AndroidDynamicPromptSnapshot,
    time_zone: Option<&str>,
    agent_token_limit: usize,
) {
    let routines = render_android_automations_system_prompt(
        &snapshot.automations,
        snapshot.automations_location.as_deref(),
        time_zone,
    );
    append_unique_android_system_section(instructions, &routines, "## Routines");

    let workflows =
        render_android_workflows_system_prompt(snapshot.workflows_location.as_deref());
    append_unique_android_system_section(
        instructions,
        &workflows,
        "Workflows are a GLOBAL, shared library",
    );

    let skills = render_budgeted_android_skills_prompt(&snapshot.skills, agent_token_limit);
    append_unique_android_system_section(instructions, &skills, "<agent_skills>");

    let channels = render_android_channels_system_prompt(
        &snapshot.channels,
        snapshot.channels_location.as_deref(),
    );
    append_unique_android_system_section(
        instructions,
        &channels,
        "Channels: outside messaging surfaces",
    );

    let directory = render_android_agent_directory_system_prompt(
        &snapshot.other_agents,
        &snapshot.groups,
        snapshot.agents_location.as_deref(),
    );
    append_unique_android_system_section(
        instructions,
        &directory,
        "Your teammates: the other agents",
    );

    let mcp_custom =
        render_android_mcp_custom_instructions_system_prompt(&snapshot.mcp_servers);
    append_unique_android_system_section(
        instructions,
        &mcp_custom,
        "## Connector custom instructions",
    );

    let mcp_status =
        render_android_mcp_discovery_status_system_prompt(snapshot.mcp_discovery_unavailable);
    append_unique_android_system_section(instructions, &mcp_status, "<mcp_status>");
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
        assert!(base_only.starts_with(ANDROID_ROOT_SYSTEM_INSTRUCTIONS));
        assert!(base_only.contains("## Walkthrough artifacts"));
        assert!(base_only.contains("/opt/cursor/artifacts"));
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

    #[test]
    fn dynamic_runtime_sections_preserve_order_dedupe_and_fail_closed_mcp() {
        let mut prompt = build_android_root_system_instructions("", Some("America/Los_Angeles"));
        let snapshot = AndroidDynamicPromptSnapshot {
            automations_location: Some("/state/automation-runtime.json".into()),
            automations: vec![AndroidAutomationPromptItem {
                id: "daily".into(),
                name: "Daily digest".into(),
                schedule: "0 9 * * 1-5".into(),
                trigger_description: "Scheduled".into(),
                enabled: true,
            }],
            workflows_location: Some("/state/workflows".into()),
            skills: vec![AndroidSkillPromptItem {
                full_path: "/state/workflows/review/SKILL.md".into(),
                description: Some("Review changes.".into()),
            }],
            channels_location: Some("/state/channels".into()),
            channels: vec![AndroidChannelPromptItem {
                platform: "slack".into(),
                label: "Ops".into(),
                status: "configured".into(),
            }],
            agents_location: Some("/state/agents.json".into()),
            other_agents: vec![AndroidAgentPromptAddress {
                id: "peer".into(),
                name: "Peer".into(),
                description: Some("Research".into()),
                is_group: false,
            }],
            groups: Vec::new(),
            mcp_servers: vec![
                AndroidMcpPromptServer {
                    name: "Acme".into(),
                    connected: true,
                    custom_instructions: "Use raw rows.".into(),
                },
                AndroidMcpPromptServer {
                    name: "Disconnected".into(),
                    connected: false,
                    custom_instructions: "Must not render.".into(),
                },
            ],
            mcp_discovery_unavailable: true,
        };
        append_android_dynamic_runtime_system_instructions(
            &mut prompt,
            &snapshot,
            Some("America/Los_Angeles"),
            200_000,
        );
        append_android_dynamic_runtime_system_instructions(
            &mut prompt,
            &snapshot,
            Some("America/Los_Angeles"),
            200_000,
        );
        let routines = prompt.find("## Routines").unwrap();
        let workflows = prompt.find("Workflows are a GLOBAL").unwrap();
        let skills = prompt.find("<agent_skills>").unwrap();
        let channels = prompt.find("Channels: outside messaging surfaces").unwrap();
        let directory = prompt.find("Your teammates: the other agents").unwrap();
        let custom = prompt.find("## Connector custom instructions").unwrap();
        let discovery = prompt.find("<mcp_status>").unwrap();
        assert!(
            routines < workflows
                && workflows < skills
                && skills < channels
                && channels < directory
                && directory < custom
                && custom < discovery
        );
        for marker in [
            "## Routines",
            "Workflows are a GLOBAL",
            "<agent_skills>",
            "Channels: outside messaging surfaces",
            "Your teammates: the other agents",
            "## Connector custom instructions",
            "<mcp_status>",
        ] {
            assert_eq!(prompt.matches(marker).count(), 1, "{marker}");
        }
        assert!(prompt.contains("Use raw rows."));
        assert!(!prompt.contains("Must not render."));
        assert!(prompt.contains("does NOT mean the user has no MCP connectors"));
        assert!(prompt.contains("never grants"));
    }

    #[test]
    fn skill_catalog_uses_two_percent_initial_context_budget_and_truncates() {
        let skills = (0..120)
            .map(|index| AndroidSkillPromptItem {
                full_path: format!("/state/workflows/skill-{index:03}/SKILL.md"),
                description: Some("x".repeat(900)),
            })
            .collect::<Vec<_>>();
        let rendered = render_budgeted_android_skills_prompt(&skills, 2_000);
        assert!(rendered.contains("<agent_skills>"));
        assert!(
            estimate_prompt_tokens(&rendered) <= 40
                || rendered.contains("Additional skills omitted from this initial list")
        );
        assert!(rendered.contains("Additional skills omitted") || rendered.contains("..."));
    }

    #[test]
    fn empty_dynamic_states_match_desktop_omission_semantics() {
        let mut prompt = build_android_root_system_instructions("", None);
        let snapshot = AndroidDynamicPromptSnapshot {
            automations_location: Some("/state/automation-runtime.json".into()),
            workflows_location: Some("/state/workflows".into()),
            agents_location: Some("/state/agents.json".into()),
            channels_location: Some("/state/channels".into()),
            ..AndroidDynamicPromptSnapshot::default()
        };
        append_android_dynamic_runtime_system_instructions(
            &mut prompt,
            &snapshot,
            None,
            200_000,
        );
        assert!(prompt.contains("No routines yet."));
        assert!(prompt.contains("Workflows are a GLOBAL"));
        assert!(!prompt.contains("<agent_skills>"));
        assert!(!prompt.contains("Channels: outside messaging surfaces"));
        assert!(prompt.contains("no other agents currently addressable"));
        assert!(!prompt.contains("## Connector custom instructions"));
        assert!(!prompt.contains("<mcp_status>"));
    }

    #[test]
    fn remote_runner_section_coexists_after_dynamic_runtime_context() {
        let mut prompt = build_android_root_system_instructions("", None);
        append_android_dynamic_runtime_system_instructions(
            &mut prompt,
            &AndroidDynamicPromptSnapshot {
                automations_location: Some("/state/automation-runtime.json".into()),
                workflows_location: Some("/state/workflows".into()),
                agents_location: Some("/state/agents.json".into()),
                ..AndroidDynamicPromptSnapshot::default()
            },
            None,
            200_000,
        );
        append_android_remote_runtime_system_instructions(
            &mut prompt,
            &AndroidRemotePromptState {
                ready: true,
                paused: false,
                has_desktop: true,
                executors: vec!["computer".into()],
            },
        );
        assert!(prompt.contains("## Routines"));
        assert!(prompt.contains("## Remote computer"));
        assert!(prompt.find("<mcp_status>").is_none());
        assert!(prompt.find("Your teammates: the other agents").unwrap() < prompt.find("## Remote computer").unwrap());
    }

    #[test]
    fn prompt_account_fence_rejects_stale_account_or_session_epoch() {
        assert!(ensure_android_prompt_account_snapshot_current(
            "session:a:epoch-1",
            "session:a:epoch-1",
        )
        .is_ok());
        assert!(ensure_android_prompt_account_snapshot_current(
            "session:a:epoch-1",
            "session:a:epoch-2",
        )
        .unwrap_err()
        .contains("stale"));
    }

    #[test]
    fn walkthrough_artifact_directory_uses_canonical_cloud_agent_fallback_and_normalizes_override() {
        assert_eq!(
            resolve_android_prompt_artifacts_dir(None),
            CLOUD_AGENT_ARTIFACTS_DIR
        );
        assert_eq!(
            resolve_android_prompt_artifacts_dir(Some(" /tmp/walkthroughs ")),
            "/tmp/walkthroughs/"
        );
        assert_eq!(
            resolve_android_prompt_artifacts_dir(Some("C:\\walkthroughs")),
            "C:\\walkthroughs\\"
        );
    }

    #[test]
    fn env_setup_skill_uses_canonical_cloud_agent_identity_and_remains_protected() {
        let item = AndroidSkillPromptItem {
            full_path: format!("/.cursor/skills-cursor/{ENV_SETUP_SKILL_ID}/SKILL.md"),
            description: Some("Managed environment setup".into()),
        };
        assert_eq!(skill_name(&item.full_path), ENV_SETUP_SKILL_ID);
        assert!(is_protected_skill(&item));
    }

    #[test]
    fn walkthrough_artifact_prompt_is_deduped_and_does_not_grant_capability() {
        let prompt = build_android_root_system_instructions("", None);
        assert_eq!(prompt.matches("## Walkthrough artifacts").count(), 1);
        assert!(prompt.contains("/opt/cursor/artifacts"));
        assert!(prompt.contains("does not grant filesystem, computer-use, recording, or upload capability"));
    }

}
