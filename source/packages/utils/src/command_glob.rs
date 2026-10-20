pub fn matches_command_glob(pattern: &str, value: &str) -> bool {
    let pattern = pattern.trim().as_bytes();
    let value = value.as_bytes();

    let (mut p, mut v) = (0usize, 0usize);
    let mut star = None;
    let mut star_match = 0usize;

    while v < value.len() {
        if p < pattern.len() && pattern[p] == value[v] && pattern[p] != b'*' {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            star_match = v;
        } else if let Some(star_index) = star {
            p = star_index + 1;
            star_match += 1;
            v = star_match;
        } else {
            return false;
        }
    }

    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_star_is_wildcard_and_match_is_anchored() {
        assert!(matches_command_glob("git *", "git status"));
        assert!(matches_command_glob("  cargo*test  ", "cargo --all test"));
        assert!(matches_command_glob("literal.+?", "literal.+?"));
        assert!(!matches_command_glob("git", "git status"));
        assert!(!matches_command_glob("git *", "sudo git status"));
    }
}
