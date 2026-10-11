pub const ENV_SETUP_SKILL_ID: &str = "env-setup";
pub const ENV_SETUP_MANAGED_SKILL_DIRECTORY: &str = "/.cursor/skills-cursor/env-setup/";
pub const ENV_SETUP_MANAGED_SKILL_PATH: &str = "/.cursor/skills-cursor/env-setup/SKILL.md";
pub const CLOUD_AGENT_SINGLE_REPO_WORKSPACE_ROOT: &str = "/workspace";
pub const CLOUD_AGENT_ARTIFACTS_DIR: &str = "/opt/cursor/artifacts/";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_skill_path_stays_inside_managed_directory() {
        assert!(ENV_SETUP_MANAGED_SKILL_PATH.starts_with(ENV_SETUP_MANAGED_SKILL_DIRECTORY));
        assert!(ENV_SETUP_MANAGED_SKILL_PATH.ends_with("/SKILL.md"));
    }
}
