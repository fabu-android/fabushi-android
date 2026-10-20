use serde::{Deserialize, Serialize};
use std::{fs::{self, File}, io::{self, Write}, path::{Path, PathBuf}, sync::Arc};

use crate::durable_file_policy::SAND_UPGRADE_RESUME_FILE_NAME;

const ANDROID_UPGRADE_LIFECYCLE_VERSION: u32 = 1;

pub trait AndroidUpgradeHooks: Send + Sync {
    fn suspend_automation_wakes(&self) -> Result<(), String>;
    fn prepare_sharing_for_upgrade(&self) -> Result<(), String>;
    fn quiesce_transcript_for_upgrade(&self) -> Result<(), String>;
    fn resume_interrupted_upgrade_turns(&self) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AndroidUpgradeResumeMarker {
    version: u32,
    pub update_id: String,
    pub from_version: String,
    pub prepared_at_ms: i64,
}

pub struct AndroidUpgradeLifecycle {
    marker_path: PathBuf,
    hooks: Arc<dyn AndroidUpgradeHooks>,
}

impl AndroidUpgradeLifecycle {
    pub fn new(app_data_dir: impl AsRef<Path>, hooks: Arc<dyn AndroidUpgradeHooks>) -> Self {
        Self { marker_path: app_data_dir.as_ref().join(SAND_UPGRADE_RESUME_FILE_NAME), hooks }
    }

    pub fn marker_path(&self) -> &Path { &self.marker_path }

    pub fn pending_resume(&self) -> io::Result<Option<AndroidUpgradeResumeMarker>> {
        match fs::read(&self.marker_path) {
            Ok(bytes) => {
                let marker: AndroidUpgradeResumeMarker = serde_json::from_slice(&bytes)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, format!("invalid Android upgrade resume marker: {error}")))?;
                if marker.version != ANDROID_UPGRADE_LIFECYCLE_VERSION
                    || marker.update_id.trim().is_empty()
                    || marker.from_version.trim().is_empty()
                {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid Android upgrade resume marker contract"));
                }
                Ok(Some(marker))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn prepare_for_package_upgrade(
        &self,
        update_id: &str,
        from_version: &str,
        prepared_at_ms: i64,
    ) -> Result<AndroidUpgradeResumeMarker, String> {
        let update_id = update_id.trim();
        let from_version = from_version.trim();
        if update_id.is_empty() || from_version.is_empty() || prepared_at_ms < 0 {
            return Err("Android package upgrade requires update id, source version and timestamp".into());
        }
        if let Some(existing) = self.pending_resume()
            .map_err(|error| format!("failed to read Android upgrade resume marker: {error}"))?
        {
            if existing.update_id == update_id && existing.from_version == from_version {
                return Ok(existing);
            }
            return Err("another Android package upgrade is already awaiting recovery".into());
        }

        self.hooks.suspend_automation_wakes()?;
        self.hooks.prepare_sharing_for_upgrade()?;
        self.hooks.quiesce_transcript_for_upgrade()?;

        let marker = AndroidUpgradeResumeMarker {
            version: ANDROID_UPGRADE_LIFECYCLE_VERSION,
            update_id: update_id.to_string(),
            from_version: from_version.to_string(),
            prepared_at_ms,
        };
        self.persist_marker(&marker)
            .map_err(|error| format!("failed to persist Android upgrade resume marker: {error}"))?;
        Ok(marker)
    }

    pub fn recover_after_package_update(&self) -> Result<bool, String> {
        let Some(_marker) = self.pending_resume()
            .map_err(|error| format!("failed to read Android upgrade resume marker: {error}"))?
        else { return Ok(false); };

        self.hooks.resume_interrupted_upgrade_turns()?;
        fs::remove_file(&self.marker_path)
            .map_err(|error| format!("failed to clear Android upgrade resume marker: {error}"))?;
        Ok(true)
    }

    fn persist_marker(&self, marker: &AndroidUpgradeResumeMarker) -> io::Result<()> {
        let parent = self.marker_path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "upgrade marker path has no parent")
        })?;
        fs::create_dir_all(parent)?;
        let bytes = serde_json::to_vec(marker).map_err(io::Error::other)?;
        let part = self.marker_path.with_extension(format!("part-{}", std::process::id()));
        let result = (|| -> io::Result<()> {
            let mut file = File::create(&part)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&part, &self.marker_path)?;
            Ok(())
        })();
        if result.is_err() { let _ = fs::remove_file(&part); }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::{AtomicBool, AtomicUsize, Ordering}, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Default)]
    struct Hooks {
        calls: Mutex<Vec<&'static str>>,
        resume_attempts: AtomicUsize,
        fail_resume: AtomicBool,
    }
    impl AndroidUpgradeHooks for Hooks {
        fn suspend_automation_wakes(&self) -> Result<(), String> { self.calls.lock().unwrap().push("automation"); Ok(()) }
        fn prepare_sharing_for_upgrade(&self) -> Result<(), String> { self.calls.lock().unwrap().push("sharing"); Ok(()) }
        fn quiesce_transcript_for_upgrade(&self) -> Result<(), String> { self.calls.lock().unwrap().push("transcript"); Ok(()) }
        fn resume_interrupted_upgrade_turns(&self) -> Result<(), String> {
            self.resume_attempts.fetch_add(1, Ordering::SeqCst);
            if self.fail_resume.load(Ordering::SeqCst) { Err("resume failed".into()) } else { Ok(()) }
        }
    }

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-android-upgrade-{label}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos()
        ))
    }

    #[test]
    fn prepare_is_ordered_durable_and_idempotent_for_same_update() {
        let dir = root("prepare");
        let hooks = Arc::new(Hooks::default());
        let lifecycle = AndroidUpgradeLifecycle::new(&dir, hooks.clone());
        let first = lifecycle.prepare_for_package_upgrade("update-7", "1.2.3", 42).unwrap();
        assert_eq!(hooks.calls.lock().unwrap().as_slice(), ["automation", "sharing", "transcript"]);
        drop(lifecycle);

        let reopened = AndroidUpgradeLifecycle::new(&dir, hooks.clone());
        assert_eq!(reopened.pending_resume().unwrap(), Some(first.clone()));
        assert_eq!(reopened.prepare_for_package_upgrade("update-7", "1.2.3", 99).unwrap(), first);
        assert!(reopened.prepare_for_package_upgrade("different", "1.2.3", 99).is_err());
        assert_eq!(hooks.calls.lock().unwrap().len(), 3);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn failed_recovery_remains_durable_until_a_later_cold_start_settles() {
        let dir = root("recover");
        let hooks = Arc::new(Hooks::default());
        let lifecycle = AndroidUpgradeLifecycle::new(&dir, hooks.clone());
        lifecycle.prepare_for_package_upgrade("update-8", "2.0.0", 100).unwrap();
        hooks.fail_resume.store(true, Ordering::SeqCst);
        assert!(lifecycle.recover_after_package_update().is_err());
        assert!(lifecycle.pending_resume().unwrap().is_some());
        drop(lifecycle);

        hooks.fail_resume.store(false, Ordering::SeqCst);
        let reopened = AndroidUpgradeLifecycle::new(&dir, hooks.clone());
        assert!(reopened.recover_after_package_update().unwrap());
        assert!(!reopened.recover_after_package_update().unwrap());
        assert_eq!(hooks.resume_attempts.load(Ordering::SeqCst), 2);
        assert!(reopened.pending_resume().unwrap().is_none());
        let _ = fs::remove_dir_all(dir);
    }
}
