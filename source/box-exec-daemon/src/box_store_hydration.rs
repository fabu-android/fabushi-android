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
