use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use fabushi_android_shared::ExecutionError;

pub const DEBUGGING_THE_BOX_FILE: &str = "DEBUGGING_THE_BOX.md";
pub const SAND_APP_UI_FILE: &str = "FABUSHI_APP_UI.md";
pub const SAND_BOX_DEBUGGING_REFERENCE_DOC: &str = "# Debugging the box\n\nUse box-doctor for bounded diagnostics. If the box needs human assistance, use request_box_help rather than inventing local state.\n";
pub const SAND_APP_UI_REFERENCE_DOC: &str = "# The Fabushi app UI\n\nThe Android app is a remote control surface for the authorized Runner. Update Fabushi's Computer through the normal product flow; do not mutate app-local files to impersonate Box state.\n";

#[derive(Clone, Debug)]
pub struct RemoteReferenceDocsConfig { pub workspace_root: PathBuf, pub reference_dir: PathBuf, pub legacy_reference_dir: Option<PathBuf> }

fn validate_relative(path:&Path,label:&str)->Result<(),ExecutionError>{
    if path.as_os_str().is_empty() || path.is_absolute() || path.components().any(|c| !matches!(c,Component::Normal(_))) {
        return Err(ExecutionError::InvalidRequest(format!("Box reference {label} must be a bounded relative path")));
    } Ok(())
}

fn atomic_write(path:&Path,bytes:&[u8])->Result<(),ExecutionError>{
    let parent=path.parent().ok_or_else(||ExecutionError::InvalidRequest("reference doc has no parent".into()))?;
    fs::create_dir_all(parent).map_err(|e|ExecutionError::Transport(format!("create Box reference dir: {e}")))?;
    let tmp=path.with_extension("tmp");
    let mut file=fs::File::create(&tmp).map_err(|e|ExecutionError::Transport(format!("create Box reference temp: {e}")))?;
    file.write_all(bytes).map_err(|e|ExecutionError::Transport(format!("write Box reference temp: {e}")))?;
    file.sync_all().map_err(|e|ExecutionError::Transport(format!("sync Box reference temp: {e}")))?;
    fs::rename(&tmp,path).map_err(|e|ExecutionError::Transport(format!("replace Box reference doc: {e}")))?;
    Ok(())
}

pub fn provision_remote_reference_docs(config:&RemoteReferenceDocsConfig)->Result<Vec<PathBuf>,ExecutionError>{
    if !config.workspace_root.is_absolute(){return Err(ExecutionError::InvalidRequest("Box workspace root must be absolute".into()));}
    validate_relative(&config.reference_dir,"directory")?;
    if let Some(legacy)=&config.legacy_reference_dir{validate_relative(legacy,"legacy directory")?;}
    let root=fs::canonicalize(&config.workspace_root).map_err(|e|ExecutionError::Transport(format!("canonicalize Box workspace: {e}")))?;
    let reference=root.join(&config.reference_dir);
    fs::create_dir_all(&reference).map_err(|e|ExecutionError::Transport(format!("create Box reference directory: {e}")))?;
    let written=[(DEBUGGING_THE_BOX_FILE,SAND_BOX_DEBUGGING_REFERENCE_DOC),(SAND_APP_UI_FILE,SAND_APP_UI_REFERENCE_DOC)].into_iter().map(|(name,body)|{let p=reference.join(name);atomic_write(&p,body.as_bytes())?;Ok(p)}).collect::<Result<Vec<_>,ExecutionError>>()?;
    if let Some(legacy)=&config.legacy_reference_dir{let path=root.join(legacy);if path.exists(){fs::remove_dir_all(&path).map_err(|e|ExecutionError::Transport(format!("remove legacy Box reference directory: {e}")))?;}}
    Ok(written)
}

#[cfg(test)]
mod tests{use super::*;
#[test]fn provisions_frozen_order_atomically_and_removes_only_bounded_legacy(){let t=tempfile::tempdir().unwrap();let legacy=PathBuf::from(".fabushi/legacy-reference");fs::create_dir_all(t.path().join(&legacy)).unwrap();fs::write(t.path().join(&legacy).join("old.md"),"old").unwrap();let cfg=RemoteReferenceDocsConfig{workspace_root:t.path().to_path_buf(),reference_dir:PathBuf::from(".fabushi/reference"),legacy_reference_dir:Some(legacy.clone())};let written=provision_remote_reference_docs(&cfg).unwrap();assert_eq!(written.iter().map(|p|p.file_name().unwrap().to_string_lossy().to_string()).collect::<Vec<_>>(),vec![DEBUGGING_THE_BOX_FILE,SAND_APP_UI_FILE]);assert!(fs::read_to_string(&written[0]).unwrap().contains("box-doctor"));assert!(fs::read_to_string(&written[1]).unwrap().contains("Update Fabushi's Computer"));assert!(!t.path().join(legacy).exists());assert!(fs::read_dir(t.path().join(".fabushi/reference")).unwrap().all(|e|!e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));}
#[test]fn refuses_absolute_or_parent_escape_paths(){let t=tempfile::tempdir().unwrap();for rel in [PathBuf::from("../escape"),PathBuf::from("/tmp/reference")]{let cfg=RemoteReferenceDocsConfig{workspace_root:t.path().to_path_buf(),reference_dir:rel,legacy_reference_dir:None};assert!(provision_remote_reference_docs(&cfg).is_err());}}
}
