use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use serde_json::json;

use super::box_store_diagnostics::report_box_store_diagnostic;

pub const INCOMPLETE_LEGACY_HYDRATE_REASON: &str = "incomplete_legacy_hydrate";
pub const BOX_STORE_HYDRATION_HANDOFF_FILE_NAME: &str =
    ".box-store-legacy-hydration-complete";
pub const BOX_STORE_HYDRATION_HANDOFF_MANIFEST_PATH: &str =
    "home/box/sand-data/.box-store-legacy-hydration-complete";


pub const COPY_IN_STATUS_THROTTLE_MS: u64 = 750;
pub const COPY_IN_STUCK_THRESHOLD_MS: u64 = 5 * 60_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CopyInProgress {
    pub total: usize,
    pub files: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopyInTraceStatus {
    pub copy_stage: Option<String>,
    pub file_entries: Option<usize>,
    pub symlink_entries: Option<usize>,
    pub symlinks_started: Option<usize>,
    pub symlinks_completed: Option<usize>,
    pub symlinks_in_flight: Option<usize>,
    pub active_symlink_steps: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyInWatchdogEvent {
    pub level: &'static str,
    pub metadata: BTreeMap<String, String>,
}

pub fn write_copy_in_status_atomic(path: &Path, status: &serde_json::Value) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp_path = PathBuf::from(format!("{}.tmp", path.display()));
    let bytes = serde_json::to_vec(status).map_err(io::Error::other)?;
    let result = (|| -> io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temp_path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp_path, path)?;
        if let Some(parent) = path.parent() {
            sync_directory(parent)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp_path);
    }
    result
}

pub fn write_copy_in_status_throttled(
    path: &Path,
    status: &serde_json::Value,
    now_ms: u64,
    last_write_ms: &mut Option<u64>,
    force: bool,
) -> io::Result<bool> {
    if !force
        && last_write_ms
            .is_some_and(|last| now_ms.saturating_sub(last) < COPY_IN_STATUS_THROTTLE_MS)
    {
        return Ok(false);
    }
    write_copy_in_status_atomic(path, status)?;
    *last_write_ms = Some(now_ms);
    Ok(true)
}

pub fn build_copy_in_watchdog_event(
    progressed_since_last_tick: bool,
    progress: CopyInProgress,
    elapsed_ms: u64,
    threshold_ms: u64,
    trace: Option<&CopyInTraceStatus>,
) -> CopyInWatchdogEvent {
    let mut metadata = BTreeMap::from([
        (
            "outcome".into(),
            if progressed_since_last_tick { "slow" } else { "stuck" }.into(),
        ),
        (
            "reason".into(),
            if progressed_since_last_tick {
                "moving-data-in-slow-but-advancing"
            } else {
                "moving-data-in-exceeded-threshold"
            }
            .into(),
        ),
        ("manifest_entries".into(), progress.total.to_string()),
        ("files".into(), progress.files.to_string()),
        ("bytes".into(), progress.bytes.to_string()),
        ("duration_ms".into(), elapsed_ms.to_string()),
        ("threshold_ms".into(), threshold_ms.to_string()),
    ]);
    if let Some(trace) = trace {
        if let Some(stage) = trace.copy_stage.as_ref() {
            metadata.insert("stage".into(), stage.clone());
        }
        if let Some(value) = trace.file_entries {
            metadata.insert("file_entries".into(), value.to_string());
        }
        if let Some(value) = trace.symlink_entries {
            metadata.insert("symlink_entries".into(), value.to_string());
        }
        if let Some(value) = trace.symlinks_started {
            metadata.insert("symlinks_started".into(), value.to_string());
        }
        if let Some(value) = trace.symlinks_completed {
            metadata.insert("symlinks_completed".into(), value.to_string());
        }
        if let Some(value) = trace.symlinks_in_flight {
            metadata.insert("symlinks_in_flight".into(), value.to_string());
        }
        let active = trace
            .active_symlink_steps
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(step, count)| format!("{step}:{count}"))
            .collect::<Vec<_>>()
            .join(",");
        if !active.is_empty() {
            metadata.insert("active_symlink_steps".into(), active);
        }
    }
    CopyInWatchdogEvent {
        level: if progressed_since_last_tick { "info" } else { "warn" },
        metadata,
    }
}

pub fn resolve_copy_in_download_owner_values(
    effective_uid: u32,
    home_uid: u32,
    home_gid: u32,
) -> Option<(u32, u32)> {
    (effective_uid == 0 && home_uid > 0).then_some((home_uid, home_gid))
}

#[cfg(unix)]
pub fn resolve_copy_in_download_owner(home_root: &Path) -> Option<(u32, u32)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::metadata(home_root).ok()?;
    let effective_uid = fs::metadata("/proc/self").ok()?.uid();
    resolve_copy_in_download_owner_values(effective_uid, metadata.uid(), metadata.gid())
}

#[cfg(not(unix))]
pub fn resolve_copy_in_download_owner(_home_root: &Path) -> Option<(u32, u32)> {
    None
}

pub fn apply_copy_in_download_owner(path: &Path, owner: Option<(u32, u32)>) -> io::Result<()> {
    let Some((uid, gid)) = owner else {
        return Ok(());
    };
    #[cfg(unix)]
    {
        std::os::unix::fs::chown(path, Some(uid), Some(gid))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, uid, gid);
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HydrationEvidence {
    pub failures: Option<Vec<String>>,
    pub manifest_entries: Option<u64>,
    pub files: Option<u64>,
    pub verified: Option<u64>,
    pub hydrate_source: Option<String>,
    pub authoritative_store_db_entries: Option<u64>,
    pub restored_store_db_entries: Option<u64>,
}

pub fn is_hydration_handoff_manifest_path(rel_path: &str) -> bool {
    rel_path == BOX_STORE_HYDRATION_HANDOFF_MANIFEST_PATH
        || (rel_path.starts_with(&format!(
            "{BOX_STORE_HYDRATION_HANDOFF_MANIFEST_PATH}."
        )) && rel_path.ends_with(".tmp"))
}

pub fn is_box_store_fully_hydrated(evidence: Option<&HydrationEvidence>) -> bool {
    let Some(evidence) = evidence else {
        return false;
    };
    let Some(failures) = evidence.failures.as_ref() else {
        return false;
    };
    if !failures.is_empty()
        || evidence.manifest_entries.is_none()
        || evidence.files.is_none()
        || evidence.verified.is_none()
    {
        return false;
    }

    if evidence.hydrate_source.as_deref() == Some("legacy") {
        let (Some(authoritative), Some(restored)) = (
            evidence.authoritative_store_db_entries,
            evidence.restored_store_db_entries,
        ) else {
            return false;
        };
        return restored >= authoritative;
    }

    evidence.files == evidence.manifest_entries
        && evidence.verified == evidence.manifest_entries
}

fn marker_parent(marker_path: &Path) -> &Path {
    marker_path.parent().unwrap_or_else(|| Path::new("."))
}

fn temp_marker_path(marker_path: &Path) -> PathBuf {
    PathBuf::from(format!(
        "{}.{}.tmp",
        marker_path.display(),
        std::process::id()
    ))
}

fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

pub fn write_hydration_handoff_marker(marker_path: impl AsRef<Path>) -> io::Result<()> {
    let marker_path = marker_path.as_ref();
    let parent = marker_parent(marker_path);
    fs::create_dir_all(parent)?;
    let temp = temp_marker_path(marker_path);

    let result = (|| -> io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temp)?;
        file.write_all(b"complete\n")?;
        file.sync_all()?;
        drop(file);

        fs::rename(&temp, marker_path)?;
        sync_directory(parent)
    })();

    if let Err(error) = fs::remove_file(&temp) {
        if error.kind() != io::ErrorKind::NotFound {
            if let Some(fields) = json!({
                "extension": "box_store",
                "kind": "hydration_temp_cleanup_failed",
                "errorClass": format!("{:?}", error.kind()),
            })
            .as_object()
            {
                report_box_store_diagnostic(fields);
            }
        }
    }
    result
}

pub fn remove_hydration_handoff_marker(marker_path: impl AsRef<Path>) -> io::Result<()> {
    let marker_path = marker_path.as_ref();
    match fs::remove_file(marker_path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let parent = marker_parent(marker_path);
    fs::create_dir_all(parent)?;
    sync_directory(parent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-runner-hydration-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn complete_evidence_matches_desktop_legacy_and_v2_rules() {
        assert!(!is_box_store_fully_hydrated(None));
        let v2 = HydrationEvidence {
            failures: Some(vec![]),
            manifest_entries: Some(2),
            files: Some(2),
            verified: Some(2),
            ..HydrationEvidence::default()
        };
        assert!(is_box_store_fully_hydrated(Some(&v2)));

        let legacy_incomplete = HydrationEvidence {
            failures: Some(vec![]),
            manifest_entries: Some(2),
            files: Some(2),
            verified: Some(2),
            hydrate_source: Some("legacy".into()),
            authoritative_store_db_entries: Some(3),
            restored_store_db_entries: Some(2),
        };
        assert!(!is_box_store_fully_hydrated(Some(&legacy_incomplete)));
        let legacy_complete = HydrationEvidence {
            restored_store_db_entries: Some(3),
            ..legacy_incomplete
        };
        assert!(is_box_store_fully_hydrated(Some(&legacy_complete)));
    }

    #[test]
    fn status_throttle_watchdog_and_download_owner_match_frozen_contract() {
        let root = root("status");
        let status = root.join("copy-in-status.json");
        let mut last = None;
        assert!(write_copy_in_status_throttled(
            &status,
            &json!({"phase":"copying","restored":1}),
            1_000,
            &mut last,
            false,
        ).unwrap());
        assert!(!write_copy_in_status_throttled(
            &status,
            &json!({"phase":"copying","restored":2}),
            1_000 + COPY_IN_STATUS_THROTTLE_MS - 1,
            &mut last,
            false,
        ).unwrap());
        assert!(write_copy_in_status_throttled(
            &status,
            &json!({"phase":"copying","restored":3}),
            1_000 + COPY_IN_STATUS_THROTTLE_MS,
            &mut last,
            false,
        ).unwrap());

        let trace = CopyInTraceStatus {
            copy_stage: Some("symlink".into()),
            file_entries: Some(3),
            symlink_entries: Some(2),
            symlinks_started: Some(2),
            symlinks_completed: Some(1),
            symlinks_in_flight: Some(1),
            active_symlink_steps: BTreeMap::from([("restore".into(), 1)]),
        };
        let stuck = build_copy_in_watchdog_event(
            false,
            CopyInProgress { total: 5, files: 3, bytes: 99 },
            300_000,
            300_000,
            Some(&trace),
        );
        assert_eq!(stuck.level, "warn");
        assert_eq!(stuck.metadata.get("outcome").map(String::as_str), Some("stuck"));
        assert_eq!(stuck.metadata.get("stage").map(String::as_str), Some("symlink"));
        assert_eq!(
            stuck.metadata.get("active_symlink_steps").map(String::as_str),
            Some("restore:1")
        );
        let slow = build_copy_in_watchdog_event(
            true,
            CopyInProgress { total: 5, files: 4, bytes: 120 },
            600_000,
            300_000,
            Some(&trace),
        );
        assert_eq!(slow.level, "info");
        assert_eq!(slow.metadata.get("outcome").map(String::as_str), Some("slow"));

        assert_eq!(resolve_copy_in_download_owner_values(0, 501, 20), Some((501, 20)));
        assert_eq!(resolve_copy_in_download_owner_values(0, 0, 0), None);
        assert_eq!(resolve_copy_in_download_owner_values(501, 501, 20), None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn hydration_marker_is_atomic_and_removable() {
        let root = root("marker");
        let marker = root.join(BOX_STORE_HYDRATION_HANDOFF_FILE_NAME);
        write_hydration_handoff_marker(&marker).unwrap();
        assert_eq!(fs::read_to_string(&marker).unwrap(), "complete\n");
        assert!(
            fs::read_dir(&root)
                .unwrap()
                .all(|entry| !entry.unwrap().file_name().to_string_lossy().ends_with(".tmp"))
        );
        remove_hydration_handoff_marker(&marker).unwrap();
        assert!(!marker.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn hydration_handoff_path_fences_temporary_marker_variants() {
        assert!(is_hydration_handoff_manifest_path(
            BOX_STORE_HYDRATION_HANDOFF_MANIFEST_PATH
        ));
        assert!(is_hydration_handoff_manifest_path(
            "home/box/sand-data/.box-store-legacy-hydration-complete.42.tmp"
        ));
        assert!(!is_hydration_handoff_manifest_path(
            "home/box/sand-data/other.tmp"
        ));
    }
}
