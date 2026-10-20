use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SAND_REMOTE_ROOM_FILENAME: &str = "remote-room.json";
pub const REMOTE_ROOM_CONFIG_VERSION: u64 = 1;
pub const REMOTE_ROOM_MAX_MEMBERS: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRoomMember {
    pub kind: String,
    pub auth_id: String,
    pub agent_id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandRemoteRoomConfig {
    pub version: u64,
    pub room_id: String,
    pub host_auth_id: String,
    pub host_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_avatar_url: Option<String>,
    pub members: Vec<RemoteRoomMember>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_revoked: Option<bool>,
}

pub fn get_sand_remote_room_path(agent_dir: impl AsRef<Path>) -> PathBuf {
    agent_dir.as_ref().join(SAND_REMOTE_ROOM_FILENAME)
}

pub fn normalize_remote_room_members(raw: &Value) -> Vec<RemoteRoomMember> {
    let Some(values) = raw.as_array() else { return Vec::new() };
    values.iter().filter_map(|value| {
        let candidate = value.as_object()?;
        let auth_id = candidate.get("authId")?.as_str()?.trim();
        if auth_id.is_empty() { return None; }
        let kind = if candidate.get("kind").and_then(Value::as_str) == Some("agent") {
            "agent"
        } else {
            "human"
        };
        Some(RemoteRoomMember {
            kind: kind.to_string(),
            auth_id: auth_id.to_string(),
            agent_id: candidate.get("agentId").and_then(Value::as_str).unwrap_or_default().trim().to_string(),
            display_name: candidate.get("displayName").and_then(Value::as_str)
                .map(str::trim).filter(|value| !value.is_empty()).unwrap_or("Someone").to_string(),
            avatar_url: candidate.get("avatarUrl").and_then(Value::as_str)
                .filter(|value| !value.is_empty()).map(ToOwned::to_owned),
        })
    }).take(REMOTE_ROOM_MAX_MEMBERS).collect()
}

pub fn read_sand_remote_room_config(agent_dir: impl AsRef<Path>) -> Option<SandRemoteRoomConfig> {
    let raw = fs::read_to_string(get_sand_remote_room_path(agent_dir)).ok()?;
    let parsed = serde_json::from_str::<Value>(&raw).ok()?;
    let object = parsed.as_object()?;
    let room_id = object.get("roomId")?.as_str()?.trim();
    let host_auth_id = object.get("hostAuthId")?.as_str()?.trim();
    if room_id.is_empty() || host_auth_id.is_empty() { return None; }
    Some(SandRemoteRoomConfig {
        version: object.get("version").and_then(Value::as_u64).unwrap_or(REMOTE_ROOM_CONFIG_VERSION),
        room_id: room_id.to_string(),
        host_auth_id: host_auth_id.to_string(),
        host_name: object.get("hostName").and_then(Value::as_str).map(str::trim)
            .filter(|value| !value.is_empty()).unwrap_or("The host").to_string(),
        host_avatar_url: object.get("hostAvatarUrl").and_then(Value::as_str)
            .filter(|value| !value.is_empty()).map(ToOwned::to_owned),
        members: normalize_remote_room_members(object.get("members").unwrap_or(&Value::Null)),
        is_revoked: (object.get("isRevoked").and_then(Value::as_bool) == Some(true)).then_some(true),
    })
}

pub fn write_sand_remote_room_config(
    agent_dir: impl AsRef<Path>,
    config: &SandRemoteRoomConfig,
) -> io::Result<()> {
    let path = get_sand_remote_room_path(agent_dir);
    let room_id = config.room_id.trim();
    let host_auth_id = config.host_auth_id.trim();
    if room_id.is_empty() || host_auth_id.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "remote room ids are required"));
    }
    let normalized = SandRemoteRoomConfig {
        version: REMOTE_ROOM_CONFIG_VERSION,
        room_id: room_id.to_string(),
        host_auth_id: host_auth_id.to_string(),
        host_name: if config.host_name.trim().is_empty() { "The host".into() } else { config.host_name.trim().into() },
        host_avatar_url: config.host_avatar_url.as_deref().filter(|value| !value.is_empty()).map(ToOwned::to_owned),
        members: normalize_remote_room_members(&serde_json::to_value(&config.members).unwrap_or_default()),
        is_revoked: (config.is_revoked == Some(true)).then_some(true),
    };
    write_atomic_json(&path, &normalized)
}

fn write_atomic_json(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "remote room path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("json.tmp-{}", std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
        bytes.push(b'\n');
        let mut file = fs::File::create(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(&temporary); }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_room_config_is_bounded_and_revocation_is_durable() {
        let root = tempfile::tempdir().unwrap();
        let members = (0..30).map(|index| RemoteRoomMember {
            kind: if index % 2 == 0 { "agent".into() } else { "human".into() },
            auth_id: format!("auth-{index}"),
            agent_id: format!("agent-{index}"),
            display_name: format!("member-{index}"),
            avatar_url: None,
        }).collect::<Vec<_>>();
        write_sand_remote_room_config(root.path(), &SandRemoteRoomConfig {
            version: 99,
            room_id: "room-1".into(),
            host_auth_id: "host-1".into(),
            host_name: "Host".into(),
            host_avatar_url: None,
            members,
            is_revoked: Some(true),
        }).unwrap();
        let reopened = read_sand_remote_room_config(root.path()).unwrap();
        assert_eq!(reopened.version, REMOTE_ROOM_CONFIG_VERSION);
        assert_eq!(reopened.members.len(), REMOTE_ROOM_MAX_MEMBERS);
        assert_eq!(reopened.is_revoked, Some(true));
    }
}
