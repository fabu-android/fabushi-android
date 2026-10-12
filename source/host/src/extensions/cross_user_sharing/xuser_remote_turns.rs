use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use crate::groups::group_chat::{GroupDescription, GroupMember, GroupMessage};
use crate::groups::xuser::{
    REMOTE_MEMBER_TURN_TIMEOUT_MS, XuserSpeakerKind, XuserTurnMessage,
    build_remote_member_turn_prompts, clamp_guest_name, to_xuser_turn_messages,
};

use super::xuser_entry_publisher::parse_remote_agent_id;
use super::xuser_state_reconcile::XuserRoom;
use super::xuser_turn_dedupe_store::XuserTurnDedupe;
use super::xuser_wire_normalization::normalize_turn_messages;

pub const REMOTE_MEMBER_UNREACHABLE_BACKOFF_MS: u64 = 10 * 60_000;
pub const REMOTE_TURN_BUDGET_WINDOW_MS: u64 = 10 * 60_000;
pub const REMOTE_TURN_BUDGET_MAX: usize = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDisplayProfile {
    pub name: String,
    pub description: String,
}

pub trait RemoteTurnsHost: Send + Sync {
    fn relay_send(&self, payload: &Value) -> Result<Value, String>;
    fn relay_request_drain(&self);
    fn relay_remove_deleted_agent(&self, agent_id: &str) -> Result<Value, String>;
    fn now_ms(&self) -> u64;
    fn is_enabled(&self) -> bool;
    fn self_auth_id(&self) -> Option<String>;
    fn rooms(&self) -> Vec<XuserRoom>;
    fn get_agent_display_profile(&self, agent_id: &str)
        -> Result<Option<AgentDisplayProfile>, String>;
    fn append_shared_room_activity_notice(
        &self,
        room_id: &str,
        text: &str,
    ) -> Result<(), String>;
    fn run_remote_requested_member_turn(
        &self,
        agent_id: &str,
        system_prompt: &str,
        prompt: &str,
    ) -> Result<Vec<String>, String>;
    fn reconcile_share_state(&self) -> Result<(), String>;
}

pub struct RemoteTurnBudget {
    starts: Mutex<VecDeque<u64>>,
    max: usize,
    window_ms: u64,
}

impl RemoteTurnBudget {
    pub fn new(max: usize, window_ms: u64) -> Self {
        Self {
            starts: Mutex::new(VecDeque::new()),
            max,
            window_ms,
        }
    }

    pub fn accept(&self, now_ms: u64) -> bool {
        let mut starts = self.starts.lock().unwrap_or_else(|p| p.into_inner());
        while starts
            .front()
            .is_some_and(|started| now_ms.saturating_sub(*started) >= self.window_ms)
        {
            starts.pop_front();
        }
        if starts.len() >= self.max {
            return false;
        }
        starts.push_back(now_ms);
        true
    }

    pub fn clear(&self) {
        self.starts
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
    }
}

struct PendingRemoteTurn {
    settle: mpsc::SyncSender<Vec<String>>,
}

pub struct SandXuserRemoteTurns {
    host: Arc<dyn RemoteTurnsHost>,
    turn_dedupe: Arc<dyn XuserTurnDedupe>,
    pending_remote_turns: Mutex<HashMap<String, PendingRemoteTurn>>,
    unreachable_remote_members: Mutex<HashMap<String, u64>>,
    remote_turn_budgets: Mutex<HashMap<String, Arc<RemoteTurnBudget>>>,
}

impl SandXuserRemoteTurns {
    pub fn new(
        host: Arc<dyn RemoteTurnsHost>,
        turn_dedupe: Arc<dyn XuserTurnDedupe>,
    ) -> Self {
        Self {
            host,
            turn_dedupe,
            pending_remote_turns: Mutex::new(HashMap::new()),
            unreachable_remote_members: Mutex::new(HashMap::new()),
            remote_turn_budgets: Mutex::new(HashMap::new()),
        }
    }

    pub fn stop(&self) {
        let pending = {
            let mut pending = self
                .pending_remote_turns
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            pending.drain().map(|(_, value)| value).collect::<Vec<_>>()
        };
        for pending in pending {
            let _ = pending.settle.send(Vec::new());
        }
        self.remote_turn_budgets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
    }

    pub fn handle_turn_request(self: &Arc<Self>, event: &Value) -> bool {
        let Some(room_id) = non_empty_str(event, "roomId") else {
            return true;
        };
        let Some(turn_nonce) = non_empty_str(event, "turnNonce") else {
            return true;
        };
        let Some(agent_id) = non_empty_str(event, "agentId") else {
            return true;
        };
        let Some(host_auth_id) = non_empty_str(event, "hostAuthId") else {
            return true;
        };

        let Some(self_auth_id) = self.host.self_auth_id() else {
            return true;
        };
        let Some(room) = self
            .host
            .rooms()
            .into_iter()
            .find(|room| room.room_id == room_id)
        else {
            return true;
        };
        if room.host_auth_id != host_auth_id
            || !room
                .members
                .iter()
                .any(|member| member.auth_id == self_auth_id && member.agent_id.is_none())
            || !room.members.iter().any(|member| {
                member.auth_id == self_auth_id
                    && member.agent_id.as_deref() == Some(agent_id.as_str())
            })
            || !self.turn_dedupe.mark_seen_if_new(&turn_nonce)
        {
            return true;
        }

        let budget_key = format!("{room_id}\0{host_auth_id}\0{agent_id}");
        let budget = {
            let mut budgets = self
                .remote_turn_budgets
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            Arc::clone(budgets.entry(budget_key).or_insert_with(|| {
                Arc::new(RemoteTurnBudget::new(
                    REMOTE_TURN_BUDGET_MAX,
                    REMOTE_TURN_BUDGET_WINDOW_MS,
                ))
            }))
        };
        if !budget.accept(self.host.now_ms()) {
            return true;
        }

        let service = Arc::clone(self);
        let event = event.clone();
        let _ = thread::Builder::new()
            .name("sand-xuser-remote-turn".into())
            .spawn(move || service.run_requested_turn(&event, &room));
        true
    }

    fn run_requested_turn(&self, event: &Value, room: &XuserRoom) {
        let agent_id = non_empty_str(event, "agentId").unwrap_or_default();
        let room_id = non_empty_str(event, "roomId").unwrap_or_default();
        let turn_nonce = non_empty_str(event, "turnNonce").unwrap_or_default();

        let profile = match self.host.get_agent_display_profile(&agent_id) {
            Ok(Some(profile)) => profile,
            Ok(None) => {
                let _ = self.host.relay_remove_deleted_agent(&agent_id);
                let _ = self.host.reconcile_share_state();
                return;
            }
            Err(_) => {
                let _ = self.host.relay_send(&json!({
                    "kind": "turn-result",
                    "roomId": room_id,
                    "turnNonce": turn_nonce,
                    "agentId": agent_id,
                    "messages": [],
                }));
                return;
            }
        };

        let attempt = (|| -> Result<Vec<String>, String> {
            let host_name = room
                .members
                .iter()
                .find(|member| {
                    member.auth_id == room.host_auth_id && member.agent_id.is_none()
                })
                .and_then(|member| member.name.clone())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "Room host".into());
            self.host.append_shared_room_activity_notice(
                &room_id,
                &format!(
                    "{} is responding in {} at the request of {}.",
                    profile.name, room.name, host_name
                ),
            )?;

            let peers = event
                .get("peers")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(index, value)| {
                    let object = value.as_object();
                    GroupMember {
                        id: format!("peer:{index}"),
                        name: clamp_guest_name(
                            object
                                .and_then(|value| value.get("name"))
                                .and_then(Value::as_str)
                                .unwrap_or(""),
                        ),
                        description: object
                            .and_then(|value| value.get("description"))
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    }
                })
                .collect::<Vec<_>>();
            let group = GroupDescription {
                name: event
                    .get("groupName")
                    .and_then(Value::as_str)
                    .unwrap_or("Shared room")
                    .to_string(),
                description: event
                    .get("groupDescription")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            };
            let member = GroupMember {
                id: agent_id.clone(),
                name: profile.name,
                description: profile.description,
            };
            let normalized = normalize_turn_messages(
                event.get("newMessages").unwrap_or(&Value::Null),
            );
            let new_messages = normalized
                .into_iter()
                .map(|message| XuserTurnMessage {
                    speaker_kind: if message.speaker_kind == "agent" {
                        XuserSpeakerKind::Agent
                    } else {
                        XuserSpeakerKind::Human
                    },
                    speaker_name: message.speaker_name,
                    text: message.text,
                    is_self: message.is_self,
                })
                .collect::<Vec<_>>();
            let prompts = build_remote_member_turn_prompts(
                &member,
                &group,
                &peers,
                &host_name,
                &new_messages,
            );
            self.host.run_remote_requested_member_turn(
                &agent_id,
                &prompts.system_prompt,
                &prompts.prompt,
            )
        })();

        let messages = attempt
            .unwrap_or_default()
            .into_iter()
            .take(2)
            .collect::<Vec<_>>();
        let _ = self.host.relay_send(&json!({
            "kind": "turn-result",
            "roomId": room_id,
            "turnNonce": turn_nonce,
            "agentId": agent_id,
            "messages": messages,
        }));
    }

    pub fn handle_turn_result(&self, event: &Value) -> bool {
        let Some(turn_nonce) = non_empty_str(event, "turnNonce") else {
            return true;
        };
        let pending = self
            .pending_remote_turns
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&turn_nonce);
        let Some(pending) = pending else {
            return true;
        };
        let messages = event
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .take(2)
            .map(str::to_string)
            .collect::<Vec<_>>();
        let _ = pending.settle.send(messages);
        true
    }

    pub fn run_remote_member_turn(
        &self,
        member: &GroupMember,
        shared_room_id: &str,
        group: &GroupDescription,
        peers: &[GroupMember],
        new_messages: &[GroupMessage],
    ) -> Vec<String> {
        if !self.host.is_enabled() {
            return Vec::new();
        }
        let Some((owner_auth_id, remote_agent_id)) = parse_remote_agent_id(&member.id) else {
            return Vec::new();
        };
        let now = self.host.now_ms();
        if self
            .unreachable_remote_members
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&member.id)
            .is_some_and(|until| now < *until)
        {
            return Vec::new();
        }

        let turn_nonce = uuid::Uuid::new_v4().to_string();
        let (settle, receive) = mpsc::sync_channel(1);
        self.pending_remote_turns
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(turn_nonce.clone(), PendingRemoteTurn { settle });

        let wire_messages = to_xuser_turn_messages(new_messages, &member.id)
            .into_iter()
            .map(|message| {
                json!({
                    "speakerKind": match message.speaker_kind {
                        XuserSpeakerKind::Human => "human",
                        XuserSpeakerKind::Agent => "agent",
                    },
                    "speakerName": message.speaker_name,
                    "text": message.text,
                    "isSelf": message.is_self,
                })
            })
            .collect::<Vec<_>>();
        let wire_peers = peers
            .iter()
            .map(|peer| json!({"name": peer.name, "description": peer.description}))
            .collect::<Vec<_>>();
        let payload = json!({
            "kind": "turn-request",
            "roomId": shared_room_id,
            "turnNonce": turn_nonce,
            "ownerAuthId": owner_auth_id,
            "agentId": remote_agent_id,
            "groupName": group.name,
            "groupDescription": group.description,
            "peers": wire_peers,
            "newMessages": wire_messages,
        });
        if self.host.relay_send(&payload).is_err() {
            self.pending_remote_turns
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&turn_nonce);
            return Vec::new();
        }
        self.host.relay_request_drain();

        match receive.recv_timeout(Duration::from_millis(REMOTE_MEMBER_TURN_TIMEOUT_MS)) {
            Ok(messages) => {
                if !messages.is_empty() {
                    self.unreachable_remote_members
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .remove(&member.id);
                }
                messages
            }
            Err(_) => {
                self.pending_remote_turns
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&turn_nonce);
                self.unreachable_remote_members
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .insert(
                        member.id.clone(),
                        self.host
                            .now_ms()
                            .saturating_add(REMOTE_MEMBER_UNREACHABLE_BACKOFF_MS),
                    );
                Vec::new()
            }
        }
    }

    pub fn pending_remote_turn_count(&self) -> usize {
        self.pending_remote_turns
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .len()
    }

    pub fn unreachable_until_ms(&self, member_id: &str) -> Option<u64> {
        self.unreachable_remote_members
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(member_id)
            .copied()
    }
}

fn non_empty_str(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
