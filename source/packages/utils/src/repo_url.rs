pub fn is_origin_git_host(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    let Some(prefix) = lower.strip_suffix(".cursor.com") else { return false; };
    if prefix == "origin" {
        return true;
    }
    let Some(suffix) = prefix.strip_prefix("origin-") else { return false; };
    !suffix.is_empty() && suffix.bytes().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_origin_cursor_hosts_only() {
        assert!(is_origin_git_host("origin.cursor.com"));
        assert!(is_origin_git_host("ORIGIN-abc123.cursor.com"));
        assert!(!is_origin_git_host("origin-.cursor.com"));
        assert!(!is_origin_git_host("origin_abc.cursor.com"));
        assert!(!is_origin_git_host("example.com"));
    }
}
