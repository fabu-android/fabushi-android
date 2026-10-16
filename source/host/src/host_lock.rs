use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub const DEFAULT_TAKEOVER_TIMEOUT_MS: u64 = 0;
pub const DEFAULT_POLL_INTERVAL_MS: u64 = 100;
pub const MAX_ACQUIRE_ATTEMPTS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostLockOutcome {
    Created,
    ReclaimedDead,
    ReclaimedCorrupt,
    ReclaimedForeign,
}

#[derive(Debug)]
pub struct HostLock {
    path: PathBuf,
    pid: u32,
    _file: File,
}

#[derive(Debug)]
pub struct HostLockAcquisition {
    pub outcome: HostLockOutcome,
    pub lock: HostLock,
    pub previous_pid: Option<u32>,
}

impl HostLock {
    pub fn acquire(path: &Path) -> io::Result<Self> {
        Ok(acquire_host_lock(path, std::process::id())?.lock)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn release(&self) -> io::Result<()> {
        // A stale handle from an earlier process/generation must never delete a
        // newer owner's lock after Android process recreation.
        if read_lock_pid(&self.path).ok() == Some(self.pid) {
            match fs::remove_file(&self.path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

impl Drop for HostLock {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

pub fn read_lock_pid(path: &Path) -> io::Result<u32> {
    let pid = fs::read_to_string(path)?
        .trim()
        .parse::<u32>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid lock pid"))?;
    if pid == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid zero lock pid",
        ));
    }
    Ok(pid)
}

fn try_create_lock(path: &Path, pid: u32) -> io::Result<Option<File>> {
    match OpenOptions::new().create_new(true).write(true).open(path) {
        Ok(mut file) => {
            writeln!(file, "{pid}")?;
            file.sync_all()?;
            Ok(Some(file))
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(None),
        Err(error) => Err(error),
    }
}


pub fn read_process_command(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let bytes = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        if bytes.is_empty() {
            return None;
        }
        let value = String::from_utf8_lossy(&bytes)
            .replace('\0', " ")
            .trim()
            .to_string();
        return (!value.is_empty()).then_some(value);
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

pub fn is_android_fabushi_host_process(pid: u32) -> bool {
    read_process_command(pid).is_some_and(|command| {
        let normalized = command.to_ascii_lowercase();
        normalized.contains("fabushi")
            || normalized.contains("mahayana")
            || normalized.contains("ombhrum")
    })
}

pub fn default_is_process_alive(pid: u32) -> bool {
    // Android/Linux exposes a live process through procfs. We intentionally do
    // not signal or terminate it: a second Host owner must fail closed.
    Path::new("/proc").join(pid.to_string()).exists()
}

pub fn acquire_host_lock_with_identity<IsAlive, IsHost>(
    path: &Path,
    pid: u32,
    mut is_alive: IsAlive,
    mut is_host_process: IsHost,
) -> io::Result<HostLockAcquisition>
where
    IsAlive: FnMut(u32) -> bool,
    IsHost: FnMut(u32) -> bool,
{
    if pid == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "pid must be non-zero"));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut outcome = HostLockOutcome::Created;
    let mut previous_pid = None;

    for _ in 0..MAX_ACQUIRE_ATTEMPTS {
        if let Some(file) = try_create_lock(path, pid)? {
            return Ok(HostLockAcquisition {
                outcome,
                lock: HostLock {
                    path: path.to_path_buf(),
                    pid,
                    _file: file,
                },
                previous_pid,
            });
        }

        match read_lock_pid(path) {
            Ok(holder) if holder == pid => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "Host lock is already held by this process",
                ));
            }
            Ok(holder) if is_alive(holder) && is_host_process(holder) => {
                // Desktop may terminate/take over a known sibling Host. Android delegates
                // process ownership to the OS and must never signal another live app process.
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("Host lock is held by live Fabushi Host process {holder}"),
                ));
            }
            Ok(holder) if is_alive(holder) => {
                // PID reuse or an unrelated process must not permanently wedge the app.
                // Reclaim only the lock file; never signal the foreign process.
                outcome = HostLockOutcome::ReclaimedForeign;
                previous_pid = Some(holder);
            }
            Ok(holder) => {
                outcome = HostLockOutcome::ReclaimedDead;
                previous_pid = Some(holder);
            }
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                outcome = HostLockOutcome::ReclaimedCorrupt;
                previous_pid = None;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }

        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::WouldBlock,
        "Host lock could not be acquired after bounded stale-lock recovery",
    ))
}

pub fn acquire_host_lock_with<IsAlive>(
    path: &Path,
    pid: u32,
    is_alive: IsAlive,
) -> io::Result<HostLockAcquisition>
where
    IsAlive: FnMut(u32) -> bool,
{
    acquire_host_lock_with_identity(path, pid, is_alive, |_| true)
}

pub fn acquire_host_lock(path: &Path, pid: u32) -> io::Result<HostLockAcquisition> {
    acquire_host_lock_with_identity(
        path,
        pid,
        default_is_process_alive,
        is_android_fabushi_host_process,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-host-lock-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn process_death_stale_pid_is_reclaimed() {
        let path = temp_path("dead");
        fs::write(&path, "777").unwrap();
        let acquired = acquire_host_lock_with(&path, 888, |pid| pid != 777).unwrap();
        assert_eq!(acquired.outcome, HostLockOutcome::ReclaimedDead);
        assert_eq!(acquired.previous_pid, Some(777));
        assert_eq!(read_lock_pid(&path).unwrap(), 888);
        drop(acquired);
        assert!(!path.exists());
    }

    #[test]
    fn live_holder_is_never_terminated_or_replaced() {
        let path = temp_path("live");
        fs::write(&path, "777").unwrap();
        let error = acquire_host_lock_with(&path, 888, |_| true).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(read_lock_pid(&path).unwrap(), 777);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn corrupt_lock_is_reclaimed_but_old_handle_cannot_delete_new_owner() {
        let path = temp_path("corrupt");
        fs::write(&path, "not-a-pid").unwrap();
        let acquired = acquire_host_lock_with(&path, 888, |_| false).unwrap();
        assert_eq!(acquired.outcome, HostLockOutcome::ReclaimedCorrupt);

        fs::write(&path, "999").unwrap();
        acquired.lock.release().unwrap();
        assert_eq!(read_lock_pid(&path).unwrap(), 999);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn foreign_live_pid_is_reclaimed_without_signalling_or_replacing_a_real_host() {
        let path = temp_path("foreign");
        fs::write(&path, "777").unwrap();
        let acquired =
            acquire_host_lock_with_identity(&path, 888, |_| true, |_| false).unwrap();
        assert_eq!(acquired.outcome, HostLockOutcome::ReclaimedForeign);
        assert_eq!(acquired.previous_pid, Some(777));
        assert_eq!(read_lock_pid(&path).unwrap(), 888);
        drop(acquired);
        assert!(!path.exists());

        fs::write(&path, "999").unwrap();
        let error =
            acquire_host_lock_with_identity(&path, 888, |_| true, |_| true).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(read_lock_pid(&path).unwrap(), 999);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn duplicate_owner_in_same_process_fails_closed() {
        let path = temp_path("same");
        let acquired = acquire_host_lock_with(&path, 888, |_| true).unwrap();
        let error = acquire_host_lock_with(&path, 888, |_| true).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        drop(acquired);
    }
}
