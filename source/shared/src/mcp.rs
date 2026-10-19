pub const DEFAULT_MCP_ACCOUNT_KEY: &str = "default";
pub const MAX_RENDERED_MCP_ACCOUNT_LABEL_LENGTH: usize = 64;
pub const MAX_CONNECTOR_ERROR_LENGTH: usize = 300;
pub const MAX_UNTRUSTED_MARKUP_SCAN_LENGTH: usize = 16_384;

pub fn normalize_mcp_account_label(raw_label: &str) -> String {
    raw_label.trim().to_lowercase()
}

pub fn provisional_mcp_account_server_identifier(
    row_identifier: &str,
    account_key: &str,
) -> String {
    if account_key == DEFAULT_MCP_ACCOUNT_KEY {
        row_identifier.to_string()
    } else {
        format!("{row_identifier}--{account_key}")
    }
}

fn hostile_label_char(character: char) -> bool {
    character <= '\u{001f}'
        || character == '\u{007f}'
        || matches!(
            character,
            '"' | '\'' | '`' | '\\' | '[' | ']' | '{' | '}' | '(' | ')' | '<' | '>'
        )
        || matches!(character, '\u{2028}' | '\u{2029}')
}

pub fn encode_mcp_account_label_for_listing(label: &str) -> String {
    let mut escaped = String::new();
    for character in label.chars() {
        if hostile_label_char(character) {
            escaped.push_str(&format!("\\u{:04x}", character as u32));
        } else {
            escaped.push(character);
        }
    }
    format!("\"{escaped}\"")
}

pub fn decode_mcp_account_label_argument(raw_argument: &str) -> String {
    let value = raw_argument.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        if let Ok(parsed) = serde_json::from_str::<String>(value) {
            return parsed;
        }
    }
    raw_argument.to_string()
}

pub fn format_mcp_account_label_for_prompt(raw_label: &str) -> String {
    let inert: String = raw_label
        .chars()
        .filter(|character| {
            !matches!(
                character,
                '"' | '\'' | '`' | '\\' | '[' | ']' | '{' | '}' | '(' | ')' | '<' | '>'
            )
        })
        .collect();
    let collapsed = inert.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(MAX_RENDERED_MCP_ACCOUNT_LABEL_LENGTH).collect()
}

pub fn format_mcp_account_display_name(name: &str, account_key: Option<&str>) -> String {
    match account_key {
        Some(key) if key != DEFAULT_MCP_ACCOUNT_KEY => {
            format!("{name} ({})", format_mcp_account_label_for_prompt(key))
        }
        _ => name.to_string(),
    }
}

pub fn is_effective_plugin_installed(is_enabled: bool) -> bool {
    is_enabled
}

pub fn uninstall_cleared_install_record(removed: bool, reason: Option<&str>) -> bool {
    removed || reason == Some("team-server")
}

fn strip_block_element(mut input: String, tag: &str) -> String {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    loop {
        let lower = input.to_ascii_lowercase();
        let Some(start) = lower.find(&open) else {
            return input;
        };
        let end = lower[start..]
            .find(&close)
            .map(|offset| start + offset + close.len())
            .unwrap_or(input.len());
        input.replace_range(start..end, " ");
    }
}

pub fn strip_markup_and_bound_connector_error(raw: &str) -> String {
    let bounded: String = raw.chars().take(MAX_UNTRUSTED_MARKUP_SCAN_LENGTH).collect();
    let bounded = strip_block_element(strip_block_element(bounded, "script"), "style");
    let mut out = String::with_capacity(bounded.len());
    let mut in_tag = false;
    for ch in bounded.chars() {
        if ch == '<' {
            in_tag = true;
            out.push(' ');
            continue;
        }
        if in_tag {
            if ch == '>' {
                in_tag = false;
                out.push(' ');
            }
            continue;
        }
        out.push(ch);
    }
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= MAX_CONNECTOR_ERROR_LENGTH {
        return collapsed;
    }
    let mut truncated: String = collapsed
        .chars()
        .take(MAX_CONNECTOR_ERROR_LENGTH.saturating_sub(1))
        .collect();
    while truncated.chars().last().is_some_and(char::is_whitespace) {
        truncated.pop();
    }
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_normalized_escaped_and_bounded() {
        assert_eq!(normalize_mcp_account_label("  Work  "), "work");
        assert_eq!(
            provisional_mcp_account_server_identifier("github", "team"),
            "github--team"
        );
        assert_eq!(encode_mcp_account_label_for_listing("a<b"), "\"a\\u003cb\"");
        assert_eq!(decode_mcp_account_label_argument("\"Team A\""), "Team A");
        assert_eq!(format_mcp_account_label_for_prompt(" <Team>   A "), "Team A");
    }

    #[test]
    fn connector_errors_drop_markup_and_are_bounded() {
        let cleaned = strip_markup_and_bound_connector_error(
            "<div> failed <b>now</b> </div>",
        );
        assert_eq!(cleaned, "failed now");
        assert_eq!(
            strip_markup_and_bound_connector_error("<script>secret()</script><p>safe</p>"),
            "safe"
        );
        assert_eq!(
            strip_markup_and_bound_connector_error("<style>.x{display:none}</style>visible"),
            "visible"
        );
        let long = "x".repeat(MAX_CONNECTOR_ERROR_LENGTH + 50);
        let bounded = strip_markup_and_bound_connector_error(&long);
        assert_eq!(bounded.chars().count(), MAX_CONNECTOR_ERROR_LENGTH);
        assert!(bounded.ends_with('…'));
    }
}
