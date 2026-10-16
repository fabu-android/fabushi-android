use std::collections::BTreeMap;

use serde_json::{Map, Value};

pub const BOX_STORE_MANIFEST_REL_PATH: &str = "manifest.json";
pub const BOX_STORE_BLOBS_PREFIX: &str = "blobs";
pub const BOX_STORE_LEGACY_MANIFEST_VERSION: u64 = 1;
pub const BOX_STORE_MANIFEST_VERSION: u64 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoxStoreManifestEntry {
    LegacyFile { sha: String, size: u64 },
    File { sha: String, size: u64, mode: u32 },
    Symlink { target: String },
}

impl BoxStoreManifestEntry {
    pub fn is_file(&self) -> bool {
        !matches!(self, Self::Symlink { .. })
    }

    pub fn is_symlink(&self) -> bool {
        matches!(self, Self::Symlink { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoxStoreManifest {
    pub version: u64,
    pub updated_at_ms: u64,
    pub writer_window_id: Option<String>,
    pub fully_hydrated: Option<bool>,
    pub entries: BTreeMap<String, BoxStoreManifestEntry>,
}

fn non_empty_string(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)?
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn nonnegative_integer(map: &Map<String, Value>, key: &str) -> Option<u64> {
    map.get(key)?.as_u64()
}

pub fn parse_manifest_entry(value: &Value) -> Option<BoxStoreManifestEntry> {
    let map = value.as_object()?;
    match map.get("kind") {
        Some(Value::String(kind)) if kind == "file" => {
            let sha = non_empty_string(map, "sha")?;
            let size = nonnegative_integer(map, "size")?;
            let mode = nonnegative_integer(map, "mode")?;
            if mode > 0o777 {
                return None;
            }
            Some(BoxStoreManifestEntry::File {
                sha,
                size,
                mode: mode as u32,
            })
        }
        Some(Value::String(kind)) if kind == "symlink" => Some(
            BoxStoreManifestEntry::Symlink {
                target: non_empty_string(map, "target")?,
            },
        ),
        Some(_) => None,
        None => {
            if map.contains_key("mode") {
                return None;
            }
            Some(BoxStoreManifestEntry::LegacyFile {
                sha: non_empty_string(map, "sha")?,
                size: nonnegative_integer(map, "size")?,
            })
        }
    }
}

pub fn is_symlink_manifest_value(value: &Value) -> bool {
    matches!(
        parse_manifest_entry(value),
        Some(BoxStoreManifestEntry::Symlink { .. })
    )
}

pub fn box_store_manifest_entries_equal(
    left: Option<&BoxStoreManifestEntry>,
    right: Option<&BoxStoreManifestEntry>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

pub fn parse_box_store_manifest(value: &Value) -> Option<BoxStoreManifest> {
    let map = value.as_object()?;
    let version = map.get("version")?.as_u64()?;
    if version != BOX_STORE_LEGACY_MANIFEST_VERSION && version != BOX_STORE_MANIFEST_VERSION {
        return None;
    }
    let updated_at_ms = map.get("updatedAtMs")?.as_u64()?;
    let writer_window_id = match map.get("writerWindowId") {
        None => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(_) => return None,
    };
    let fully_hydrated = match map.get("fullyHydrated") {
        None => None,
        Some(Value::Bool(value)) => Some(*value),
        Some(_) => return None,
    };
    let raw_entries = map.get("entries")?.as_object()?;
    let mut entries = BTreeMap::new();
    for (path, value) in raw_entries {
        let entry = parse_manifest_entry(value)?;
        if version == BOX_STORE_LEGACY_MANIFEST_VERSION
            && !matches!(entry, BoxStoreManifestEntry::LegacyFile { .. })
        {
            return None;
        }
        entries.insert(path.clone(), entry);
    }
    Some(BoxStoreManifest {
        version,
        updated_at_ms,
        writer_window_id,
        fully_hydrated,
        entries,
    })
}

pub fn manifest_to_value(manifest: &BoxStoreManifest) -> Value {
    let entries = manifest
        .entries
        .iter()
        .map(|(path, entry)| {
            let value = match entry {
                BoxStoreManifestEntry::LegacyFile { sha, size } => serde_json::json!({
                    "sha": sha,
                    "size": size,
                }),
                BoxStoreManifestEntry::File { sha, size, mode } => serde_json::json!({
                    "kind": "file",
                    "sha": sha,
                    "size": size,
                    "mode": mode,
                }),
                BoxStoreManifestEntry::Symlink { target } => serde_json::json!({
                    "kind": "symlink",
                    "target": target,
                }),
            };
            (path.clone(), value)
        })
        .collect::<Map<String, Value>>();
    let mut value = serde_json::json!({
        "version": manifest.version,
        "updatedAtMs": manifest.updated_at_ms,
        "entries": entries,
    });
    let map = value.as_object_mut().expect("manifest JSON is an object");
    if let Some(writer) = manifest.writer_window_id.as_ref() {
        map.insert("writerWindowId".into(), Value::String(writer.clone()));
    }
    if let Some(fully_hydrated) = manifest.fully_hydrated {
        map.insert("fullyHydrated".into(), Value::Bool(fully_hydrated));
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_v2_file_symlink_mode_and_hydration_round_trip() {
        let value = serde_json::json!({
            "version": 2,
            "updatedAtMs": 17,
            "writerWindowId": "runner-window",
            "fullyHydrated": true,
            "entries": {
                "workspace/run.sh": {
                    "kind": "file",
                    "sha": "abc",
                    "size": 3,
                    "mode": 493
                },
                "home/box/cli-config/current": {
                    "kind": "symlink",
                    "target": "profiles/default"
                }
            }
        });
        let parsed = parse_box_store_manifest(&value).unwrap();
        assert_eq!(parsed.version, BOX_STORE_MANIFEST_VERSION);
        assert_eq!(parsed.fully_hydrated, Some(true));
        assert_eq!(
            parsed.entries["workspace/run.sh"],
            BoxStoreManifestEntry::File {
                sha: "abc".into(),
                size: 3,
                mode: 0o755,
            }
        );
        assert_eq!(parse_box_store_manifest(&manifest_to_value(&parsed)), Some(parsed));
    }

    #[test]
    fn legacy_manifest_rejects_v2_entries_and_v2_rejects_invalid_mode() {
        let legacy = serde_json::json!({
            "version": 1,
            "updatedAtMs": 1,
            "entries": {
                "workspace/a": {"kind":"file","sha":"abc","size":1,"mode":420}
            }
        });
        assert!(parse_box_store_manifest(&legacy).is_none());

        let invalid_mode = serde_json::json!({
            "version": 2,
            "updatedAtMs": 1,
            "entries": {
                "workspace/a": {"kind":"file","sha":"abc","size":1,"mode":512}
            }
        });
        assert!(parse_box_store_manifest(&invalid_mode).is_none());
    }
}
