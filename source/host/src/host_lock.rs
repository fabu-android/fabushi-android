use std::{fs::{self,File,OpenOptions},io::{self,Write},path::{Path,PathBuf}};

pub const DEFAULT_TAKEOVER_TIMEOUT_MS:u64=5_000;
pub const DEFAULT_POLL_INTERVAL_MS:u64=100;
pub const MAX_ACQUIRE_ATTEMPTS:u32=50;

pub struct HostLock { path:PathBuf, _file:File }

impl HostLock {
    pub fn acquire(path:&Path)->io::Result<Self>{
        if let Some(parent)=path.parent(){fs::create_dir_all(parent)?;}
        let mut file=OpenOptions::new().create_new(true).write(true).open(path)?;
        writeln!(file,"{}",std::process::id())?;
        Ok(Self{path:path.to_path_buf(),_file:file})
    }
}
impl Drop for HostLock { fn drop(&mut self){ let _=fs::remove_file(&self.path); } }

pub fn read_lock_pid(path:&Path)->io::Result<u32>{ fs::read_to_string(path)?.trim().parse().map_err(|_|io::Error::new(io::ErrorKind::InvalidData,"invalid lock pid")) }
