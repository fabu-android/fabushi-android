use crate::android_agent_roster::AndroidAgentRoster;
use crate::extensions::cross_user_sharing::xuser_sharing_service::XuserSharingManager;
use crate::extensions::cross_user_sharing::xuser_state_reconcile::XuserRoom;
use crate::extensions::cross_user_sharing::xuser_wire_normalization::{
    normalize_mirror_entry, InlineImage, MirrorEntry,
};
use crate::extensions::transcript::TranscriptStore;
use crate::groups::group_store::{
    write_sand_group_config, RemoteGroupMember, SandGroupConfig, GROUP_CONFIG_VERSION,
};
use crate::groups::remote_room_store::{
    read_sand_remote_room_config, write_sand_remote_room_config, RemoteRoomMember,
    SandRemoteRoomConfig, REMOTE_ROOM_CONFIG_VERSION,
};
use crate::sha256::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedRoomMirrorRecord {
    room: XuserRoom,
    mirror_agent_id: String,
    #[serde(default)]
    revoked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedRoomRegistryFile {
    #[serde(default = "registry_schema_version")]
    schema_version: u8,
    #[serde(default)]
    rooms: BTreeMap<String, SharedRoomMirrorRecord>,
}

impl Default for SharedRoomRegistryFile {
    fn default() -> Self {
        Self {
            schema_version: registry_schema_version(),
            rooms: BTreeMap::new(),
        }
    }
}

fn registry_schema_version() -> u8 {
    1
}

struct DurableSharedRoomRegistry {
    path: PathBuf,
    state: SharedRoomRegistryFile,
}

impl DurableSharedRoomRegistry {
    fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        if !path.exists() {
            return Ok(Self {
                path,
                state: SharedRoomRegistryFile::default(),
            });
        }
        let raw = fs::read_to_string(&path)?;
        let mut state: SharedRoomRegistryFile = serde_json::from_str(&raw)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if state.schema_version != registry_schema_version() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported shared-room registry schema",
            ));
        }
        state.rooms.retain(|room_id, record| {
            !room_id.trim().is_empty()
                && room_id == &record.room.room_id
                && !record.mirror_agent_id.trim().is_empty()
        });
        Ok(Self { path, state })
    }

    fn room(&self, room_id: &str) -> Option<SharedRoomMirrorRecord> {
        self.state.rooms.get(room_id).cloned()
    }

    fn install(&mut self, room: XuserRoom, mirror_agent_id: String) -> io::Result<()> {
        let mut next = self.state.clone();
        next.rooms.insert(
            room.room_id.clone(),
            SharedRoomMirrorRecord {
                room,
                mirror_agent_id,
                revoked: false,
            },
        );
        self.commit(next)
    }

    fn revoke(&mut self, room_id: &str) -> io::Result<Option<String>> {
        let mut next = self.state.clone();
        let mirror_agent_id = next.rooms.get_mut(room_id).map(|record| {
            record.revoked = true;
            record.mirror_agent_id.clone()
        });
        if mirror_agent_id.is_some() {
            self.commit(next)?;
        }
        Ok(mirror_agent_id)
    }

    fn commit(&mut self, next: SharedRoomRegistryFile) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.path.with_extension(format!("tmp-{}", std::process::id()));
        let serialized = serde_json::to_vec_pretty(&next)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let result = (|| -> io::Result<()> {
            let mut file = fs::File::create(&temporary)?;
            file.write_all(&serialized)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        self.state = next;
        Ok(())
    }
}

/// Android adapter from Sand room state into the existing canonical Agent and transcript owners.
///
/// The registry owns only room -> mirror Agent association. SandXuserSharingService remains the
/// canonical remote room/membership owner, AndroidAgentRoster remains the Agent profile owner, and
/// TranscriptStore remains the transcript owner.
pub struct AndroidXuserSharingManager {
    agents: Arc<Mutex<AndroidAgentRoster>>,
    transcript: Arc<Mutex<TranscriptStore>>,
    registry: Mutex<DurableSharedRoomRegistry>,
    room_metadata_root: PathBuf,
}

impl AndroidXuserSharingManager {
    pub fn open(
        path: impl Into<PathBuf>,
        agents: Arc<Mutex<AndroidAgentRoster>>,
        transcript: Arc<Mutex<TranscriptStore>>,
    ) -> io::Result<Self> {
        let path = path.into();
        let room_metadata_root = path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("room-agents");
        Ok(Self {
            agents,
            transcript,
            registry: Mutex::new(DurableSharedRoomRegistry::open(path)?),
            room_metadata_root,
        })
    }

    fn room_agent_dir(&self, agent_id: &str) -> PathBuf {
        self.room_metadata_root.join(agent_id)
    }

    fn persist_room_binding(
        &self,
        room: &XuserRoom,
        self_auth_id: Option<&str>,
        agent_id: &str,
    ) -> Result<(), String> {
        let dir = self.room_agent_dir(agent_id);
        let is_host = self_auth_id.is_some_and(|self_id| self_id == room.host_auth_id);
        if is_host {
            let local_member_ids = room
                .members
                .iter()
                .filter(|member| Some(member.auth_id.as_str()) == self_auth_id)
                .filter_map(|member| member.agent_id.as_deref())
                .map(str::to_string)
                .collect::<Vec<_>>();
            let remote_members = room
                .members
                .iter()
                .filter(|member| Some(member.auth_id.as_str()) != self_auth_id)
                .filter_map(|member| {
                    Some(RemoteGroupMember {
                        owner_auth_id: member.auth_id.clone(),
                        agent_id: member.agent_id.clone()?,
                        name: member.name.clone().unwrap_or_else(|| "Agent".into()),
                        avatar_data_url: None,
                    })
                })
                .collect::<Vec<_>>();
            write_sand_group_config(
                &dir,
                &SandGroupConfig {
                    version: GROUP_CONFIG_VERSION,
                    member_ids: local_member_ids,
                    remote_members: (!remote_members.is_empty()).then_some(remote_members),
                    shared_room_id: Some(room.room_id.clone()),
                },
            )
            .map_err(|error| error.to_string())
        } else {
            let previous = read_sand_remote_room_config(&dir);
            let members = room
                .members
                .iter()
                .filter(|member| Some(member.auth_id.as_str()) != self_auth_id)
                .map(|member| RemoteRoomMember {
                    kind: if member.agent_id.is_some() { "agent".into() } else { "human".into() },
                    auth_id: member.auth_id.clone(),
                    agent_id: member.agent_id.clone().unwrap_or_default(),
                    display_name: member.name.clone().unwrap_or_else(|| "Someone".into()),
                    avatar_url: None,
                })
                .collect::<Vec<_>>();
            write_sand_remote_room_config(
                &dir,
                &SandRemoteRoomConfig {
                    version: REMOTE_ROOM_CONFIG_VERSION,
                    room_id: room.room_id.clone(),
                    host_auth_id: room.host_auth_id.clone(),
                    host_name: room
                        .members
                        .iter()
                        .find(|member| member.auth_id == room.host_auth_id)
                        .and_then(|member| member.name.clone())
                        .unwrap_or_else(|| "The host".into()),
                    host_avatar_url: previous.and_then(|value| value.host_avatar_url),
                    members,
                    is_revoked: None,
                },
            )
            .map_err(|error| error.to_string())
        }
    }

    fn mark_room_metadata_revoked(&self, agent_id: &str) -> Result<(), String> {
        let dir = self.room_agent_dir(agent_id);
        let Some(mut config) = read_sand_remote_room_config(&dir) else {
            return Ok(());
        };
        if config.is_revoked == Some(true) {
            return Ok(());
        }
        config.is_revoked = Some(true);
        write_sand_remote_room_config(&dir, &config).map_err(|error| error.to_string())
    }

    fn room_record(&self, room_id: &str) -> Option<SharedRoomMirrorRecord> {
        self.registry.lock().ok()?.room(room_id)
    }

    fn mirror_agent_id(&self, room_id: &str) -> Result<Option<String>, String> {
        Ok(self
            .room_record(room_id)
            .filter(|record| !record.revoked)
            .map(|record| record.mirror_agent_id))
    }

    fn append_entry_if_absent(&self, entry: Value) -> Result<bool, String> {
        self.transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?
            .append_entry_if_absent(entry)
            .map_err(|error| error.to_string())
    }

    /// Hide and revoke every mirror owned by the previous account before the Sand owner is fenced.
    ///
    /// A later account that happens to receive the same room id must create a fresh mirror Agent,
    /// so old-account transcript rows can never become visible again through id reuse.
    pub fn fence_account_transition(&self) -> Result<(), String> {
        let mirror_ids = {
            let mut registry = self
                .registry
                .lock()
                .map_err(|_| "shared-room registry lock poisoned".to_string())?;
            let mut next = registry.state.clone();
            let mut mirror_ids = Vec::new();
            for record in next.rooms.values_mut() {
                record.revoked = true;
                mirror_ids.push(record.mirror_agent_id.clone());
            }
            if !mirror_ids.is_empty() {
                registry.commit(next).map_err(|error| error.to_string())?;
            }
            mirror_ids
        };
        let mut agents = self
            .agents
            .lock()
            .map_err(|_| "agent roster lock poisoned".to_string())?;
        for mirror_id in mirror_ids {
            self.mark_room_metadata_revoked(&mirror_id)?;
            if agents.get(&mirror_id).is_some() {
                agents
                    .set_hidden(&mirror_id, true)
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }
}

impl XuserSharingManager for AndroidXuserSharingManager {
    fn get_agent_display_profile(
        &self,
        agent_id: &str,
    ) -> Result<Option<(String, String)>, String> {
        Ok(self
            .agents
            .lock()
            .map_err(|_| "agent roster lock poisoned".to_string())?
            .get(agent_id)
            .map(|agent| (agent.name, agent.description)))
    }

    fn get_agent_avatar_data_url(&self, agent_id: &str) -> Result<Option<String>, String> {
        Ok(self
            .agents
            .lock()
            .map_err(|_| "agent roster lock poisoned".to_string())?
            .get(agent_id)
            .and_then(|agent| agent.avatar_data_url))
    }

    fn find_room_agent_id(&self, room_id: &str) -> Result<Option<String>, String> {
        self.mirror_agent_id(room_id)
    }

    fn is_room_abandoned(&self, room_id: &str, self_auth_id: Option<&str>) -> bool {
        let Some(record) = self.room_record(room_id) else {
            return false;
        };
        if record.revoked {
            return true;
        }
        let Some(self_auth_id) = self_auth_id else {
            return false;
        };
        record.room.host_auth_id != self_auth_id
            && !record
                .room
                .members
                .iter()
                .any(|member| member.auth_id == self_auth_id)
    }

    fn install_room(&self, room: &XuserRoom, self_auth_id: Option<&str>) -> Result<(), String> {
        let existing = self.room_record(&room.room_id);
        let mirror_agent_id = if let Some(existing) = existing.filter(|record| !record.revoked) {
            let mirror_id = existing.mirror_agent_id;
            let mut agents = self
                .agents
                .lock()
                .map_err(|_| "agent roster lock poisoned".to_string())?;
            if agents.get(&mirror_id).is_some() {
                agents
                    .update_profile(&mirror_id, &room.name, "Shared room")
                    .map_err(|error| error.to_string())?;
                agents
                    .set_hidden(&mirror_id, false)
                    .map_err(|error| error.to_string())?;
                mirror_id
            } else {
                agents
                    .create(&room.name, "Shared room")
                    .map_err(|error| error.to_string())?
                    .id
            }
        } else {
            self.agents
                .lock()
                .map_err(|_| "agent roster lock poisoned".to_string())?
                .create(&room.name, "Shared room")
                .map_err(|error| error.to_string())?
                .id
        };

        self.persist_room_binding(room, self_auth_id, &mirror_agent_id)?;
        self.registry
            .lock()
            .map_err(|_| "shared-room registry lock poisoned".to_string())?
            .install(room.clone(), mirror_agent_id)
            .map_err(|error| error.to_string())
    }

    fn mark_mirror_room_revoked(&self, room_id: &str) -> Result<(), String> {
        let mirror_agent_id = self
            .registry
            .lock()
            .map_err(|_| "shared-room registry lock poisoned".to_string())?
            .revoke(room_id)
            .map_err(|error| error.to_string())?;
        if let Some(mirror_agent_id) = mirror_agent_id {
            self.mark_room_metadata_revoked(&mirror_agent_id)?;
            let mut agents = self
                .agents
                .lock()
                .map_err(|_| "agent roster lock poisoned".to_string())?;
            if agents.get(&mirror_agent_id).is_some() {
                agents
                    .set_hidden(&mirror_agent_id, true)
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    fn post_shared_room_guest_message(&self, event: &Value) -> Result<(), String> {
        let room_id = event
            .get("roomId")
            .and_then(Value::as_str)
            .ok_or_else(|| "room-post missing roomId".to_string())?;
        let Some(agent_id) = self.mirror_agent_id(room_id)? else {
            return Ok(());
        };
        let client_nonce = event
            .get("clientNonce")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if client_nonce.is_some_and(|nonce| {
            self.transcript.lock().is_ok_and(|transcript| {
                transcript.get_transcript().iter().any(|entry| {
                    entry.get("agentId").and_then(Value::as_str) == Some(agent_id.as_str())
                        && entry.get("clientNonce").and_then(Value::as_str) == Some(nonce)
                })
            })
        }) {
            return Ok(());
        }
        let identity = client_nonce
            .map(|nonce| format!("{room_id}\n{nonce}").into_bytes())
            .unwrap_or(serde_json::to_vec(event).map_err(|error| error.to_string())?);
        let mut entry = json!({
            "id": format!("xuser-room-post-{}", sha256_hex(&identity)),
            "agentId": agent_id,
            "kind": "message",
            "role": "user",
            "content": event.get("text").and_then(Value::as_str).unwrap_or_default(),
            "fromUser": {
                "authId": event.get("authorAuthId").and_then(Value::as_str).unwrap_or_default(),
                "name": event.get("authorName").and_then(Value::as_str).unwrap_or("Someone"),
            },
            "images": event.get("images").cloned().unwrap_or_else(|| Value::Array(vec![])),
            "timestampMs": event.get("timestampMs").and_then(Value::as_f64).unwrap_or_else(|| now_ms() as f64),
            "sharedRoomId": room_id,
            "xuserRemote": true,
        });
        if let Some(client_nonce) = client_nonce {
            entry["clientNonce"] = Value::String(client_nonce.to_string());
        }
        self.append_entry_if_absent(entry)?;
        Ok(())
    }

    fn append_mirror_room_entry(
        &self,
        event: &Value,
        self_auth_id: &str,
    ) -> Result<bool, String> {
        let room_id = event
            .get("roomId")
            .and_then(Value::as_str)
            .ok_or_else(|| "room-entry missing roomId".to_string())?;
        let Some(agent_id) = self.mirror_agent_id(room_id)? else {
            return Ok(true);
        };
        let Some(wire) = event.get("entry") else {
            return Ok(true);
        };
        let Some(normalized) = normalize_mirror_entry(wire, now_ms()) else {
            return Ok(true);
        };
        let replay_nonce = match &normalized {
            MirrorEntry::HumanMessage { client_nonce, .. } => client_nonce.as_deref(),
            MirrorEntry::AgentMessage { .. } => None,
        };
        if replay_nonce.is_some_and(|nonce| {
            self.transcript.lock().is_ok_and(|transcript| {
                transcript.get_transcript().iter().any(|entry| {
                    entry.get("agentId").and_then(Value::as_str) == Some(agent_id.as_str())
                        && entry.get("clientNonce").and_then(Value::as_str) == Some(nonce)
                })
            })
        }) {
            return Ok(true);
        }
        let local = match normalized {
            MirrorEntry::HumanMessage {
                entry_id,
                author_auth_id,
                author_name,
                author_avatar_url,
                text,
                images,
                client_nonce,
                timestamp_ms,
            } => {
                let mut value = json!({
                    "id": format!("xuser:{room_id}:{entry_id}"),
                    "agentId": agent_id,
                    "kind": "message",
                    "role": "user",
                    "content": text,
                    "images": images_json(images),
                    "clientNonce": client_nonce,
                    "timestampMs": timestamp_ms,
                    "sharedRoomId": room_id,
                    "xuserRemote": true,
                });
                if author_auth_id != self_auth_id {
                    value["fromUser"] = json!({
                        "authId": author_auth_id,
                        "name": author_name,
                        "avatarUrl": author_avatar_url,
                    });
                }
                value
            },
            MirrorEntry::AgentMessage {
                entry_id,
                agent_owner_auth_id,
                agent_id: remote_agent_id,
                author_name,
                text,
                images,
                timestamp_ms,
            } => json!({
                "id": format!("xuser:{room_id}:{entry_id}"),
                "agentId": agent_id,
                "kind": "send-message",
                "author": {
                    "id": format!("remote-agent:{agent_owner_auth_id}:{remote_agent_id}"),
                    "name": author_name,
                },
                "message": {
                    "type": "text",
                    "content": text,
                },
                "images": images_json(images),
                "timestampMs": timestamp_ms,
                "sharedRoomId": room_id,
                "xuserRemote": true,
            }),
        };
        self.append_entry_if_absent(local)?;
        Ok(true)
    }
}

fn images_json(images: Vec<InlineImage>) -> Value {
    Value::Array(
        images
            .into_iter()
            .map(|image| {
                json!({
                    "base64": image.base64,
                    "mediaType": image.media_type,
                    "alt": image.alt,
                })
            })
            .collect(),
    )
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::cross_user_sharing::xuser_state_reconcile::RoomMember;
    use std::path::Path;
    use tempfile::tempdir;

    fn room() -> XuserRoom {
        XuserRoom {
            room_id: "room-a".into(),
            name: "Shared A".into(),
            host_auth_id: "host-a".into(),
            members: vec![RoomMember {
                auth_id: "self-a".into(),
                agent_id: None,
                name: Some("Me".into()),
            }],
            avatar_data_url: None,
        }
    }

    fn open_manager(
        root: &Path,
    ) -> (
        Arc<Mutex<AndroidAgentRoster>>,
        Arc<Mutex<TranscriptStore>>,
        AndroidXuserSharingManager,
    ) {
        let agents = Arc::new(Mutex::new(
            AndroidAgentRoster::open(root.join("agents.json")).unwrap(),
        ));
        let transcript = Arc::new(Mutex::new(
            TranscriptStore::open(root.join("transcript.json")).unwrap(),
        ));
        let manager = AndroidXuserSharingManager::open(
            root.join("shared-room-registry.json"),
            Arc::clone(&agents),
            Arc::clone(&transcript),
        )
        .unwrap();
        (agents, transcript, manager)
    }

    #[test]
    fn room_mirror_mapping_survives_reopen_and_revoke_hides_projection() {
        let root = tempdir().unwrap();
        let (_, _, manager) = open_manager(root.path());
        manager.install_room(&room(), Some("self-a")).unwrap();
        let mirror_id = manager.find_room_agent_id("room-a").unwrap().unwrap();
        drop(manager);

        let (reopened_agents, _, reopened) = open_manager(root.path());
        assert_eq!(
            reopened.find_room_agent_id("room-a").unwrap().as_deref(),
            Some(mirror_id.as_str())
        );
        reopened.mark_mirror_room_revoked("room-a").unwrap();
        assert!(reopened.find_room_agent_id("room-a").unwrap().is_none());
        assert!(
            reopened_agents
                .lock()
                .unwrap()
                .get(&mirror_id)
                .unwrap()
                .is_hidden_from_sidebar
        );
    }

    #[test]
    fn new_account_same_room_id_gets_fresh_mirror_after_fence() {
        let root = tempdir().unwrap();
        let (agents, _, manager) = open_manager(root.path());
        manager.install_room(&room(), Some("account-a")).unwrap();
        let old_mirror = manager.find_room_agent_id("room-a").unwrap().unwrap();

        manager.fence_account_transition().unwrap();
        assert!(manager.find_room_agent_id("room-a").unwrap().is_none());
        assert!(agents.lock().unwrap().get(&old_mirror).unwrap().is_hidden_from_sidebar);

        manager.install_room(&room(), Some("account-b")).unwrap();
        let new_mirror = manager.find_room_agent_id("room-a").unwrap().unwrap();
        assert_ne!(old_mirror, new_mirror);
        assert!(!agents.lock().unwrap().get(&new_mirror).unwrap().is_hidden_from_sidebar);
    }

    #[test]
    fn hosted_and_mirror_room_metadata_survive_reopen_and_revocation() {
        let root = tempdir().unwrap();
        let (_, _, manager) = open_manager(root.path());

        let mut hosted = room();
        hosted.host_auth_id = "self-a".into();
        hosted.members.push(RoomMember {
            auth_id: "self-a".into(),
            agent_id: Some("local-agent".into()),
            name: Some("Local".into()),
        });
        hosted.members.push(RoomMember {
            auth_id: "remote-a".into(),
            agent_id: Some("remote-agent".into()),
            name: Some("Remote".into()),
        });
        manager.install_room(&hosted, Some("self-a")).unwrap();
        let hosted_agent = manager.find_room_agent_id("room-a").unwrap().unwrap();
        let hosted_config = crate::groups::group_store::read_sand_group_config(
            manager.room_agent_dir(&hosted_agent),
        )
        .unwrap();
        assert_eq!(hosted_config.shared_room_id.as_deref(), Some("room-a"));
        assert!(hosted_config.member_ids.contains(&"local-agent".to_string()));
        assert_eq!(hosted_config.remote_members.unwrap()[0].agent_id, "remote-agent");

        let mirror = XuserRoom {
            room_id: "room-b".into(),
            name: "Mirror B".into(),
            host_auth_id: "remote-host".into(),
            members: vec![
                RoomMember {
                    auth_id: "self-a".into(),
                    agent_id: None,
                    name: Some("Me".into()),
                },
                RoomMember {
                    auth_id: "remote-host".into(),
                    agent_id: None,
                    name: Some("Host".into()),
                },
            ],
            avatar_data_url: None,
        };
        manager.install_room(&mirror, Some("self-a")).unwrap();
        let mirror_agent = manager.find_room_agent_id("room-b").unwrap().unwrap();
        let mirror_dir = manager.room_agent_dir(&mirror_agent);
        let mirror_config =
            crate::groups::remote_room_store::read_sand_remote_room_config(&mirror_dir).unwrap();
        assert_eq!(mirror_config.room_id, "room-b");
        assert_eq!(mirror_config.host_auth_id, "remote-host");
        assert!(mirror_config.members.iter().all(|member| member.auth_id != "self-a"));

        manager.mark_mirror_room_revoked("room-b").unwrap();
        let revoked =
            crate::groups::remote_room_store::read_sand_remote_room_config(&mirror_dir).unwrap();
        assert_eq!(revoked.is_revoked, Some(true));
    }

    #[test]
    fn guest_room_posts_dedupe_by_client_nonce_across_relay_rewrites() {
        let root = tempdir().unwrap();
        let (_, transcript, manager) = open_manager(root.path());
        manager.install_room(&room(), Some("self-a")).unwrap();
        let first = json!({
            "kind":"room-post",
            "roomId":"room-a",
            "authorAuthId":"guest-a",
            "authorName":"Guest",
            "text":"hello",
            "clientNonce":"nonce-a",
            "timestampMs":42
        });
        let mut replay = first.clone();
        replay["timestampMs"] = json!(84);
        replay["authorName"] = json!("Guest renamed");
        manager.post_shared_room_guest_message(&first).unwrap();
        manager.post_shared_room_guest_message(&replay).unwrap();
        let rows = transcript.lock().unwrap().get_transcript();
        let matching = rows.iter().filter(|row| {
            row.get("clientNonce").and_then(Value::as_str) == Some("nonce-a")
        }).collect::<Vec<_>>();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].get("timestampMs"), Some(&json!(42.0)));
    }

    #[test]
    fn mirror_human_replay_uses_nonce_and_self_messages_have_no_remote_author() {
        let root = tempdir().unwrap();
        let (_, transcript, manager) = open_manager(root.path());
        manager.install_room(&room(), Some("self-a")).unwrap();
        let first = json!({
            "kind":"room-entry",
            "roomId":"room-a",
            "entry":{
                "kind":"human-message",
                "entryId":"entry-self-a",
                "authorAuthId":"self-a",
                "authorName":"Me",
                "text":"hello",
                "clientNonce":"self-nonce-a",
                "timestampMs":42
            }
        });
        let replay = json!({
            "kind":"room-entry",
            "roomId":"room-a",
            "entry":{
                "kind":"human-message",
                "entryId":"entry-self-b",
                "authorAuthId":"self-a",
                "authorName":"Me",
                "text":"hello",
                "clientNonce":"self-nonce-a",
                "timestampMs":84
            }
        });
        assert!(manager.append_mirror_room_entry(&first, "self-a").unwrap());
        assert!(manager.append_mirror_room_entry(&replay, "self-a").unwrap());
        let rows = transcript.lock().unwrap().get_transcript();
        let matching = rows.iter().filter(|row| {
            row.get("clientNonce").and_then(Value::as_str) == Some("self-nonce-a")
        }).collect::<Vec<_>>();
        assert_eq!(matching.len(), 1);
        assert!(matching[0].get("fromUser").is_none());
        assert_eq!(matching[0].get("id"), Some(&json!("xuser:room-a:entry-self-a")));
    }

    #[test]
    fn remote_entries_are_idempotent_in_canonical_transcript() {
        let root = tempdir().unwrap();
        let (_, transcript, manager) = open_manager(root.path());
        manager.install_room(&room(), Some("self-a")).unwrap();
        let event = json!({
            "kind":"room-entry",
            "roomId":"room-a",
            "entry":{
                "kind":"human-message",
                "entryId":"entry-a",
                "authorAuthId":"guest-a",
                "authorName":"Guest",
                "text":"hello",
                "timestampMs":42
            }
        });
        assert!(manager.append_mirror_room_entry(&event, "self-a").unwrap());
        assert!(manager.append_mirror_room_entry(&event, "self-a").unwrap());
        let rows = transcript.lock().unwrap().get_transcript();
        assert_eq!(
            rows.iter()
                .filter(|row| row.get("id") == Some(&json!("xuser:room-a:entry-a")))
                .count(),
            1
        );
    }
}
