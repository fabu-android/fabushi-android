use std::path::{Path, PathBuf};

use super::disk_pressure_guard::{
    read_disk_volume_snapshots, DiskVolumeRoot, DiskVolumeSnapshot,
};

/// Android replacement boundary for Desktop ForeverBox platform ownership.
///
/// Android never exposes an arbitrary local desktop/container. Local execution remains bounded by
/// AndroidLocalRunner and desktop-class Box work is delegated only to AuthorizedRemoteHostRunner.
/// This adapter owns only the Android app-private storage-volume sampling needed by the Host.
#[derive(Debug, Clone)]
pub struct AndroidForeverBoxExtension {
    app_private_root: PathBuf,
}

impl AndroidForeverBoxExtension {
    pub fn new(app_private_root: impl Into<PathBuf>) -> Self {
        Self {
            app_private_root: app_private_root.into(),
        }
    }

    pub fn app_private_root(&self) -> &Path {
        &self.app_private_root
    }

    pub fn sample_host_volume(&self) -> Result<DiskVolumeSnapshot, String> {
        let sample = read_disk_volume_snapshots(&[DiskVolumeRoot::new(
            "android-app-private",
            self.app_private_root.clone(),
        )]);
        if !sample.complete {
            return Err("Android app-private storage volume sample is incomplete".into());
        }
        sample
            .snapshots
            .into_iter()
            .next()
            .ok_or_else(|| "Android app-private storage volume is unavailable".into())
    }

    pub const fn local_desktop_box_available(&self) -> bool {
        false
    }

    pub const fn execution_replacement(&self) -> &'static str {
        "bounded-local-runner-or-authorized-remote-runner"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_forever_box_adapter_samples_only_app_private_volume() {
        let root = tempfile::tempdir().unwrap();
        let extension = AndroidForeverBoxExtension::new(root.path());
        let snapshot = extension.sample_host_volume().unwrap();
        assert_eq!(snapshot.volume, "android-app-private");
        assert!(snapshot.total_bytes > 0);
        assert!(snapshot.available_bytes <= snapshot.total_bytes);
        assert!(!extension.local_desktop_box_available());
        assert_eq!(
            extension.execution_replacement(),
            "bounded-local-runner-or-authorized-remote-runner",
        );
    }

    #[test]
    fn missing_app_private_root_fails_closed() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-missing-forever-box-root-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let extension = AndroidForeverBoxExtension::new(root);
        assert!(extension.sample_host_volume().is_err());
    }
}
