use std::{collections::BTreeMap,fs,io,path::{Path,PathBuf},time::SystemTime};
#[derive(Default)]
pub struct WatchedDirectory{snapshot:BTreeMap<PathBuf,SystemTime>}
impl WatchedDirectory{
    pub fn scan(&mut self,root:&Path)->io::Result<Vec<PathBuf>>{
        let mut next=BTreeMap::new();
        if root.exists(){for entry in fs::read_dir(root)?{let entry=entry?;let path=entry.path();if path.is_file(){let modified=entry.metadata()?.modified().unwrap_or(SystemTime::UNIX_EPOCH);next.insert(path,modified);}}}
        let changed=next.iter().filter_map(|(path,mtime)|if self.snapshot.get(path)!=Some(mtime){Some(path.clone())}else{None})
            .chain(self.snapshot.keys().filter(|path|!next.contains_key(*path)).cloned()).collect();
        self.snapshot=next;Ok(changed)
    }
}
