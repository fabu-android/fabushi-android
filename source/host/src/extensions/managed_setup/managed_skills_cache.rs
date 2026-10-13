use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MANAGED_SKILLS_DIRNAME: &str = "managed-skills";
pub const MANAGED_SKILLS_CACHE_FILENAME: &str = "cache.json";
pub const MANAGED_SKILL_FILES_DIRNAME: &str = "skills";
pub const AGENT_READABLE_SKILL_DIR_MODE: u32 = 0o755;
pub const AGENT_READABLE_SKILL_FILE_MODE: u32 = 0o644;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSkillsCache {
    pub fetched_at: f64,
    pub skills: Vec<ManagedSkill>,
}

pub fn get_managed_skills_dir(sand_root: impl AsRef<Path>) -> PathBuf {
    sand_root.as_ref().join(MANAGED_SKILLS_DIRNAME)
}

pub fn get_managed_skills_cache_path(cache_dir: impl AsRef<Path>) -> PathBuf {
    cache_dir.as_ref().join(MANAGED_SKILLS_CACHE_FILENAME)
}

pub fn get_managed_skill_file_path(
    cache_dir: impl AsRef<Path>,
    id: &str,
) -> PathBuf {
    cache_dir
        .as_ref()
        .join(MANAGED_SKILL_FILES_DIRNAME)
        .join(id)
        .join("SKILL.md")
}

pub fn is_managed_skill(skill: &ManagedSkill) -> bool {
    !skill.id.is_empty() && !skill.body.is_empty()
}

pub fn read_managed_skills_cache(
    cache_dir: impl AsRef<Path>,
) -> Option<ManagedSkillsCache> {
    let raw = fs::read_to_string(get_managed_skills_cache_path(cache_dir)).ok()?;
    let parsed: ManagedSkillsCache = serde_json::from_str(&raw).ok()?;
    if !parsed.fetched_at.is_finite() || !parsed.skills.iter().all(is_managed_skill) {
        return None;
    }
    Some(parsed)
}

pub fn write_managed_skills_cache(
    cache_dir: impl AsRef<Path>,
    skills: &[ManagedSkill],
    fetched_at: f64,
) -> io::Result<()> {
    let cache_dir = cache_dir.as_ref();
    create_agent_readable_dir(cache_dir)?;
    let cache = ManagedSkillsCache {
        fetched_at,
        skills: skills.to_vec(),
    };
    let mut body = serde_json::to_string_pretty(&cache)
        .map_err(io::Error::other)?;
    body.push('\n');
    let path = get_managed_skills_cache_path(cache_dir);
    atomic_write_agent_readable(&path, body.as_bytes())?;
    materialize_managed_skill_files(cache_dir, skills)
}

pub fn materialize_managed_skill_files(
    cache_dir: impl AsRef<Path>,
    skills: &[ManagedSkill],
) -> io::Result<()> {
    let cache_dir = cache_dir.as_ref();
    let files_dir = cache_dir.join(MANAGED_SKILL_FILES_DIRNAME);
    create_agent_readable_dir(&files_dir)?;

    let keep = skills
        .iter()
        .map(|skill| skill.id.as_str())
        .collect::<BTreeSet<_>>();
    for entry in fs::read_dir(&files_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !keep.contains(name.as_ref()) {
            let path = entry.path();
            if path.is_dir() {
                fs::remove_dir_all(path)?;
            } else {
                fs::remove_file(path)?;
            }
        }
    }

    for skill in skills {
        let skill_dir = files_dir.join(&skill.id);
        create_agent_readable_dir(&skill_dir)?;
        let path = get_managed_skill_file_path(cache_dir, &skill.id);
        let content = serialize_skill_markdown(skill);
        atomic_write_agent_readable(&path, content.as_bytes())?;
    }
    Ok(())
}

fn serialize_skill_markdown(skill: &ManagedSkill) -> String {
    let name = serde_json::to_string(&skill.name)
        .unwrap_or_else(|_| "\"\"".into());
    let description = serde_json::to_string(&skill.description)
        .unwrap_or_else(|_| "\"\"".into());
    let mut lines = vec!["---".to_string(), format!("name: {name}")];
    if !skill.description.is_empty() {
        lines.push(format!("description: {description}"));
    }
    lines.push("---".into());
    lines.push(skill.body.trim().to_string());
    format!("{}\n", lines.join("\n"))
}

fn create_agent_readable_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    set_mode(path, AGENT_READABLE_SKILL_DIR_MODE)
}

fn atomic_write_agent_readable(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = PathBuf::from(format!("{}.tmp", path.display()));
    fs::write(&temp, bytes)?;
    set_mode(&temp, AGENT_READABLE_SKILL_FILE_MODE)?;
    match fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temp);
            Err(error)
        }
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod android_parity_tests {
    use super::*;
    #[test]
    fn cache_materialization_reopens_and_removes_revoked_skills() {
        let root = tempfile::tempdir().unwrap();
        let alpha = ManagedSkill { id: "alpha".into(), name: "Alpha".into(), description: "A".into(), body: "Do alpha".into() };
        let beta = ManagedSkill { id: "beta".into(), name: "Beta".into(), description: "B".into(), body: "Do beta".into() };
        write_managed_skills_cache(root.path(), &[alpha.clone(), beta.clone()], 10.0).unwrap();
        assert_eq!(read_managed_skills_cache(root.path()).unwrap().skills, vec![alpha.clone(), beta]);
        write_managed_skills_cache(root.path(), &[alpha.clone()], 20.0).unwrap();
        assert_eq!(read_managed_skills_cache(root.path()).unwrap().skills, vec![alpha]);
        assert!(!get_managed_skill_file_path(root.path(), "beta").exists());
    }
}
