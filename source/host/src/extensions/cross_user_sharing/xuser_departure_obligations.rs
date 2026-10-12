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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Relay { leaves: Mutex<Vec<String>>, removals: Mutex<Vec<String>> }
    impl XuserDepartureRelay for Relay {
        fn leave_room(&self, room_id: &str) -> Result<(), String> {
            self.leaves.lock().unwrap().push(room_id.to_string());
            Ok(())
        }
        fn remove_deleted_agent(&self, agent_id: &str) -> Result<(), String> {
            self.removals.lock().unwrap().push(agent_id.to_string());
            Ok(())
        }
    }

    #[test]
    fn successful_drain_clears_pending_but_retains_room_tombstone() {
        let root = tempfile::tempdir().unwrap();
        let pending = Arc::new(SandXuserPendingDepartureStore::new(root.path()));
        let tombstones = Arc::new(SandXuserRoomTombstoneStore::new(root.path()));
        let owner = SandXuserDepartureObligations::new(Arc::clone(&pending), Arc::clone(&tombstones));
        owner.record_leave(Some("owner-a".into()), "room-a".into(), "agent-a".into(), 100);
        owner.record_agent_removal(Some("owner-a".into()), "agent-b".into());
        assert_eq!(pending.list().len(), 2);
        assert!(owner.is_departure_owed("room-a", Some("owner-a")));
        let relay = Relay::default();
        assert_eq!(owner.drain(&relay), 2);
        assert!(pending.list().is_empty());
        assert_eq!(relay.leaves.lock().unwrap().as_slice(), &["room-a"]);
        assert_eq!(relay.removals.lock().unwrap().as_slice(), &["agent-b"]);
        assert!(owner.is_room_abandoned("room-a", Some("owner-a")));
        assert_eq!(tombstones.list().len(), 1);
    }
}
