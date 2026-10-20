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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_desktop_git_diff_contract() {
        assert_eq!(GIT_DIFF_APPROXIMATE_MAX_TOKENS, 10_000);
        assert_eq!(GIT_DIFF_CHARS_PER_TOKEN, 4);
        assert_eq!(MAX_GIT_DIFF_CHAR_LENGTH, 40_000);
        assert_eq!(GIT_DIFF_INTRO, "Relevant Diff: The following is the git diff from the current branch to the main/default branch:\n\n");
        assert_eq!(GIT_DIFF_UNCOMMITTED_INTRO, "Relevant Diff: The following is the git diff of uncommitted changes in the working tree:\n\n");
        assert_eq!(GIT_DIFF_TRUNCATION_NOTICE, "\n\n[diff truncated due to size; run `git diff` locally for the full output]");
    }
}
