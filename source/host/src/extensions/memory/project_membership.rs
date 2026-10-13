use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

pub const MEMBERSHIP_FILENAME: &str = "projects.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProjectMembership { agent_dir: PathBuf }

impl AgentProjectMembership {
    pub fn new(agent_dir: impl Into<PathBuf>) -> Self { Self { agent_dir: agent_dir.into() } }
    pub fn path(&self) -> PathBuf { get_agent_projects_path(&self.agent_dir) }

    pub fn read(&self) -> BTreeSet<String> {
        let Ok(raw)=fs::read_to_string(self.path()) else { return BTreeSet::new(); };
        let Ok(value)=serde_json::from_str::<Value>(&raw) else { return BTreeSet::new(); };
        to_safe_slug_set(value.get("projects"))
    }

    pub fn write(&self, slugs:&BTreeSet<String>) -> io::Result<()> {
        if !slugs.iter().all(|slug| is_safe_project_slug(slug)) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,"invalid project membership slug"));
        }
        fs::create_dir_all(&self.agent_dir)?;
        let body=serde_json::to_string_pretty(&json!({"projects":slugs.iter().cloned().collect::<Vec<_>>() })).map_err(io::Error::other)?;
        write_atomic(&self.path(),format!("{body}\n").as_bytes())
    }

    pub fn join(&self,slug:&str)->io::Result<bool>{
        if !is_safe_project_slug(slug){return Ok(false)}
        let mut slugs=self.read(); if slugs.insert(slug.to_string()){self.write(&slugs)?;} Ok(true)
    }
    pub fn leave(&self,slug:&str)->io::Result<bool>{
        if !is_safe_project_slug(slug){return Ok(false)}
        let mut slugs=self.read(); if slugs.remove(slug){self.write(&slugs)?;} Ok(true)
    }
    pub fn prune_missing(&self,mut project_exists:impl FnMut(&str)->bool)->io::Result<bool>{
        let mut slugs=self.read(); let before=slugs.len(); slugs.retain(|slug|project_exists(slug));
        if slugs.len()==before{return Ok(false)} self.write(&slugs)?; Ok(true)
    }
}

pub fn get_agent_projects_path(agent_dir:impl AsRef<Path>)->PathBuf{agent_dir.as_ref().join(MEMBERSHIP_FILENAME)}

pub fn is_safe_project_slug(slug:&str)->bool{
    !slug.is_empty() && slug!="." && slug!=".." && slug.len()<=128 && slug.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b,b'.'|b'_'|b'-'))
}

pub fn to_safe_slug_set(value:Option<&Value>)->BTreeSet<String>{
    value.and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).filter(|slug|is_safe_project_slug(slug)).map(ToOwned::to_owned).collect()
}

fn write_atomic(path:&Path,contents:&[u8])->io::Result<()> {
    let parent=path.parent().ok_or_else(||io::Error::new(io::ErrorKind::InvalidInput,"membership path has no parent"))?;
    fs::create_dir_all(parent)?;
    let name=path.file_name().ok_or_else(||io::Error::new(io::ErrorKind::InvalidInput,"membership path has no file name"))?;
    let temporary=parent.join(format!(".{}.{}.tmp",name.to_string_lossy(),std::process::id()));
    let result=(||->io::Result<()>{
        let mut file=fs::File::create(&temporary)?; file.write_all(contents)?; file.sync_all()?; fs::rename(&temporary,path)?; Ok(())
    })();
    if result.is_err(){let _=fs::remove_file(&temporary);} result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsafe_slugs_never_escape_agent_directory(){
        let root=tempfile::tempdir().unwrap(); let membership=AgentProjectMembership::new(root.path().join("agent"));
        for slug in ["",".","..","../escape","nested/escape","a\\b"] { assert_eq!(membership.join(slug).unwrap(),false); }
        assert!(!root.path().join("escape").exists());
    }
    #[test]
    fn membership_survives_reopen_and_prunes_missing_projects_atomically(){
        let root=tempfile::tempdir().unwrap(); let dir=root.path().join("agent");
        let membership=AgentProjectMembership::new(&dir); assert!(membership.join("project-a").unwrap()); assert!(membership.join("project-b").unwrap());
        let reopened=AgentProjectMembership::new(&dir); assert_eq!(reopened.read(),BTreeSet::from(["project-a".to_string(),"project-b".to_string()]));
        assert!(reopened.prune_missing(|slug|slug=="project-b").unwrap()); assert_eq!(reopened.read(),BTreeSet::from(["project-b".to_string()]));
        assert!(fs::read_dir(&dir).unwrap().all(|entry|!entry.unwrap().file_name().to_string_lossy().contains(".tmp")));
    }
    #[test]
    fn corrupt_membership_fails_closed_to_empty_set(){
        let root=tempfile::tempdir().unwrap(); let dir=root.path().join("agent"); fs::create_dir_all(&dir).unwrap(); fs::write(dir.join(MEMBERSHIP_FILENAME),"not-json").unwrap();
        assert!(AgentProjectMembership::new(dir).read().is_empty());
    }
}
