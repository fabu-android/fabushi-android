pub const ADMIN_COMMAND_DENYLIST_MAX_RULE_LENGTH: usize = 512;

fn is_separator(character: char) -> bool {
    matches!(
        character,
        ' ' | '\t' | '\n' | '\r' | '\u{00a0}' | '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{feff}'
    )
}

pub fn normalize_admin_command_denylist_text(value: &str) -> String {
    let mut result = String::new();
    let mut pending_separator = false;
    for character in value.chars() {
        if is_separator(character) {
            pending_separator = true;
            continue;
        }
        if pending_separator && !result.is_empty() {
            result.push(' ');
        }
        pending_separator = false;
        result.push(character);
    }
    if pending_separator && !result.is_empty() {
        result.push(' ');
    }
    result
}

fn is_only_wildcards(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|character| character == '*')
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColonRule {
    pub executable_pattern: String,
    pub args_pattern: String,
}

pub fn parse_admin_command_denylist_colon_rule(rule: &str) -> Option<ColonRule> {
    let index = rule.find(':')?;
    if index == 0 { return None; }
    let executable_pattern = rule[..index].trim();
    if executable_pattern.chars().any(char::is_whitespace) { return None; }
    Some(ColonRule {
        executable_pattern: executable_pattern.to_string(),
        args_pattern: rule[index + 1..].trim().to_string(),
    })
}

pub fn get_admin_command_denylist_rule_error(rule: &str) -> Option<String> {
    let normalized = normalize_admin_command_denylist_text(rule);
    let trimmed = normalized.trim();
    if trimmed.is_empty() { return Some("Rule cannot be empty".into()); }
    if trimmed.chars().count() > ADMIN_COMMAND_DENYLIST_MAX_RULE_LENGTH {
        return Some(format!("Rule cannot exceed {} characters", ADMIN_COMMAND_DENYLIST_MAX_RULE_LENGTH));
    }
    if is_only_wildcards(trimmed) {
        return Some("Rule cannot match every command; be more specific than wildcards alone".into());
    }
    if trimmed.starts_with(':') {
        return Some("Colon rules need an executable before `:` (e.g. `aws:*s3 rm*`)".into());
    }
    if let Some(colon) = parse_admin_command_denylist_colon_rule(trimmed) {
        if is_only_wildcards(&colon.executable_pattern)
            && (colon.args_pattern.is_empty() || is_only_wildcards(&colon.args_pattern))
        {
            return Some("Rule cannot match every command; narrow the executable or argument pattern".into());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_collapses_unicode_and_ascii_separators() {
        assert_eq!(
            normalize_admin_command_denylist_text("  aws\t\u{200b}s3\n rm  "),
            "aws s3 rm "
        );
    }

    #[test]
    fn unsafe_catch_all_and_malformed_colon_rules_fail_closed() {
        assert!(get_admin_command_denylist_rule_error("").is_some());
        assert!(get_admin_command_denylist_rule_error("***").is_some());
        assert!(get_admin_command_denylist_rule_error(":*").is_some());
        assert!(get_admin_command_denylist_rule_error("*:*").is_some());
        assert!(get_admin_command_denylist_rule_error("aws:*s3 rm*").is_none());
    }

    #[test]
    fn colon_rule_requires_one_executable_token() {
        assert_eq!(
            parse_admin_command_denylist_colon_rule("aws: *s3 rm*"),
            Some(ColonRule { executable_pattern: "aws".into(), args_pattern: "*s3 rm*".into() })
        );
        assert!(parse_admin_command_denylist_colon_rule("aws cli:*").is_none());
    }
}
