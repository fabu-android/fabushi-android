use std::path::{Path, PathBuf};

pub const ATTACHMENTS_DIRNAME: &str = "attachments";
pub const ASSETS_DIRNAME: &str = "assets";

pub fn get_agent_attachments_dir(root: &Path, agent_id: &str) -> Result<PathBuf, &'static str> {
    safe_agent_dir(root, agent_id, ATTACHMENTS_DIRNAME)
}

pub fn get_agent_assets_dir(root: &Path, agent_id: &str) -> Result<PathBuf, &'static str> {
    safe_agent_dir(root, agent_id, ASSETS_DIRNAME)
}

fn safe_agent_dir(root: &Path, agent_id: &str, leaf: &str) -> Result<PathBuf, &'static str> {
    if agent_id.trim().is_empty() || agent_id.contains('/') || agent_id.contains('\\') || agent_id.contains("..") {
        return Err("invalid agent id");
    }
    Ok(root.join("agents").join(agent_id).join(leaf))
}

pub fn get_agent_media_store_roots(root: &Path, agent_id: &str) -> Result<Vec<PathBuf>, &'static str> {
    Ok(vec![get_agent_attachments_dir(root, agent_id)?, get_agent_assets_dir(root, agent_id)?])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_traversal() {
        assert!(get_agent_assets_dir(Path::new("/tmp"), "../x").is_err());
    }
}
