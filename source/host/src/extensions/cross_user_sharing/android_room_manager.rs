use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::android_agent_roster::AndroidAgentRoster;
use crate::extensions::transcript::TranscriptStore;
use crate::sha256::sha256_hex;

use super::xuser_sharing_service::XuserSharingManager;
use super::xuser_state_reconcile::XuserRoom;

const ROOM_BINDING_KIND: &str = "xuser-room-binding";

pub struct AndroidXuserRoomManager {
    roster: Arc<Mutex<AndroidAgentRoster>>,
    transcript: Arc<Mutex<TranscriptStore>>,
}

impl AndroidXuserRoomManager {
    pub fn new(
        roster: Arc<Mutex<AndroidAgentRoster>>,
        transcript: Arc<Mutex<TranscriptStore>>,
    ) -> Self {
        Self { roster, transcript }
    }

    fn binding_id(room_id: &str) -> String {
        format!("xuser-room-binding:{}", sha256_hex(room_id.as_bytes()))
    }

    fn room_binding(&self, room_id: &str) -> Result<Option<Value>, String> {
        let transcript = self
            .transcript
            .lock()
            .map_err(|_| "canonical transcript lock poisoned".to_string())?;
        Ok(transcript
            .get_transcript()
            .into_iter()
            .find(|entry| {
                entry.get("kind").and_then(Value::as_str) == Some(ROOM_BINDING_KIND)
                    && entry.get("roomId").and_then(Value::as_str) == Some(room_id)
            }))
    }

    fn upsert_binding(
        &self,
        room: &XuserRoom,
        agent_id: &str,
        self_auth_id: Option<&str>,
        revoked: bool,
    ) -> Result<(), String> {
        let id = Self::binding_id(&room.room_id);
        let members = room
            .members
            .iter()
            .map(|member| {
                json!({
                    "authId": member.auth_id,
                    "agentId": member.agent_id,
                    "name": member.name,
                })
            })
            .collect::<Vec<_>>();
        let entry = json!({
            "id": id,
            "kind": ROOM_BINDING_KIND,
            "agentId": agent_id,
            "roomId": room.room_id,
            "roomName": room.name,
            "hostAuthId": room.host_auth_id,
            "selfAuthId": self_auth_id,
            "members": members,
            "avatarDataUrl": room.avatar_data_url,
            "revoked": revoked,
            "timestampMs": now_ms(),
        });
        let mut transcript = self
            .transcript
            .lock()
            .map_err(|_| "canonical transcript lock poisoned".to_string())?;
        if transcript.contains_id(&id) {
            transcript
                .update_entry(&id, |_| entry.clone())
                .map_err(|error| format!("failed to update shared-room binding: {error}"))?;
        } else {
            transcript
                .append_entry(entry)
                .map_err(|error| format!("failed to persist shared-room binding: {error}"))?;
        }
        Ok(())
    }

    fn room_agent_id(&self, room_id: &str) -> Result<Option<String>, String> {
        Ok(self
            .room_binding(room_id)?
            .and_then(|entry| entry.get("agentId").and_then(Value::as_str).map(str::to_string)))
    }

    fn append_room_message(
        &self,
        room_id: &str,
        entry_id: String,
        role: &str,
        text: &str,
        metadata: Value,
        timestamp_ms: f64,
    ) -> Result<bool, String> {
        let Some(agent_id) = self.room_agent_id(room_id)? else {
            return Ok(false);
        };
        let entry = json!({
            "id": entry_id,
            "kind": "message",
            "agentId": agent_id,
            "role": role,
            "content": text,
            "timestampMs": timestamp_ms,
            "xuser": metadata,
        });
        self.transcript
            .lock()
            .map_err(|_| "canonical transcript lock poisoned".to_string())?
            .append_entry_if_absent(entry)
            .map_err(|error| format!("failed to persist shared-room transcript entry: {error}"))
    }
}

impl XuserSharingManager for AndroidXuserRoomManager {
    fn get_agent_display_profile(
        &self,
        agent_id: &str,
    ) -> Result<Option<(String, String)>, String> {
        Ok(self
            .roster
            .lock()
            .map_err(|_| "canonical Agent roster lock poisoned".to_string())?
            .get(agent_id)
            .map(|agent| (agent.name, agent.description)))
    }

    fn get_agent_avatar_data_url(&self, agent_id: &str) -> Result<Option<String>, String> {
        Ok(self
            .roster
            .lock()
            .map_err(|_| "canonical Agent roster lock poisoned".to_string())?
            .get(agent_id)
            .and_then(|agent| agent.avatar_data_url))
    }

    fn find_room_agent_id(&self, room_id: &str) -> Result<Option<String>, String> {
        self.room_agent_id(room_id)
    }

    fn is_room_abandoned(&self, room_id: &str, _self_auth_id: Option<&str>) -> bool {
        self.room_binding(room_id)
            .ok()
            .flatten()
            .and_then(|entry| entry.get("revoked").and_then(Value::as_bool))
            .unwrap_or(false)
    }

    fn install_room(&self, room: &XuserRoom, self_auth_id: Option<&str>) -> Result<(), String> {
        let existing_id = self.room_agent_id(&room.room_id)?;
        let agent_id = {
            let mut roster = self
                .roster
                .lock()
                .map_err(|_| "canonical Agent roster lock poisoned".to_string())?;
            match existing_id.as_deref().and_then(|id| roster.get(id).map(|_| id.to_string())) {
                Some(id) => {
                    roster
                        .update_profile(&id, &room.name, "Shared room")
                        .map_err(|error| format!("failed to update shared-room group: {error}"))?;
                    roster
                        .set_hidden(&id, false)
                        .map_err(|error| format!("failed to unhide shared-room group: {error}"))?;
                    id
                }
                None => roster
                    .create_group(&room.name, "Shared room", &[])
                    .map_err(|error| format!("failed to create shared-room group: {error}"))?
                    .id,
            }
        };
        self.upsert_binding(room, &agent_id, self_auth_id, false)
    }

    fn mark_mirror_room_revoked(&self, room_id: &str) -> Result<(), String> {
        let Some(binding) = self.room_binding(room_id)? else {
            return Ok(());
        };
        if let Some(agent_id) = binding.get("agentId").and_then(Value::as_str) {
            let _ = self
                .roster
                .lock()
                .map_err(|_| "canonical Agent roster lock poisoned".to_string())?
                .set_hidden(agent_id, true);
        }
        let id = Self::binding_id(room_id);
        self.transcript
            .lock()
            .map_err(|_| "canonical transcript lock poisoned".to_string())?
            .update_entry(&id, |current| {
                let mut next = current.clone();
                next["revoked"] = Value::Bool(true);
                next["timestampMs"] = Value::from(now_ms());
                next
            })
            .map_err(|error| format!("failed to revoke shared-room binding: {error}"))?;
        Ok(())
    }

    fn post_shared_room_guest_message(&self, event: &Value) -> Result<(), String> {
        let Some(room_id) = event.get("roomId").and_then(Value::as_str) else {
            return Ok(());
        };
        let author = event
            .get("authorName")
            .and_then(Value::as_str)
            .unwrap_or("Someone");
        let text = event.get("text").and_then(Value::as_str).unwrap_or_default();
        let timestamp_ms = event
            .get("timestampMs")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .unwrap_or_else(now_ms);
        let fingerprint = serde_json::to_vec(event).unwrap_or_default();
        let id = format!("xuser-room-post:{}", sha256_hex(&fingerprint));
        let _ = self.append_room_message(
            room_id,
            id,
            "user",
            text,
            json!({
                "kind":"room-post",
                "authorAuthId":event.get("authorAuthId"),
                "authorName":author,
                "images":event.get("images").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
            }),
            timestamp_ms,
        )?;
        Ok(())
    }

    fn append_mirror_room_entry(
        &self,
        event: &Value,
        self_auth_id: &str,
    ) -> Result<bool, String> {
        let Some(room_id) = event.get("roomId").and_then(Value::as_str) else {
            return Ok(true);
        };
        if self.is_room_abandoned(room_id, Some(self_auth_id)) {
            return Ok(true);
        }
        let nested = event.get("entry").unwrap_or(event);
        let text = nested
            .get("content")
            .or_else(|| nested.get("text"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let role = nested
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("assistant");
        let timestamp_ms = event
            .get("timestampMs")
            .or_else(|| nested.get("timestampMs"))
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .unwrap_or_else(now_ms);
        let id = nested
            .get("id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                let fingerprint = serde_json::to_vec(event).unwrap_or_default();
                format!("xuser-room-entry:{}", sha256_hex(&fingerprint))
            });
        self.append_room_message(
            room_id,
            id,
            role,
            text,
            json!({
                "kind":"room-entry",
                "selfAuthId":self_auth_id,
                "source":nested,
            }),
            timestamp_ms,
        )
    }
}

fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::xuser_state_reconcile::{RoomMember, XuserRoom};
    use std::path::PathBuf;

    fn root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-android-xuser-manager-{}-{}",
            std::process::id(),
            now_ms() as u64
        ))
    }

    #[test]
    fn room_binding_survives_reopen_and_inbound_message_is_idempotent() {
        let root = root();
        let roster_path = root.join("agents.json");
        let transcript_path = root.join("transcript.json");
        let roster = Arc::new(Mutex::new(AndroidAgentRoster::open(&roster_path).unwrap()));
        let transcript = Arc::new(Mutex::new(TranscriptStore::open(&transcript_path).unwrap()));
        let manager = AndroidXuserRoomManager::new(Arc::clone(&roster), Arc::clone(&transcript));
        let room = XuserRoom {
            room_id: "room-a".into(),
            name: "Shared A".into(),
            host_auth_id: "remote-host".into(),
            members: vec![RoomMember {
                auth_id: "self".into(),
                agent_id: None,
                name: Some("Me".into()),
            }],
            avatar_data_url: None,
        };
        manager.install_room(&room, Some("self")).unwrap();
        let group_id = manager.find_room_agent_id("room-a").unwrap().unwrap();
        assert!(roster.lock().unwrap().get(&group_id).unwrap().is_group);

        let event = json!({
            "roomId":"room-a",
            "authorAuthId":"remote-human",
            "authorName":"Remote",
            "text":"hello",
            "timestampMs":42.0,
        });
        manager.post_shared_room_guest_message(&event).unwrap();
        manager.post_shared_room_guest_message(&event).unwrap();

        drop(manager);
        drop(transcript);
        drop(roster);
        let roster = Arc::new(Mutex::new(AndroidAgentRoster::open(&roster_path).unwrap()));
        let transcript = Arc::new(Mutex::new(TranscriptStore::open(&transcript_path).unwrap()));
        let reopened = AndroidXuserRoomManager::new(roster, Arc::clone(&transcript));
        assert_eq!(reopened.find_room_agent_id("room-a").unwrap().as_deref(), Some(group_id.as_str()));
        let messages = transcript
            .lock()
            .unwrap()
            .get_transcript()
            .into_iter()
            .filter(|entry| entry.get("agentId").and_then(Value::as_str) == Some(group_id.as_str()))
            .filter(|entry| entry.get("kind").and_then(Value::as_str) == Some("message"))
            .collect::<Vec<_>>();
        assert_eq!(messages.len(), 1);

        reopened.mark_mirror_room_revoked("room-a").unwrap();
        assert!(reopened.is_room_abandoned("room-a", Some("self")));
        let _ = std::fs::remove_dir_all(root);
    }
}
