use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MANAGED_SKILLS_CACHE_FILENAME: &str = "cache.json";
pub const MANAGED_SKILL_FILES_DIRNAME: &str = "skills";

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

pub fn get_managed_skills_cache_path(cache_dir: impl AsRef<Path>) -> PathBuf {
    cache_dir.as_ref().join(MANAGED_SKILLS_CACHE_FILENAME)
}

pub fn get_managed_skill_file_path(cache_dir: impl AsRef<Path>, id: &str) -> PathBuf {
    cache_dir.as_ref().join(MANAGED_SKILL_FILES_DIRNAME).join(id).join("SKILL.md")
}

pub fn is_safe_managed_skill_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && id.len() <= 128
        && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

pub fn is_managed_skill(skill: &ManagedSkill) -> bool {
    is_safe_managed_skill_id(skill.id.trim()) && !skill.body.trim().is_empty()
}

pub fn read_managed_skills_cache(cache_dir: impl AsRef<Path>) -> Option<ManagedSkillsCache> {
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
    if !fetched_at.is_finite() || !skills.iter().all(is_managed_skill) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid managed skills cache"));
    }
    let cache_dir = cache_dir.as_ref();
    fs::create_dir_all(cache_dir)?;
    let cache = ManagedSkillsCache { fetched_at, skills: skills.to_vec() };
    let mut body = serde_json::to_string_pretty(&cache).map_err(io::Error::other)?;
    body.push('\n');
    atomic_write_private(&get_managed_skills_cache_path(cache_dir), body.as_bytes())?;
    materialize_managed_skill_files(cache_dir, skills)
}

pub fn materialize_managed_skill_files(cache_dir: impl AsRef<Path>, skills: &[ManagedSkill]) -> io::Result<()> {
    if !skills.iter().all(is_managed_skill) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid managed skill"));
    }
    let cache_dir = cache_dir.as_ref();
    let files_dir = cache_dir.join(MANAGED_SKILL_FILES_DIRNAME);
    fs::create_dir_all(&files_dir)?;
    let keep = skills.iter().map(|skill| skill.id.as_str()).collect::<BTreeSet<_>>();
    for entry in fs::read_dir(&files_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !keep.contains(name.as_ref()) {
            let path = entry.path();
            if path.is_dir() { fs::remove_dir_all(path)?; } else { fs::remove_file(path)?; }
        }
    }
    for skill in skills {
        let skill_dir = files_dir.join(&skill.id);
        fs::create_dir_all(&skill_dir)?;
        atomic_write_private(&get_managed_skill_file_path(cache_dir, &skill.id), serialize_skill_markdown(skill).as_bytes())?;
    }
    Ok(())
}

fn serialize_skill_markdown(skill: &ManagedSkill) -> String {
    let name = serde_json::to_string(&skill.name).unwrap_or_else(|_| "\"\"".into());
    let description = serde_json::to_string(&skill.description).unwrap_or_else(|_| "\"\"".into());
    let mut lines = vec!["---".to_string(), format!("name: {name}")];
    if !skill.description.is_empty() { lines.push(format!("description: {description}")); }
    lines.push("---".into());
    lines.push(skill.body.trim().to_string());
    format!("{}\n", lines.join("\n"))
}

fn atomic_write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "managed skill path has no parent"))?;
    fs::create_dir_all(parent)?;
    let name = path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "managed skill path has no file name"))?;
    let temp = parent.join(format!(".{}.{}.tmp", name.to_string_lossy(), std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(&temp); }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_ids_are_rejected_before_materialization() {
        for id in ["", ".", "..", "../escape", "nested/escape", "a\\b"] {
            assert!(!is_safe_managed_skill_id(id), "unsafe id accepted: {id}");
        }
        for id in ["skill-1", "team.skill_v2"] { assert!(is_safe_managed_skill_id(id)); }
    }

    #[test]
    fn cache_round_trip_is_atomic_and_removes_stale_skill_directories() {
        let root = tempfile::tempdir().unwrap();
        let first = ManagedSkill { id:"one".into(), name:"One".into(), description:"first".into(), body:"do one".into() };
        let stale = ManagedSkill { id:"stale".into(), name:"Stale".into(), description:String::new(), body:"remove me".into() };
        write_managed_skills_cache(root.path(), &[first.clone(), stale], 10.0).unwrap();
        assert!(get_managed_skill_file_path(root.path(), "stale").is_file());
        write_managed_skills_cache(root.path(), std::slice::from_ref(&first), 11.0).unwrap();
        assert!(!root.path().join(MANAGED_SKILL_FILES_DIRNAME).join("stale").exists());
        assert_eq!(read_managed_skills_cache(root.path()).unwrap().skills, vec![first]);
        assert!(fs::read_dir(root.path()).unwrap().all(|entry| !entry.unwrap().file_name().to_string_lossy().contains(".tmp")));
    }
}
