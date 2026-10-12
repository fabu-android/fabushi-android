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
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let owner_auth_id = item
            .get("ownerAuthId")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let tombstone = XuserRoomTombstone {
            room_id: room_id.into(),
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

fn persist_atomic(path: &Path, map: &BTreeMap<String, XuserRoomTombstone>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = PathBuf::from(format!("{}.tmp-{}", path.display(), std::process::id()));
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
            .map_err(|_| io::Error::other("xuser tombstone store lock poisoned"))?;
        if guard.is_none() {
            *guard = Some(parse_tombstone_file(
                fs::read_to_string(&self.file_path).ok().as_deref(),
            ));
        }
        let map = guard.as_mut().expect("tombstone cache initialized");
        let previous = map.clone();
        let (output, changed) = mutate(map);
        if changed {
            if let Err(error) = persist_atomic(&self.file_path, map) {
                *map = previous;
                return Err(error);
            }
        }
        Ok(output)
    }

    pub fn list(&self) -> io::Result<Vec<XuserRoomTombstone>> {
        self.with_map(|map| (map.values().cloned().collect(), false))
    }

    pub fn record(&self, value: XuserRoomTombstone) -> io::Result<bool> {
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
        let key = format!("{}\0{}", owner_auth_id.unwrap_or(""), room_id);
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
                .as_nanos()
        ))
    }

    #[test]
    fn tombstones_are_durable_account_scoped_and_atomic() {
        let root = root("reopen");
        let a = XuserRoomTombstone {
            room_id: "room-a".into(),
            owner_auth_id: Some("owner-a".into()),
            torn_down_at_ms: 10,
        };
        let b = XuserRoomTombstone {
            room_id: "room-a".into(),
            owner_auth_id: Some("owner-b".into()),
            torn_down_at_ms: 20,
        };
        {
            let store = SandXuserRoomTombstoneStore::new(&root);
            assert!(store.record(a.clone()).unwrap());
            assert!(!store.record(a.clone()).unwrap());
            assert!(store.record(b.clone()).unwrap());
            assert_eq!(store.list().unwrap().len(), 2);
            assert!(fs::read_dir(&root).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains(".tmp-")
            }));
        }
        let reopened = SandXuserRoomTombstoneStore::new(&root);
        assert_eq!(reopened.list().unwrap().len(), 2);
        assert!(reopened.clear("room-a", Some("owner-a")).unwrap());
        assert_eq!(reopened.list().unwrap(), vec![b]);
        let _ = fs::remove_dir_all(root);
    }
}
