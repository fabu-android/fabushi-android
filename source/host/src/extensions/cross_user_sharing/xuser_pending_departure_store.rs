use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const SAND_XUSER_PENDING_DEPARTURE_FILE_NAME: &str = "host-xuser-pending-departures.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum XuserDeparture {
    #[serde(rename = "leave-room")]
    LeaveRoom {
        #[serde(rename = "ownerAuthId", skip_serializing_if = "Option::is_none")]
        owner_auth_id: Option<String>,
        #[serde(rename = "roomId")]
        room_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
    },
    #[serde(rename = "remove-agent")]
    RemoveAgent {
        #[serde(rename = "ownerAuthId", skip_serializing_if = "Option::is_none")]
        owner_auth_id: Option<String>,
        #[serde(rename = "agentId")]
        agent_id: String,
    },
}

pub fn departure_key(value: &XuserDeparture) -> String {
    match value {
        XuserDeparture::LeaveRoom {
            owner_auth_id,
            room_id,
            ..
        } => format!(
            "leave-room\0{}\0{}",
            owner_auth_id.as_deref().unwrap_or(""),
            room_id
        ),
        XuserDeparture::RemoveAgent {
            owner_auth_id,
            agent_id,
        } => format!(
            "remove-agent\0{}\0{}",
            owner_auth_id.as_deref().unwrap_or(""),
            agent_id
        ),
    }
}

pub fn parse_departure_file(raw: Option<&str>) -> BTreeMap<String, XuserDeparture> {
    let mut map = BTreeMap::new();
    let Some(raw) = raw else { return map };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return map;
    };
    let Some(items) = value.get("pending").and_then(|value| value.as_array()) else {
        return map;
    };
    for item in items {
        let Some(agent_id) = item
            .get("agentId")
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
        let departure = match item.get("kind").and_then(|value| value.as_str()) {
            Some("leave-room") => {
                let Some(room_id) = item
                    .get("roomId")
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
                else {
                    continue;
                };
                XuserDeparture::LeaveRoom {
                    owner_auth_id,
                    room_id: room_id.into(),
                    agent_id: agent_id.into(),
                }
            }
            Some("remove-agent") => XuserDeparture::RemoveAgent {
                owner_auth_id,
                agent_id: agent_id.into(),
            },
            _ => continue,
        };
        map.insert(departure_key(&departure), departure);
    }
    map
}

fn persist_atomic(path: &Path, map: &BTreeMap<String, XuserDeparture>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = PathBuf::from(format!("{}.tmp-{}", path.display(), std::process::id()));
    let values = map.values().cloned().collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&serde_json::json!({"version":2,"pending":values}))
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

pub struct SandXuserPendingDepartureStore {
    pub file_path: PathBuf,
    cache: Mutex<Option<BTreeMap<String, XuserDeparture>>>,
}

impl SandXuserPendingDepartureStore {
    pub fn new(root: &Path) -> Self {
        Self {
            file_path: root.join(SAND_XUSER_PENDING_DEPARTURE_FILE_NAME),
            cache: Mutex::new(None),
        }
    }

    fn with_map<R>(
        &self,
        mutate: impl FnOnce(&mut BTreeMap<String, XuserDeparture>) -> (R, bool),
    ) -> io::Result<R> {
        let mut guard = self
            .cache
            .lock()
            .map_err(|_| io::Error::other("xuser departure store lock poisoned"))?;
        if guard.is_none() {
            *guard = Some(parse_departure_file(
                fs::read_to_string(&self.file_path).ok().as_deref(),
            ));
        }
        let map = guard.as_mut().expect("departure cache initialized");
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

    pub fn list(&self) -> io::Result<Vec<XuserDeparture>> {
        self.with_map(|map| (map.values().cloned().collect(), false))
    }

    pub fn record(&self, value: XuserDeparture) -> io::Result<bool> {
        self.with_map(|map| {
            let key = departure_key(&value);
            if map.contains_key(&key) {
                (false, false)
            } else {
                map.insert(key, value);
                (true, true)
            }
        })
    }

    pub fn clear(&self, value: &XuserDeparture) -> io::Result<bool> {
        let key = departure_key(value);
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
            "fabushi-xuser-departure-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn pending_departure_is_deduped_and_survives_reopen() {
        let root = root("reopen");
        let value = XuserDeparture::LeaveRoom {
            owner_auth_id: Some("owner-a".into()),
            room_id: "room-a".into(),
            agent_id: "agent-a".into(),
        };
        {
            let store = SandXuserPendingDepartureStore::new(&root);
            assert!(store.record(value.clone()).unwrap());
            assert!(!store.record(value.clone()).unwrap());
            assert_eq!(store.list().unwrap(), vec![value.clone()]);
        }
        let reopened = SandXuserPendingDepartureStore::new(&root);
        assert_eq!(reopened.list().unwrap(), vec![value.clone()]);
        assert!(reopened.clear(&value).unwrap());
        assert!(reopened.list().unwrap().is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn atomic_persist_leaves_no_temporary_file() {
        let root = root("atomic");
        let store = SandXuserPendingDepartureStore::new(&root);
        store
            .record(XuserDeparture::RemoveAgent {
                owner_auth_id: None,
                agent_id: "agent-a".into(),
            })
            .unwrap();
        assert!(fs::read_dir(&root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp-")
        }));
        let _ = fs::remove_dir_all(root);
    }
}
