use std::sync::Arc;

use super::xuser_pending_departure_store::{SandXuserPendingDepartureStore, XuserDeparture};
use super::xuser_room_tombstone_store::{SandXuserRoomTombstoneStore, XuserRoomTombstone};

pub const ROOM_TOMBSTONE_MIN_RETENTION_MS: u64 = 48 * 60 * 60_000;

pub trait XuserDepartureRelay: Send + Sync {
    fn leave_room(&self, room_id: &str) -> Result<(), String>;
    fn remove_deleted_agent(&self, agent_id: &str) -> Result<(), String>;
}

#[derive(Clone)]
pub struct SandXuserDepartureObligations {
    pub pending: Arc<SandXuserPendingDepartureStore>,
    pub tombstones: Arc<SandXuserRoomTombstoneStore>,
}

impl SandXuserDepartureObligations {
    pub fn new(
        pending: Arc<SandXuserPendingDepartureStore>,
        tombstones: Arc<SandXuserRoomTombstoneStore>,
    ) -> Self {
        Self { pending, tombstones }
    }

    pub fn record_leave(
        &self,
        owner_auth_id: Option<String>,
        room_id: String,
        agent_id: String,
        now_ms: u64,
    ) {
        self.pending.record(XuserDeparture::LeaveRoom {
            owner_auth_id: owner_auth_id.clone(),
            room_id: room_id.clone(),
            agent_id,
        });
        self.tombstones.record(XuserRoomTombstone {
            room_id,
            owner_auth_id,
            torn_down_at_ms: now_ms,
        });
    }

    pub fn record_agent_removal(&self, owner_auth_id: Option<String>, agent_id: String) {
        self.pending.record(XuserDeparture::RemoveAgent {
            owner_auth_id,
            agent_id,
        })
    }

    pub fn drain(&self, relay: &dyn XuserDepartureRelay) -> usize {
        let mut completed = 0;
        for departure in self.pending.list() {
            let result = match &departure {
                XuserDeparture::LeaveRoom { room_id, .. } => relay.leave_room(room_id),
                XuserDeparture::RemoveAgent { agent_id, .. } => {
                    relay.remove_deleted_agent(agent_id)
                }
            };
            if result.is_ok() {
                self.pending.clear(&departure);
                completed += 1;
            }
        }
        completed
    }

    pub fn is_departure_owed(&self, room_id: &str, self_auth_id: Option<&str>) -> bool {
        self.pending.list().iter().any(|value| {
            matches!(
                value,
                XuserDeparture::LeaveRoom {
                    owner_auth_id,
                    room_id: owed_room_id,
                    ..
                } if owed_room_id == room_id
                    && (owner_auth_id.is_none() || owner_auth_id.as_deref() == self_auth_id)
            )
        })
    }

    pub fn is_room_abandoned(&self, room_id: &str, self_auth_id: Option<&str>) -> bool {
        self.tombstones.list().iter().any(|value| {
            value.room_id == room_id
                && (value.owner_auth_id.is_none()
                    || value.owner_auth_id.as_deref() == self_auth_id)
        }) || self.is_departure_owed(room_id, self_auth_id)
    }

    pub fn clear_room_tombstones(&self, room_id: &str, self_auth_id: Option<&str>) {
        for value in self.tombstones.list() {
            if value.room_id == room_id
                && (value.owner_auth_id.is_none()
                    || value.owner_auth_id.as_deref() == self_auth_id)
            {
                self.tombstones
                    .clear(&value.room_id, value.owner_auth_id.as_deref());
            }
        }
    }

    pub fn prune_tombstones_for_registry(
        &self,
        now_ms: u64,
        registry_room_ids: &std::collections::BTreeSet<String>,
        self_auth_id: Option<&str>,
    ) {
        for value in self.tombstones.list() {
            if (value.owner_auth_id.is_none()
                || value.owner_auth_id.as_deref() == self_auth_id)
                && !registry_room_ids.contains(&value.room_id)
                && now_ms.saturating_sub(value.torn_down_at_ms)
                    >= ROOM_TOMBSTONE_MIN_RETENTION_MS
            {
                self.tombstones
                    .clear(&value.room_id, value.owner_auth_id.as_deref());
            }
        }
    }
}
