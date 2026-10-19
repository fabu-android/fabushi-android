use std::path::{Path, PathBuf};

use crate::storage::folder_id::is_safe_folder_id;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Invalid Sand agent id: {0}")]
pub struct SandInvalidAgentIdError(pub String);

pub fn assert_valid_sand_agent_id(agent_id: &str) -> Result<(), SandInvalidAgentIdError> {
    if !is_safe_folder_id(agent_id) || agent_id.trim() != agent_id {
        return Err(SandInvalidAgentIdError(agent_id.to_owned()));
    }
    Ok(())
}

/// Resolve one Agent directory beneath an Android app-private agents root.
///
/// Android must not infer production storage from HOME/argv. Kotlin/Host composition supplies
/// the app-private root; this helper validates that the id contributes exactly one path component.
pub fn resolve_sand_agent_dir(
    agents_root: &Path,
    agent_id: &str,
) -> Result<PathBuf, SandInvalidAgentIdError> {
    assert_valid_sand_agent_id(agent_id)?;
    let target = agents_root.join(agent_id);
    let relative = target
        .strip_prefix(agents_root)
        .map_err(|_| SandInvalidAgentIdError(agent_id.to_owned()))?;
    if relative.as_os_str().is_empty() || relative.components().count() != 1 {
        return Err(SandInvalidAgentIdError(agent_id.to_owned()));
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_id_is_one_safe_component_under_supplied_root() {
        let root = Path::new("/private/fabushi/agents");
        assert_eq!(
            resolve_sand_agent_dir(root, "agent-a").unwrap(),
            root.join("agent-a")
        );
        for invalid in ["", ".", "..", " ../escape", "../escape", "a/b", r"a\b", " agent"] {
            assert!(resolve_sand_agent_dir(root, invalid).is_err(), "{invalid}");
        }
    }
}
