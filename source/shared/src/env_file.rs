pub const ENV_NAME_PATTERN: &str = "^[A-Za-z_][A-Za-z0-9_]*$";

pub fn is_valid_env_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else { return false; };
    if !(first.is_ascii_alphabetic() || first == '_') { return false; }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_shell_style_environment_names() {
        assert!(is_valid_env_name("FABUSHI_TOKEN"));
        assert!(is_valid_env_name("_PRIVATE1"));
        assert!(!is_valid_env_name("1BAD"));
        assert!(!is_valid_env_name("BAD-NAME"));
        assert!(!is_valid_env_name(""));
    }
}
