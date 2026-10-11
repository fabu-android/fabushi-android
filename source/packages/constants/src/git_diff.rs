pub const GIT_DIFF_APPROXIMATE_MAX_TOKENS: usize = 10_000;
pub const GIT_DIFF_CHARS_PER_TOKEN: usize = 4;
pub const MAX_GIT_DIFF_CHAR_LENGTH: usize =
    GIT_DIFF_APPROXIMATE_MAX_TOKENS * GIT_DIFF_CHARS_PER_TOKEN;
pub const GIT_DIFF_INTRO: &str =
    "Relevant Diff: The following is the git diff from the current branch to the main/default branch:\n\n";
pub const GIT_DIFF_UNCOMMITTED_INTRO: &str =
    "Relevant Diff: The following is the git diff of uncommitted changes in the working tree:\n\n";
pub const GIT_DIFF_TRUNCATION_NOTICE: &str =
    "\n\n[diff truncated due to size; run `git diff` locally for the full output]";

pub fn truncate_git_diff(diff: &str) -> String {
    if diff.chars().count() <= MAX_GIT_DIFF_CHAR_LENGTH {
        return diff.to_string();
    }
    let body = diff.chars().take(MAX_GIT_DIFF_CHAR_LENGTH).collect::<String>();
    format!("{body}{GIT_DIFF_TRUNCATION_NOTICE}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_truncation_preserves_small_input_and_marks_large_input() {
        assert_eq!(truncate_git_diff("abc"), "abc");
        let large = "x".repeat(MAX_GIT_DIFF_CHAR_LENGTH + 1);
        let truncated = truncate_git_diff(&large);
        assert!(truncated.ends_with(GIT_DIFF_TRUNCATION_NOTICE));
    }
}
