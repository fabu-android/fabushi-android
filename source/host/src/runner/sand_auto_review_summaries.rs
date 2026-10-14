use std::collections::HashSet;

use serde_json::{Map, Value, json};

const SECRET_KEYS: &[&str] = &[
    "authorization",
    "apikey",
    "api_key",
    "api-key",
    "credential",
    "password",
    "secret",
    "token",
];

pub fn compact(value: &str, max_chars: usize) -> String {
    let normalized = normalize_whitespace(value);
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    if max_chars == 0 {
        return String::new();
    }
    let keep = max_chars.saturating_sub(1);
    format!("{}…", take_chars(&normalized, keep))
}

pub fn compact_head_and_tail(value: &str, max_chars: usize) -> String {
    let normalized = normalize_whitespace(value);
    let length = normalized.chars().count();
    if length <= max_chars {
        return normalized;
    }
    let mut omitted_chars = length.saturating_sub(max_chars);
    let mut omitted_label = String::new();
    let mut available = 2usize;
    for _ in 0..4 {
        omitted_label = format!("…[{omitted_chars} chars omitted]…");
        available = max_chars.saturating_sub(omitted_label.chars().count()).max(2);
        let actual = length.saturating_sub(available);
        if actual == omitted_chars {
            break;
        }
        omitted_chars = actual;
    }
    let head_chars = available.div_ceil(2);
    let tail_chars = available / 2;
    format!(
        "{}{}{}",
        take_chars(&normalized, head_chars),
        omitted_label,
        take_last_chars(&normalized, tail_chars)
    )
}

pub fn shell_location_phrase(surface: &str) -> &'static str {
    if surface == "host_shell" {
        "on your local computer"
    } else {
        "on Fabushi's computer"
    }
}

pub fn generic_sand_shell_auto_review_summary(surface: &str) -> &'static str {
    if surface == "host_shell" {
        "Run a command on your local computer"
    } else {
        "Run a command on Fabushi's computer"
    }
}

pub fn describe_sand_shell_auto_review_action(
    surface: &str,
    description: Option<&str>,
    working_directory: Option<&str>,
) -> String {
    let location = shell_location_phrase(surface);
    let description = description.map(normalize_whitespace);
    let cwd = working_directory
        .map(|cwd| format!(" from {}", compact(cwd, 100)))
        .unwrap_or_default();
    if let Some(description) = description.filter(|value| !value.is_empty()) {
        let head = trim_terminal_punctuation(&description);
        return compact(&format!("{head} {location}{cwd}"), 340);
    }
    compact(
        &format!("{}{cwd}", generic_sand_shell_auto_review_summary(surface)),
        340,
    )
}

pub fn redact_mcp_value(value: &Value, key: Option<&str>, depth: usize) -> Value {
    if key.is_some_and(contains_secret_key) {
        return Value::String("…".into());
    }
    match value {
        Value::String(value) => {
            let redacted = redact_inline_secrets(value);
            if looks_like_secret_value(&redacted) {
                Value::String("…".into())
            } else {
                Value::String(compact_head_and_tail(&redacted, 160))
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
        Value::Array(items) if depth >= 3 => Value::String(format!("[{} items]", items.len())),
        Value::Object(_) if depth >= 3 => Value::String("{…}".into()),
        Value::Array(items) => {
            let selected = if items.len() <= 6 {
                items.clone()
            } else {
                let mut values = items[..3].to_vec();
                values.push(Value::String(format!("{} items omitted", items.len() - 6)));
                values.extend_from_slice(&items[items.len() - 3..]);
                values
            };
            Value::Array(
                selected
                    .iter()
                    .map(|entry| redact_mcp_value(entry, None, depth + 1))
                    .collect(),
            )
        }
        Value::Object(object) => {
            let entries = object.iter().collect::<Vec<_>>();
            let selected: Vec<(String, Value)> = if entries.len() <= 12 {
                entries
                    .into_iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect()
            } else {
                let mut values = entries[..6]
                    .iter()
                    .map(|(key, value)| ((*key).clone(), (*value).clone()))
                    .collect::<Vec<_>>();
                values.push(("…".into(), Value::String(format!("{} fields omitted", entries.len() - 12))));
                values.extend(
                    entries[entries.len() - 6..]
                        .iter()
                        .map(|(key, value)| ((*key).clone(), (*value).clone())),
                );
                values
            };
            Value::Object(
                selected
                    .into_iter()
                    .map(|(entry_key, entry)| {
                        (
                            compact(&entry_key, 60),
                            redact_mcp_value(&entry, Some(&entry_key), depth + 1),
                        )
                    })
                    .collect::<Map<String, Value>>(),
            )
        }
    }
}

pub fn summarize_sand_mcp_auto_review_action(
    server_display_name: &str,
    tool_name: &str,
    mcp_arguments: Option<&Value>,
) -> String {
    let server = compact(server_display_name, 80);
    let tool = compact(tool_name, 80);
    let payload = redact_mcp_value(mcp_arguments.unwrap_or(&json!({})), None, 0);
    let details = compact_head_and_tail(
        &serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into()),
        300,
    );
    format!(
        "Use {} tool {} with {details}",
        if server.is_empty() { "a connected service" } else { &server },
        if tool.is_empty() { "action" } else { &tool },
    )
}

pub fn generic_sand_mcp_auto_review_summary(server_display_name: &str) -> String {
    let server = compact(server_display_name, 80);
    if server.is_empty() {
        "Use a connected service".into()
    } else {
        format!("Use {server}")
    }
}

pub fn humanize_mcp_tool_action(tool_name: &str) -> Option<String> {
    let compacted = compact(tool_name, 80);
    let mut spaced = String::new();
    let chars = compacted.chars().collect::<Vec<_>>();
    for (index, ch) in chars.iter().enumerate() {
        if index > 0
            && ch.is_ascii_uppercase()
            && chars[index - 1].is_ascii_lowercase()
        {
            spaced.push(' ');
        }
        spaced.push(if matches!(ch, '-' | '_' | '.') { ' ' } else { *ch });
    }
    let words = spaced
        .to_lowercase()
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if words.is_empty() {
        return None;
    }
    let verbs: HashSet<&str> = [
        "send", "create", "update", "delete", "post", "write", "read", "add",
        "remove", "open", "close", "list", "search", "fetch", "get", "set",
        "upload", "download", "invite", "share",
    ]
    .into_iter()
    .collect();
    if words.len() == 2
        && verbs.contains(words[0].as_str())
        && !words[1].to_lowercase().ends_with('s')
    {
        let article = if words[1]
            .chars()
            .next()
            .is_some_and(|ch| "aeiou".contains(ch))
        {
            "an"
        } else {
            "a"
        };
        return Some(format!("{} {article} {}", words[0], words[1]));
    }
    Some(words.join(" "))
}

pub fn mentions_server(text: &str, server_display_name: &str) -> bool {
    let server = server_display_name.trim();
    !server.is_empty() && text.to_lowercase().contains(&server.to_lowercase())
}

pub fn safe_mcp_destination_hint(mcp_arguments: Option<&Value>) -> Option<String> {
    let object = mcp_arguments?.as_object()?;
    for (raw_key, value) in object {
        let Some(value) = value.as_str() else {
            continue;
        };
        let key = raw_key
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .collect::<String>()
            .to_lowercase();
        if !matches!(
            key.as_str(),
            "channel" | "channelid" | "path" | "page" | "pageid" | "repo"
                | "repository" | "title" | "name" | "url"
        ) || contains_secret_key(raw_key)
        {
            continue;
        }
        let redacted = normalize_whitespace(&redact_inline_secrets(value));
        if redacted.is_empty()
            || redacted.chars().count() > 80
            || looks_like_secret_value(&redacted)
        {
            continue;
        }
        return Some(compact(&redacted, 60));
    }
    None
}

pub fn fallback_sand_mcp_auto_review_summary(
    server_display_name: &str,
    tool_name: &str,
    mcp_arguments: Option<&Value>,
) -> String {
    let server = compact(server_display_name, 80);
    let action = humanize_mcp_tool_action(tool_name);
    let hint = safe_mcp_destination_hint(mcp_arguments);
    let Some(action) = action else {
        return generic_sand_mcp_auto_review_summary(&server);
    };
    let base = if server.is_empty() {
        format!("Use a connected service to {action}")
    } else {
        format!("Use {server} to {action}")
    };
    match hint {
        Some(hint) if !base.to_lowercase().contains(&hint.to_lowercase()) => {
            format!("{base} to {hint}")
        }
        _ => base,
    }
}

pub fn describe_sand_mcp_auto_review_action(
    description: Option<&str>,
    server_display_name: &str,
    tool_name: &str,
    mcp_arguments: Option<&Value>,
) -> String {
    let description = description.map(normalize_whitespace);
    let server = compact(server_display_name, 80);
    if let Some(description) = description.filter(|value| !value.is_empty()) {
        let head = trim_terminal_punctuation(&description);
        if !server.is_empty() && !mentions_server(head, &server) {
            return compact(&format!("{head} with {server}"), 340);
        }
        return compact(head, 340);
    }
    fallback_sand_mcp_auto_review_summary(server_display_name, tool_name, mcp_arguments)
}

pub fn summarize_sand_automation_write_action(
    operation: &str,
    name: &str,
    trigger_description: &str,
    prompt: &str,
    is_enabled: Option<bool>,
    referencing_routine_names: &[String],
) -> String {
    let compact_name = compact(name, 80);
    let name = non_empty(&compact_name, "a routine");
    let instruction = compact_head_and_tail(&redact_inline_secrets(prompt), 240);
    if operation == "workflow_body" {
        let routines = referencing_routine_names
            .iter()
            .map(|routine| compact(routine, 40))
            .filter(|routine| !routine.is_empty())
            .take(3)
            .collect::<Vec<_>>()
            .join(", ");
        let routines = if routines.is_empty() { "standing orders" } else { &routines };
        return format!("Change workflow “{name}” used by {routines}: “{instruction}”");
    }
    let compact_trigger = compact(trigger_description, 120);
    let when = non_empty(&compact_trigger, "on a trigger").to_lowercase();
    let verb = if operation == "create" { "Save" } else { "Change" };
    let paused = if is_enabled == Some(false) { " (paused)" } else { "" };
    format!("{verb} the routine “{name}”{paused} to run {when}: “{instruction}”")
}

pub fn summarize_sand_cloud_agent_action(
    action: &str,
    prompt: &str,
    agent_id: Option<&str>,
    image_count: usize,
    interrupt: bool,
    repo_url: Option<&str>,
    title: Option<&str>,
) -> String {
    let instruction = compact_head_and_tail(&redact_inline_secrets(prompt), 240);
    let images = if image_count > 0 {
        format!(
            " with {image_count} attached image{} it can see",
            if image_count == 1 { "" } else { "s" }
        )
    } else {
        String::new()
    };
    if action == "reply" {
        let target = agent_id.map(|id| compact(id, 40)).unwrap_or_default();
        let target = if target.is_empty() { "a cloud agent" } else { &target };
        let interrupt = if interrupt { " (interrupt)" } else { "" };
        return format!("Send a follow-up to cloud agent {target}{interrupt}{images}: “{instruction}”");
    }
    let repo = repo_url.map(|url| compact(url, 120)).unwrap_or_default();
    let titled = title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(|title| format!(" “{}”", compact(title, 80)))
        .unwrap_or_default();
    let where_text = if repo.is_empty() { String::new() } else { format!(" on {repo}") };
    format!("Launch a cloud agent{titled}{where_text}{images}: “{instruction}”")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudLifecycleAction {
    Rename,
    Cancel,
    Archive,
    Unarchive,
    Delete,
}

pub fn summarize_sand_cloud_agent_lifecycle_action(
    action: CloudLifecycleAction,
    agent_id: &str,
    title: Option<&str>,
) -> String {
    let target = compact(agent_id, 40);
    let target = if target.is_empty() { "a cloud agent" } else { &target };
    match action {
        CloudLifecycleAction::Rename => {
            let title = title
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .map(|title| format!(" to “{}”", compact(title, 80)))
                .unwrap_or_default();
            format!("Rename cloud agent {target}{title}")
        }
        CloudLifecycleAction::Cancel => format!("Cancel the active run of cloud agent {target}"),
        CloudLifecycleAction::Archive => format!("Archive cloud agent {target}"),
        CloudLifecycleAction::Unarchive => format!("Unarchive cloud agent {target}"),
        CloudLifecycleAction::Delete => format!("Permanently delete cloud agent {target}"),
    }
}

pub fn summarize_sand_subagent_action(action: &str, prompt: &str) -> String {
    let instruction = compact_head_and_tail(&redact_inline_secrets(prompt), 240);
    if action == "steer" {
        format!("Send a follow-up to a running task: “{instruction}”")
    } else {
        format!("Run a task on Fabushi's computer: “{instruction}”")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SandBrowserSummaryArgs {
    pub op: String,
    pub element: Option<String>,
    pub target_page_url: Option<String>,
    pub url: Option<String>,
    pub text: Option<String>,
    pub value: Option<String>,
    pub values: Vec<String>,
    pub key: Option<String>,
    pub cdp_method: Option<String>,
    pub cdp_params: Option<String>,
    pub tabs_action: Option<String>,
    pub tab_index: Option<usize>,
}

pub fn summarize_sand_browser_auto_review_action(args: &SandBrowserSummaryArgs) -> String {
    let element = args.element.as_deref().map(normalize_whitespace);
    let target = element
        .filter(|element| !element.is_empty())
        .map(|element| format!("“{}”", compact(&element, 160)))
        .unwrap_or_else(|| "an element".into());
    let page = args
        .target_page_url
        .as_deref()
        .filter(|url| !url.is_empty())
        .map(|url| format!(" on {}", compact_head_and_tail(&redact_inline_secrets(url), 120)))
        .unwrap_or_default();
    let summarize = |action: String| compact(&format!("{action}{page} in the box browser"), 340);
    match args.op.as_str() {
        "navigate" => compact(
            &format!(
                "Open “{}” in the box browser",
                redact_inline_secrets(args.url.as_deref().unwrap_or(""))
            ),
            340,
        ),
        "click" | "mouse_click_xy" => summarize(format!("Click {target}")),
        "type" => summarize(summarize_typed_text(args.text.as_deref().unwrap_or(""))),
        "fill" => summarize(summarize_typed_text(args.value.as_deref().unwrap_or(""))),
        "select_option" => summarize(format!(
            "Select “{}” in {target}",
            redact_inline_secrets(&args.values.join(", "))
        )),
        "press_key" => summarize(format!(
            "Press {}",
            args.key
                .as_deref()
                .map(|key| take_chars(key, 80))
                .unwrap_or_else(|| "a key".into())
        )),
        "drag" => summarize(format!("Drag {target}")),
        "cdp" => summarize(format!(
            "Run CDP command {} with {}",
            args.cdp_method
                .as_deref()
                .map(|method| take_chars(method, 120))
                .unwrap_or_default(),
            compact_head_and_tail(
                &redact_inline_secrets(args.cdp_params.as_deref().unwrap_or("{}")),
                160,
            )
        )),
        "tabs" => {
            if args.tabs_action.as_deref() == Some("close") {
                summarize(format!(
                    "Close browser tab{}",
                    args.tab_index.map(|index| format!(" {index}")).unwrap_or_default()
                ))
            } else {
                summarize("Open a new browser tab".into())
            }
        }
        op => compact(&format!("Browser {op} on Fabushi's computer"), 340),
    }
}

pub fn summarize_typed_text(text: &str) -> String {
    let redacted = redact_inline_secrets(text);
    let normalized = compact_head_and_tail(&redacted, 80);
    let trimmed = text.trim();
    let looks_sensitive = redacted != text
        || looks_like_secret_value(trimmed)
        || contains_inline_secret_assignment(text)
        || (trimmed.chars().count() >= 24
            && !trimmed.contains(' ')
            && trimmed
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || "+/_=-".contains(ch)));
    if looks_sensitive || normalized.is_empty() {
        format!("Type {} characters", text.chars().count())
    } else {
        format!("Type {} characters (“{normalized}”)", text.chars().count())
    }
}

pub fn summarize_sand_computer_typed_text(text: &str) -> String {
    format!("{} on Fabushi's computer", summarize_typed_text(text))
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn take_chars(value: &str, count: usize) -> String {
    value.chars().take(count).collect()
}

fn take_last_chars(value: &str, count: usize) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    chars[chars.len().saturating_sub(count)..].iter().collect()
}

fn trim_terminal_punctuation(value: &str) -> &str {
    value.trim_end_matches(['.', '!', '?'])
}

fn non_empty<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() { fallback } else { value }
}

fn normalized_secret_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

fn contains_secret_key(value: &str) -> bool {
    let normalized = normalized_secret_key(value);
    SECRET_KEYS.iter().any(|key| {
        let key = normalized_secret_key(key);
        normalized.contains(&key)
    })
}

fn looks_like_secret_value(value: &str) -> bool {
    let trimmed = value.trim();
    let raw = trimmed
        .strip_prefix("Bearer ")
        .or_else(|| trimmed.strip_prefix("bearer "))
        .unwrap_or(trimmed);
    let len = raw.chars().count();
    len >= 24
        && raw
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "_+/-=".contains(ch))
}

fn contains_inline_secret_assignment(value: &str) -> bool {
    let lower = value.to_lowercase();
    ["authorization", "api_key", "api-key", "apikey", "password", "secret", "token"]
        .iter()
        .any(|key| {
            lower
                .find(key)
                .is_some_and(|start| {
                    let rest = lower[start + key.len()..].trim_start();
                    rest.starts_with(':') || rest.starts_with('=')
                })
        })
}

fn redact_inline_secrets(value: &str) -> String {
    let mut output = value.to_string();
    for key in [
        "authorization",
        "api_key",
        "api-key",
        "apikey",
        "password",
        "secret",
        "token",
    ] {
        let mut search_from = 0usize;
        loop {
            if search_from >= output.len() {
                break;
            }
            let bytes = output.as_bytes();
            let needle = key.as_bytes();
            let Some(start) = (search_from..=bytes.len().saturating_sub(needle.len()))
                .find(|&index| {
                    bytes
                        .get(index..index + needle.len())
                        .is_some_and(|candidate| {
                            candidate
                                .iter()
                                .zip(needle)
                                .all(|(left, right)| left.to_ascii_lowercase() == *right)
                        })
                })
            else {
                break;
            };

            let after_key = start + needle.len();
            let whitespace = output[after_key..]
                .chars()
                .take_while(|ch| ch.is_whitespace())
                .map(char::len_utf8)
                .sum::<usize>();
            let separator_index = after_key + whitespace;
            let Some(separator) = output[separator_index..].chars().next() else {
                break;
            };
            if !matches!(separator, ':' | '=') {
                search_from = after_key;
                continue;
            }

            let value_start = separator_index + separator.len_utf8();
            let leading = output[value_start..]
                .chars()
                .take_while(|ch| ch.is_whitespace())
                .map(char::len_utf8)
                .sum::<usize>();
            let secret_start = value_start + leading;
            let secret_len = output[secret_start..]
                .chars()
                .take_while(|ch| !ch.is_whitespace())
                .map(char::len_utf8)
                .sum::<usize>();
            if secret_len == 0 {
                search_from = secret_start;
                continue;
            }

            output.replace_range(secret_start..secret_start + secret_len, "…");
            search_from = secret_start + "…".len();
        }
    }
    output
}
