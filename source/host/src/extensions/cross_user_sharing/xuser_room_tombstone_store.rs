use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const SAND_XUSER_ROOM_TOMBSTONE_FILE_NAME: &str = "host-xuser-room-tombstones.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XuserRoomTombstone {
    pub room_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_auth_id: Option<String>,
    pub torn_down_at_ms: u64,
}

pub fn tombstone_key(value: &XuserRoomTombstone) -> String {
    format!(
        "{}\0{}",
        value.owner_auth_id.as_deref().unwrap_or(""),
        value.room_id
    )
}

pub fn parse_tombstone_file(raw: Option<&str>) -> BTreeMap<String, XuserRoomTombstone> {
    let mut map = BTreeMap::new();
    let Some(raw) = raw else { return map };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return map;
    };
    let Some(items) = value.get("tombstones").and_then(|value| value.as_array()) else {
        return map;
    };
    for item in items {
        let Some(room_id) = item
            .get("roomId")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let owner_auth_id = item
            .get("ownerAuthId")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let tombstone = XuserRoomTombstone {
            room_id: room_id.to_string(),
            owner_auth_id,
            torn_down_at_ms: item
                .get("tornDownAtMs")
                .and_then(|value| value.as_u64())
                .unwrap_or(0),
        };
        map.insert(tombstone_key(&tombstone), tombstone);
    }
    map
}

fn persist_atomic(
    path: &Path,
    map: &BTreeMap<String, XuserRoomTombstone>,
) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("json.tmp-{}", std::process::id()));
    let values = map.values().cloned().collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&serde_json::json!({
        "version": 1,
        "tombstones": values,
    }))
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub struct SandXuserRoomTombstoneStore {
    pub file_path: PathBuf,
    cache: Mutex<Option<BTreeMap<String, XuserRoomTombstone>>>,
}

impl SandXuserRoomTombstoneStore {
    pub fn new(root: &Path) -> Self {
        Self {
            file_path: root.join(SAND_XUSER_ROOM_TOMBSTONE_FILE_NAME),
            cache: Mutex::new(None),
        }
    }

    fn with_map<R>(
        &self,
        mutate: impl FnOnce(&mut BTreeMap<String, XuserRoomTombstone>) -> (R, bool),
    ) -> io::Result<R> {
        let mut guard = self
            .cache
            .lock()
            .map_err(|_| io::Error::other("xuser room tombstone lock poisoned"))?;
        if guard.is_none() {
            *guard = Some(parse_tombstone_file(
                fs::read_to_string(&self.file_path).ok().as_deref(),
            ));
        }
        let map = guard.as_mut().expect("tombstone cache initialized");
        let previous = map.clone();
        let (out, changed) = mutate(map);
        if changed {
            if let Err(error) = persist_atomic(&self.file_path, map) {
                *map = previous;
                return Err(error);
            }
        }
        Ok(out)
    }

    pub fn list(&self) -> io::Result<Vec<XuserRoomTombstone>> {
        self.with_map(|map| (map.values().cloned().collect(), false))
    }

    pub fn record(&self, value: XuserRoomTombstone) -> io::Result<bool> {
        if value.room_id.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "xuser tombstone room id is required",
            ));
        }
        self.with_map(|map| {
            let key = tombstone_key(&value);
            if map.contains_key(&key) {
                (false, false)
            } else {
                map.insert(key, value);
                (true, true)
            }
        })
    }

    pub fn clear(&self, room_id: &str, owner_auth_id: Option<&str>) -> io::Result<bool> {
        let room_id = room_id.trim();
        if room_id.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "xuser tombstone room id is required",
            ));
        }
        let key = format!("{}\0{}", owner_auth_id.unwrap_or("").trim(), room_id);
        self.with_map(|map| {
            let removed = map.remove(&key).is_some();
            (removed, removed)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-xuser-tombstone-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        ))
    }

    fn tombstone(room: &str, owner: Option<&str>, torn_down_at_ms: u64) -> XuserRoomTombstone {
        XuserRoomTombstone {
            room_id: room.into(),
            owner_auth_id: owner.map(str::to_string),
            torn_down_at_ms,
        }
    }

    #[test]
    fn tombstone_survives_reopen_and_owner_scope_is_independent() {
        let root = root("reopen");
        let store = SandXuserRoomTombstoneStore::new(&root);
        assert!(store.record(tombstone("room-a", Some("owner-a"), 10)).unwrap());
        assert!(!store.record(tombstone("room-a", Some("owner-a"), 20)).unwrap());
        assert!(store.record(tombstone("room-a", Some("owner-b"), 30)).unwrap());
        drop(store);

        let reopened = SandXuserRoomTombstoneStore::new(&root);
        let listed = reopened.list().unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().any(|value| value.owner_auth_id.as_deref() == Some("owner-a")));
        assert!(listed.iter().any(|value| value.owner_auth_id.as_deref() == Some("owner-b")));
        assert!(reopened.clear("room-a", Some("owner-a")).unwrap());
        assert_eq!(reopened.list().unwrap().len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_room_fails_closed_and_atomic_write_leaves_no_temp_file() {
        let root = root("atomic");
        let store = SandXuserRoomTombstoneStore::new(&root);
        assert!(store.record(tombstone(" ", None, 1)).is_err());
        assert!(store.record(tombstone("room-ok", None, 1)).unwrap());
        assert!(
            fs::read_dir(&root)
                .unwrap()
                .all(|entry| !entry.unwrap().file_name().to_string_lossy().contains("tmp-")),
        );
        let _ = fs::remove_dir_all(root);
    }
}
