use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SAND_GROUP_FILENAME: &str = "group.json";
pub const GROUP_CONFIG_VERSION: u64 = 1;
pub const GROUP_MAX_MEMBERS: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteGroupMember {
    pub owner_auth_id: String,
    pub agent_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_data_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandGroupConfig {
    pub version: u64,
    pub member_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_members: Option<Vec<RemoteGroupMember>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_room_id: Option<String>,
}

pub fn get_sand_group_path(agent_dir: impl AsRef<Path>) -> PathBuf {
    agent_dir.as_ref().join(SAND_GROUP_FILENAME)
}

pub fn normalize_member_ids(raw: &Value) -> Vec<String> {
    let Some(values) = raw.as_array() else { return Vec::new() };
    let mut seen = BTreeSet::new();
    let mut ids = Vec::new();
    for value in values {
        let Some(raw_id) = value.as_str() else { continue };
        let id = raw_id.trim();
        if id.is_empty() || !seen.insert(id.to_string()) { continue; }
        ids.push(id.to_string());
        if ids.len() >= GROUP_MAX_MEMBERS { break; }
    }
    ids
}

pub fn normalize_remote_members(raw: &Value) -> Vec<RemoteGroupMember> {
    let Some(values) = raw.as_array() else { return Vec::new() };
    let mut seen = BTreeSet::new();
    let mut members = Vec::new();
    for value in values {
        let Some(candidate) = value.as_object() else { continue };
        let Some(owner_auth_id) = candidate.get("ownerAuthId").and_then(Value::as_str) else { continue };
        let Some(agent_id) = candidate.get("agentId").and_then(Value::as_str) else { continue };
        let owner_auth_id = owner_auth_id.trim();
        let agent_id = agent_id.trim();
        if owner_auth_id.is_empty() || agent_id.is_empty() { continue; }
        if !seen.insert(format!("{owner_auth_id}\0{agent_id}")) { continue; }
        members.push(RemoteGroupMember {
            owner_auth_id: owner_auth_id.to_string(),
            agent_id: agent_id.to_string(),
            name: candidate.get("name").and_then(Value::as_str).map(str::trim)
                .filter(|value| !value.is_empty()).unwrap_or("Agent").to_string(),
            avatar_data_url: candidate.get("avatarDataUrl").and_then(Value::as_str)
                .filter(|value| !value.is_empty()).map(ToOwned::to_owned),
        });
        if members.len() >= GROUP_MAX_MEMBERS { break; }
    }
    members
}

pub fn read_sand_group_config(agent_dir: impl AsRef<Path>) -> Option<SandGroupConfig> {
    let raw = fs::read_to_string(get_sand_group_path(agent_dir)).ok()?;
    let parsed = serde_json::from_str::<Value>(&raw).ok()?;
    let object = parsed.as_object()?;
    let member_ids = normalize_member_ids(object.get("memberIds").unwrap_or(&Value::Null));
    let remote_members = normalize_remote_members(object.get("remoteMembers").unwrap_or(&Value::Null));
    let shared_room_id = object.get("sharedRoomId").and_then(Value::as_str)
        .map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned);
    if member_ids.is_empty() && remote_members.is_empty() && shared_room_id.is_none() {
        return None;
    }
    Some(SandGroupConfig {
        version: object.get("version").and_then(Value::as_u64).unwrap_or(GROUP_CONFIG_VERSION),
        member_ids,
        remote_members: (!remote_members.is_empty()).then_some(remote_members),
        shared_room_id,
    })
}

pub fn write_sand_group_config(
    agent_dir: impl AsRef<Path>,
    config: &SandGroupConfig,
) -> io::Result<()> {
    let path = get_sand_group_path(agent_dir);
    let member_ids = normalize_member_ids(&serde_json::to_value(&config.member_ids).unwrap_or_default());
    let remote_members = normalize_remote_members(
        &serde_json::to_value(config.remote_members.as_deref().unwrap_or_default()).unwrap_or_default()
    );
    let normalized = SandGroupConfig {
        version: GROUP_CONFIG_VERSION,
        member_ids,
        remote_members: (!remote_members.is_empty()).then_some(remote_members),
        shared_room_id: config.shared_room_id.as_deref().map(str::trim)
            .filter(|value| !value.is_empty()).map(ToOwned::to_owned),
    };
    write_atomic_json(&path, &normalized)
}

fn write_atomic_json(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "group path has no parent"))?;
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
    fn group_config_normalizes_members_and_survives_reopen() {
        let root = tempfile::tempdir().unwrap();
        write_sand_group_config(root.path(), &SandGroupConfig {
            version: 99,
            member_ids: vec![" a ".into(), "a".into(), "b".into()],
            remote_members: Some(vec![RemoteGroupMember {
                owner_auth_id: "remote-user".into(),
                agent_id: "remote-agent".into(),
                name: " Remote ".into(),
                avatar_data_url: None,
            }]),
            shared_room_id: Some(" room-1 ".into()),
        }).unwrap();
        let reopened = read_sand_group_config(root.path()).unwrap();
        assert_eq!(reopened.version, GROUP_CONFIG_VERSION);
        assert_eq!(reopened.member_ids, vec!["a", "b"]);
        assert_eq!(reopened.shared_room_id.as_deref(), Some("room-1"));
        assert_eq!(reopened.remote_members.unwrap().len(), 1);
    }
}
