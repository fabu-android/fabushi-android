use super::xuser_entry_publisher::SandXuserEntryPublisher;
use super::xuser_relay::{SandXuserRelayClient, SandXuserRelayDriver};
use super::xuser_state_reconcile::{XuserRoom, reconcile_rooms};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

pub const STATE_RECONCILE_INTERVAL_MS: u64 = 5 * 60_000;
pub const SELF_IDENTITY_RETRY_MS: u64 = 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SandSharingState {
    pub is_enabled: bool,
    pub self_auth_id: Option<String>,
    pub pending_join_requests: Vec<Value>,
    pub rooms: Vec<XuserRoom>,
    pub typing_users: Vec<Value>,
}

pub trait XuserSharingManager: Send + Sync {
    fn get_agent_display_profile(
        &self,
        _agent_id: &str,
    ) -> Result<Option<(String, String)>, String> {
        Ok(None)
    }
    fn get_agent_avatar_data_url(&self, _agent_id: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn find_room_agent_id(&self, _room_id: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn is_room_abandoned(&self, _room_id: &str, _self_auth_id: Option<&str>) -> bool {
        false
    }
    fn install_room(&self, room: &XuserRoom, self_auth_id: Option<&str>) -> Result<(), String>;
    fn mark_mirror_room_revoked(&self, room_id: &str) -> Result<(), String>;
    fn post_shared_room_guest_message(&self, event: &Value) -> Result<(), String>;
    fn append_mirror_room_entry(
        &self,
        event: &Value,
        self_auth_id: &str,
    ) -> Result<bool, String>;
}

#[derive(Clone)]
pub struct SandXuserManagerDelegate {
    service: Weak<SandXuserSharingService>,
}

impl SandXuserManagerDelegate {
    pub fn is_enabled(&self) -> bool {
        self.service
            .upgrade()
            .is_some_and(|service| service.get_state().is_enabled)
    }

    pub fn publish_room_entry(&self, room_id: &str, entry: &Value) -> Result<(), String> {
        let Some(service) = self.service.upgrade() else {
            return Ok(());
        };
        if !service.get_state().is_enabled {
            return Ok(());
        }
        let publisher = service
            .entry_publisher
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .cloned();
        if let Some(publisher) = publisher {
            publisher.enqueue_publish(room_id, entry)?;
        }
        Ok(())
    }

    pub fn publish_room_entry_and_wait(
        &self,
        room_id: &str,
        entry: &Value,
    ) -> Result<(), String> {
        let Some(service) = self.service.upgrade() else {
            return Ok(());
        };
        if !service.get_state().is_enabled {
            return Ok(());
        }
        let publisher = service
            .entry_publisher
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .cloned();
        if let Some(publisher) = publisher {
            publisher.publish_entry_ordered_and_wait(room_id, entry)?;
        }
        Ok(())
    }
}

pub struct SandXuserSharingService {
    relay: Arc<SandXuserRelayClient>,
    relay_driver: Mutex<Option<Arc<SandXuserRelayDriver>>>,
    entry_publisher: Mutex<Option<Arc<SandXuserEntryPublisher>>>,
    manager: Mutex<Option<Arc<dyn XuserSharingManager>>>,
    state: Mutex<SandSharingState>,
    room_mint_lock: Mutex<()>,
    started: AtomicBool,
}
impl SandXuserSharingService {
    pub fn new(relay: Arc<SandXuserRelayClient>) -> Self {
        Self {
            relay,
            relay_driver: Mutex::new(None),
            entry_publisher: Mutex::new(None),
            manager: Mutex::new(None),
            state: Mutex::new(SandSharingState::default()),
            room_mint_lock: Mutex::new(()),
            started: AtomicBool::new(false),
        }
    }
    pub fn bind_relay_driver(&self, driver: Arc<SandXuserRelayDriver>) {
        let mut slot = self.relay_driver.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(previous) = slot.replace(driver) {
            previous.stop();
        }
    }

    pub fn bind_entry_publisher(&self, publisher: Arc<SandXuserEntryPublisher>) {
        *self
            .entry_publisher
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(publisher);
    }

    pub fn bind_manager(&self, manager: Arc<dyn XuserSharingManager>) {
        *self
            .manager
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(manager);
    }

    fn manager(&self) -> Option<Arc<dyn XuserSharingManager>> {
        self.manager
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .cloned()
    }

    pub fn build_manager_delegate(self: &Arc<Self>) -> SandXuserManagerDelegate {
        SandXuserManagerDelegate {
            service: Arc::downgrade(self),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        {
            let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
            s.is_enabled = enabled;
            if !enabled {
                s.pending_join_requests.clear();
                s.rooms.clear();
                s.typing_users.clear();
            }
        }
        if !enabled {
            self.stop();
        }
    }

    pub fn start(&self) -> Result<SandSharingState, String> {
        if !self.get_state().is_enabled {
            return Ok(self.get_state());
        }
        if self.started.swap(true, Ordering::AcqRel) {
            return Ok(self.get_state());
        }
        if let Err(error) = self.reconcile_share_state() {
            self.started.store(false, Ordering::Release);
            return Err(error);
        }
        if let Some(driver) = self
            .relay_driver
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .cloned()
        {
            driver.start();
        }
        Ok(self.get_state())
    }

    pub fn stop(&self) {
        if !self.started.swap(false, Ordering::AcqRel) {
            return;
        }
        if let Some(driver) = self
            .relay_driver
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .cloned()
        {
            driver.stop();
        }
    }

    pub fn request_relay_drain(&self) {
        if !self.started.load(Ordering::Acquire) {
            return;
        }
        if let Some(driver) = self
            .relay_driver
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .cloned()
        {
            driver.request_drain();
        }
    }

    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::Acquire)
    }
    pub fn set_self_auth_id(&self, id: Option<String>) {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .self_auth_id = id
    }
    pub fn get_state(&self) -> SandSharingState {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if !state.is_enabled {
            return SandSharingState::default();
        }
        let now_ms = system_now_ms();
        let self_auth_id = state.self_auth_id.as_deref();
        state.typing_users.retain(|user| {
            let expires_at_ms = user
                .get("expiresAtMs")
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite())
                .unwrap_or_default();
            let auth_id = user.get("authId").and_then(Value::as_str);
            expires_at_ms > now_ms && auth_id != self_auth_id
        });
        state
    }
    pub fn reconcile_share_state(&self) -> Result<SandSharingState, String> {
        let current = self.get_state();
        if !current.is_enabled {
            return Ok(current);
        }
        let raw = self.relay.fetch_share_state()?;
        let pending = raw
            .get("pendingJoinRequests")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let remote: Vec<XuserRoom> = serde_json::from_value(
            raw.get("rooms")
                .cloned()
                .unwrap_or_else(|| Value::Array(vec![])),
        )
        .map_err(|e| e.to_string())?;
        let result = reconcile_rooms(&current.rooms, remote, current.self_auth_id.as_deref());
        let rooms = result.rooms.clone();
        let revoked_room_ids = result.revoked_room_ids.clone();
        {
            let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
            s.pending_join_requests = pending;
            s.rooms = result.rooms;
        }
        if let Some(manager) = self.manager() {
            for room_id in revoked_room_ids {
                manager.mark_mirror_room_revoked(&room_id)?;
            }
            for room in &rooms {
                manager.install_room(room, current.self_auth_id.as_deref())?;
            }
        }
        Ok(self.get_state())
    }
    pub fn create_room_from_agent(&self, agent_id: &str) -> Result<Value, String> {
        let current = self.get_state();
        if !current.is_enabled {
            return Ok(serde_json::json!({
                "status": "error",
                "message": "Sharing isn't enabled."
            }));
        }
        let _mint_guard = self
            .room_mint_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(manager) = self.manager() else {
            return Ok(serde_json::json!({
                "status": "error",
                "message": "That agent no longer exists."
            }));
        };
        let Some((agent_name, _description)) = manager.get_agent_display_profile(agent_id)? else {
            return Ok(serde_json::json!({
                "status": "error",
                "message": "That agent no longer exists."
            }));
        };
        if let Some(self_auth_id) = current.self_auth_id.as_deref() {
            if let Some(existing_room_id) = current.rooms.iter().find_map(|room| {
                if room.host_auth_id != self_auth_id {
                    return None;
                }
                let agent_members = room
                    .members
                    .iter()
                    .filter(|member| member.agent_id.is_some())
                    .collect::<Vec<_>>();
                (agent_members.len() == 1
                    && agent_members[0].auth_id == self_auth_id
                    && agent_members[0].agent_id.as_deref() == Some(agent_id))
                .then_some(room.room_id.as_str())
            }) {
                return self.relay.create_room_invite(existing_room_id);
            }
        }
        let avatar_data_url = manager.get_agent_avatar_data_url(agent_id)?;
        let mut payload = serde_json::json!({
            "agentId": agent_id,
            "agentName": agent_name,
        });
        if let Some(avatar_data_url) = avatar_data_url {
            payload["avatarDataUrl"] = Value::String(avatar_data_url);
        }
        let result = self.relay.create_room_from_agent(&payload)?;
        if result.get("status").and_then(Value::as_str) == Some("ok") {
            let _ = self.reconcile_share_state();
        }
        Ok(result)
    }


    pub fn create_room_invite(&self, room_id: &str) -> Result<Value, String> {
        if !self.get_state().is_enabled {
            return Ok(serde_json::json!({
                "status": "error",
                "message": "Sharing isn't enabled."
            }));
        }
        self.relay.create_room_invite(room_id)
    }

    pub fn join_room(&self, link: &str) -> Result<Value, String> {
        if !self.get_state().is_enabled {
            return Ok(serde_json::json!({
                "status": "error",
                "message": "Sharing isn't enabled."
            }));
        }
        let result = self.relay.join_room(link)?;
        if result.get("status").and_then(Value::as_str) == Some("already-member") {
            let _ = self.reconcile_share_state();
        }
        Ok(result)
    }

    pub fn respond_to_join_request(&self, payload: &Value) -> Result<SandSharingState, String> {
        if !self.get_state().is_enabled {
            return Ok(self.get_state());
        }
        let request_id = payload
            .get("requestId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let result = self.relay.respond_to_join_request(payload)?;
        {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(request_id) = request_id.as_deref() {
                state.pending_join_requests.retain(|request| {
                    request.get("requestId").and_then(Value::as_str) != Some(request_id)
                });
            }
        }
        if result.get("status").and_then(Value::as_str) == Some("approved") {
            if let Some(room) = result.get("room") {
                self.install_room_value(room)?;
            }
        }
        Ok(self.get_state())
    }

    pub fn create_shared_room(&self, payload: &Value) -> Result<Value, String> {
        let current = self.get_state();
        if !current.is_enabled {
            return Ok(serde_json::json!({
                "status": "error",
                "message": "Sharing isn't enabled."
            }));
        }
        if current.self_auth_id.is_none() {
            return Ok(serde_json::json!({
                "status": "error",
                "message": "Sign in to create shared groups."
            }));
        }
        let mut payload = payload.clone();
        if let Some(manager) = self.manager() {
            if let Some(agents) = payload.get_mut("agents").and_then(Value::as_array_mut) {
                for agent in agents {
                    let Some(agent_id) = agent.get("agentId").and_then(Value::as_str) else {
                        continue;
                    };
                    if let Some(avatar_data_url) = manager.get_agent_avatar_data_url(agent_id)? {
                        if let Some(object) = agent.as_object_mut() {
                            object.insert(
                                "avatarDataUrl".into(),
                                Value::String(avatar_data_url),
                            );
                        }
                    }
                }
            }
        }
        let result = self.relay.create_room(&payload)?;
        if result.get("status").and_then(Value::as_str) != Some("created") {
            return Ok(serde_json::json!({
                "status": "error",
                "message": "The room couldn't be created."
            }));
        }
        let Some(room) = result.get("room") else {
            return Ok(serde_json::json!({
                "status": "error",
                "message": "The room couldn't be created."
            }));
        };
        let parsed: XuserRoom = serde_json::from_value(room.clone()).map_err(|e| e.to_string())?;
        let room_id = parsed.room_id.clone();
        self.install_room(parsed)?;
        let local_room_agent_id = self
            .manager()
            .map(|manager| manager.find_room_agent_id(&room_id))
            .transpose()?
            .flatten();
        Ok(serde_json::json!({
            "status": "ok",
            "roomId": room_id,
            "localRoomAgentId": local_room_agent_id,
        }))
    }

    pub fn add_own_agent(&self, payload: &Value) -> Result<SandSharingState, String> {
        if !self.get_state().is_enabled {
            return Ok(self.get_state());
        }
        let mut payload = payload.clone();
        if let (Some(manager), Some(agent_id)) = (
            self.manager(),
            payload.get("agentId").and_then(Value::as_str).map(str::to_string),
        ) {
            if let Some(avatar_data_url) = manager.get_agent_avatar_data_url(&agent_id)? {
                if let Some(object) = payload.as_object_mut() {
                    object.insert(
                        "avatarDataUrl".into(),
                        Value::String(avatar_data_url),
                    );
                }
            }
        }
        let result = self.relay.add_own_agent(&payload)?;
        if let Some(room) = result.get("room") {
            self.install_room_value(room)?;
        }
        Ok(self.get_state())
    }

    pub fn remove_own_agent(&self, room_id: &str, agent_id: &str) -> Result<SandSharingState, String> {
        if !self.get_state().is_enabled {
            return Ok(self.get_state());
        }
        let result = self.relay.remove_own_agent(&serde_json::json!({
            "roomId": room_id,
            "agentId": agent_id,
        }))?;
        if let Some(room) = result.get("room") {
            self.install_room_value(room)?;
        }
        Ok(self.get_state())
    }

    pub fn leave_shared_room(
        &self,
        room_id: &str,
        target_auth_id: Option<&str>,
    ) -> Result<SandSharingState, String> {
        let current = self.get_state();
        if !current.is_enabled {
            return Ok(current);
        }
        let _ = self.relay.leave_room_with_target(room_id, target_auth_id)?;
        if target_auth_id.is_none() || target_auth_id == current.self_auth_id.as_deref() {
            self.state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .rooms
                .retain(|room| room.room_id != room_id);
        }
        let _ = self.reconcile_share_state();
        Ok(self.get_state())
    }

    pub fn note_agent_deleted(&self, agent_id: &str) -> Result<(), String> {
        if !self.get_state().is_enabled {
            return Ok(());
        }
        let _ = self.relay.remove_deleted_agent(agent_id)?;
        let _ = self.reconcile_share_state();
        Ok(())
    }

    pub fn set_room_typing(&self, room_id: &str, is_typing: bool) -> Result<(), String> {
        if !self.get_state().is_enabled {
            return Ok(());
        }
        let _ = self.relay.send(&serde_json::json!({
            "kind": "room-typing",
            "roomId": room_id,
            "isTyping": is_typing,
        }))?;
        Ok(())
    }

    pub fn prepare_for_upgrade(&self) {
        self.stop();
    }

    pub fn handle_event(&self, event: &Value) -> Result<bool, String> {
        if !self.get_state().is_enabled {
            return Ok(true);
        }
        match event.get("kind").and_then(Value::as_str) {
            Some("room-join-request") => {
                let Some(request) = event.get("request").filter(|value| value.is_object()) else {
                    return Ok(true);
                };
                let Some(request_id) = request.get("requestId").and_then(Value::as_str) else {
                    return Ok(true);
                };
                let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
                if state.pending_join_requests.iter().any(|current| {
                    current.get("requestId").and_then(Value::as_str) == Some(request_id)
                }) {
                    return Ok(true);
                }
                let requester_name = clamp_guest_name(
                    request
                        .get("requesterName")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                );
                let mut normalized = serde_json::json!({
                    "requestId": request_id,
                    "roomId": request.get("roomId").and_then(Value::as_str).unwrap_or_default(),
                    "roomName": request.get("roomName").and_then(Value::as_str).unwrap_or("Shared room"),
                    "requesterAuthId": request.get("requesterAuthId").and_then(Value::as_str).unwrap_or_default(),
                    "requesterName": requester_name,
                    "createdAtMs": request.get("createdAtMs").and_then(Value::as_f64).filter(|value| value.is_finite()).unwrap_or_else(system_now_ms),
                });
                if let Some(avatar_url) = request
                    .get("requesterAvatarUrl")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                {
                    normalized["requesterAvatarUrl"] = Value::String(avatar_url.to_string());
                }
                state.pending_join_requests.push(normalized);
                Ok(true)
            }
            Some("room-join-decision") => {
                if event.get("isApproved").and_then(Value::as_bool) == Some(true) {
                    if let Some(room) = event.get("room") {
                        self.install_room_value(room)?;
                    }
                }
                Ok(true)
            }
            Some("room-upsert") => {
                if let Some(room) = event.get("room") {
                    self.install_room_value(room)?;
                }
                Ok(true)
            }
            Some("room-typing") => {
                let Some(user) = event.get("user").filter(|value| value.is_object()) else {
                    return Ok(true);
                };
                let room_id = user.get("roomId").and_then(Value::as_str).unwrap_or_default();
                let auth_id = user.get("authId").and_then(Value::as_str).unwrap_or_default();
                let name = user.get("name").and_then(Value::as_str).unwrap_or_default();
                let expires_at_ms = user
                    .get("expiresAtMs")
                    .and_then(Value::as_f64)
                    .filter(|value| value.is_finite())
                    .unwrap_or_default();
                if room_id.is_empty() || auth_id.is_empty() || name.is_empty() {
                    return Ok(true);
                }
                let state_snapshot = self.get_state();
                let is_human_member = state_snapshot
                    .rooms
                    .iter()
                    .find(|room| room.room_id == room_id)
                    .is_some_and(|room| {
                        room.members.iter().any(|member| {
                            member.auth_id == auth_id && member.agent_id.is_none()
                        })
                    });
                if !is_human_member {
                    return Ok(true);
                }
                let key_matches = |value: &Value| {
                    value.get("roomId").and_then(Value::as_str) == Some(room_id)
                        && value.get("authId").and_then(Value::as_str) == Some(auth_id)
                };
                let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
                state.typing_users.retain(|value| !key_matches(value));
                if event.get("isTyping").and_then(Value::as_bool) == Some(true)
                    && expires_at_ms > system_now_ms()
                {
                    let mut normalized = serde_json::json!({
                        "roomId": room_id,
                        "authId": auth_id,
                        "name": clamp_guest_name(name),
                        "expiresAtMs": expires_at_ms,
                    });
                    if let Some(avatar_url) = user
                        .get("avatarUrl")
                        .and_then(Value::as_str)
                        .filter(|value| !value.is_empty())
                    {
                        normalized["avatarUrl"] = Value::String(avatar_url.to_string());
                    }
                    state.typing_users.push(normalized);
                }
                Ok(true)
            }
            Some("room-post") => {
                let Some(room_id) = event.get("roomId").and_then(Value::as_str) else {
                    return Ok(true);
                };
                let Some(manager) = self.manager() else {
                    return Ok(true);
                };
                let self_auth_id = self.get_state().self_auth_id;
                if manager.is_room_abandoned(room_id, self_auth_id.as_deref()) {
                    return Ok(true);
                }
                let author_auth_id = event
                    .get("authorAuthId")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let mut normalized = serde_json::json!({
                    "kind": "room-post",
                    "roomId": room_id,
                    "authorAuthId": author_auth_id,
                    "authorName": clamp_guest_name(
                        event.get("authorName").and_then(Value::as_str).unwrap_or_default()
                    ),
                    "text": event.get("text").and_then(Value::as_str).unwrap_or_default(),
                    "images": event.get("images").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                });
                if let Some(timestamp_ms) = event
                    .get("timestampMs")
                    .and_then(Value::as_f64)
                    .filter(|value| value.is_finite())
                {
                    normalized["timestampMs"] = serde_json::json!(timestamp_ms);
                }
                manager.post_shared_room_guest_message(&normalized)?;
                Ok(true)
            }
            Some("room-entry") => {
                let Some(room_id) = event.get("roomId").and_then(Value::as_str) else {
                    return Ok(true);
                };
                let Some(self_auth_id) = self.get_state().self_auth_id else {
                    return Ok(false);
                };
                let Some(manager) = self.manager() else {
                    return Ok(false);
                };
                if manager.is_room_abandoned(room_id, Some(&self_auth_id)) {
                    return Ok(true);
                }
                manager.append_mirror_room_entry(event, &self_auth_id)
            }
            Some("member-left") | Some("room-ended") => {
                if let Some(room_id) = event.get("roomId").and_then(Value::as_str) {
                    if let Some(manager) = self.manager() {
                        manager.mark_mirror_room_revoked(room_id)?;
                    }
                    self.state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .rooms
                        .retain(|room| room.room_id != room_id);
                    let _ = self.reconcile_share_state();
                }
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    pub fn install_room_value(&self, room: &Value) -> Result<(), String> {
        let parsed: XuserRoom = serde_json::from_value(room.clone()).map_err(|e| e.to_string())?;
        self.install_room(parsed)
    }

    fn install_room(&self, room: XuserRoom) -> Result<(), String> {
        let self_auth_id = self.get_state().self_auth_id;
        {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state.rooms.retain(|current| current.room_id != room.room_id);
            state.rooms.push(room.clone());
            state.rooms.sort_by(|left, right| left.room_id.cmp(&right.room_id));
        }
        if let Some(manager) = self.manager() {
            manager.install_room(&room, self_auth_id.as_deref())?;
        }
        Ok(())
    }
}

fn clamp_guest_name(value: &str) -> String {
    let one_line = value
        .replace(['\r', '\n'], " ")
        .trim()
        .chars()
        .take(120)
        .collect::<String>();
    if one_line.is_empty() {
        "Someone".into()
    } else {
        one_line
    }
}

fn system_now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as f64
}
