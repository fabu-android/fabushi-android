pub const MAX_TEXT_SIZE: usize = 8 * 1024 * 1024;

pub fn text_size_allowed(text: &str) -> bool {
    text.len() <= MAX_TEXT_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_limit_is_inclusive() {
        assert!(text_size_allowed(&"a".repeat(MAX_TEXT_SIZE)));
        assert!(!text_size_allowed(&"a".repeat(MAX_TEXT_SIZE + 1)));
    }
}
