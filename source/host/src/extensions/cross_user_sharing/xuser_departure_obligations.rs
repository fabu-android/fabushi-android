use std::collections::BTreeSet;
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
    ) -> Result<(), String> {
        // Persist the side-effect obligation first. If tombstone persistence fails, a retry can
        // still reconcile the owed leave rather than losing a remote side effect after process death.
        self.pending
            .record(XuserDeparture::LeaveRoom {
                owner_auth_id: owner_auth_id.clone(),
                room_id: room_id.clone(),
                agent_id,
            })
            .map_err(|error| format!("failed to persist xuser leave obligation: {error}"))?;
        self.tombstones
            .record(XuserRoomTombstone {
                room_id,
                owner_auth_id,
                torn_down_at_ms: now_ms,
            })
            .map_err(|error| format!("failed to persist xuser room tombstone: {error}"))?;
        Ok(())
    }

    pub fn record_agent_removal(
        &self,
        owner_auth_id: Option<String>,
        agent_id: String,
    ) -> Result<(), String> {
        self.pending
            .record(XuserDeparture::RemoveAgent {
                owner_auth_id,
                agent_id,
            })
            .map(|_| ())
            .map_err(|error| format!("failed to persist xuser agent-removal obligation: {error}"))
    }

    pub fn drain(&self, relay: &dyn XuserDepartureRelay) -> Result<usize, String> {
        let mut completed = 0;
        for departure in self
            .pending
            .list()
            .map_err(|error| format!("failed to read xuser departure obligations: {error}"))?
        {
            let remote_result = match &departure {
                XuserDeparture::LeaveRoom { room_id, .. } => relay.leave_room(room_id),
                XuserDeparture::RemoveAgent { agent_id, .. } => {
                    relay.remove_deleted_agent(agent_id)
                }
            };
            if remote_result.is_err() {
                // Outcome is not promoted to success. Keep the durable obligation for explicit
                // reconciliation on the next allowed drain.
                continue;
            }
            self.pending
                .clear(&departure)
                .map_err(|error| format!("remote xuser departure succeeded but durable obligation could not be cleared: {error}"))?;
            completed += 1;
        }
        Ok(completed)
    }

    pub fn is_departure_owed(
        &self,
        room_id: &str,
        self_auth_id: Option<&str>,
    ) -> Result<bool, String> {
        Ok(self
            .pending
            .list()
            .map_err(|error| format!("failed to read xuser departure obligations: {error}"))?
            .iter()
            .any(|value| {
                matches!(
                    value,
                    XuserDeparture::LeaveRoom {
                        owner_auth_id,
                        room_id: owed_room_id,
                        ..
                    } if owed_room_id == room_id
                        && (owner_auth_id.is_none() || owner_auth_id.as_deref() == self_auth_id)
                )
            }))
    }

    pub fn is_room_abandoned(
        &self,
        room_id: &str,
        self_auth_id: Option<&str>,
    ) -> Result<bool, String> {
        let tombstoned = self
            .tombstones
            .list()
            .map_err(|error| format!("failed to read xuser room tombstones: {error}"))?
            .iter()
            .any(|value| {
                value.room_id == room_id
                    && (value.owner_auth_id.is_none()
                        || value.owner_auth_id.as_deref() == self_auth_id)
            });
        Ok(tombstoned || self.is_departure_owed(room_id, self_auth_id)?)
    }

    pub fn clear_room_tombstones(
        &self,
        room_id: &str,
        self_auth_id: Option<&str>,
    ) -> Result<usize, String> {
        let mut cleared = 0;
        for value in self
            .tombstones
            .list()
            .map_err(|error| format!("failed to read xuser room tombstones: {error}"))?
        {
            if value.room_id == room_id
                && (value.owner_auth_id.is_none()
                    || value.owner_auth_id.as_deref() == self_auth_id)
                && self
                    .tombstones
                    .clear(&value.room_id, value.owner_auth_id.as_deref())
                    .map_err(|error| format!("failed to clear xuser room tombstone: {error}"))?
            {
                cleared += 1;
            }
        }
        Ok(cleared)
    }

    pub fn prune_tombstones_for_registry(
        &self,
        now_ms: u64,
        registry_room_ids: &BTreeSet<String>,
        self_auth_id: Option<&str>,
    ) -> Result<usize, String> {
        let mut pruned = 0;
        for value in self
            .tombstones
            .list()
            .map_err(|error| format!("failed to read xuser room tombstones: {error}"))?
        {
            if (value.owner_auth_id.is_none()
                || value.owner_auth_id.as_deref() == self_auth_id)
                && !registry_room_ids.contains(&value.room_id)
                && now_ms.saturating_sub(value.torn_down_at_ms)
                    >= ROOM_TOMBSTONE_MIN_RETENTION_MS
                && self
                    .tombstones
                    .clear(&value.room_id, value.owner_auth_id.as_deref())
                    .map_err(|error| format!("failed to prune xuser room tombstone: {error}"))?
            {
                pruned += 1;
            }
        }
        Ok(pruned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-xuser-obligations-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[derive(Default)]
    struct Relay {
        leaves: Mutex<Vec<String>>,
        removals: Mutex<Vec<String>>,
        fail_room: Option<String>,
    }

    impl XuserDepartureRelay for Relay {
        fn leave_room(&self, room_id: &str) -> Result<(), String> {
            if self.fail_room.as_deref() == Some(room_id) {
                return Err("relay unavailable".into());
            }
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
        let root = root("success");
        let pending = Arc::new(SandXuserPendingDepartureStore::new(&root));
        let tombstones = Arc::new(SandXuserRoomTombstoneStore::new(&root));
        let owner =
            SandXuserDepartureObligations::new(Arc::clone(&pending), Arc::clone(&tombstones));
        owner
            .record_leave(
                Some("owner-a".into()),
                "room-a".into(),
                "agent-a".into(),
                100,
            )
            .unwrap();
        owner
            .record_agent_removal(Some("owner-a".into()), "agent-b".into())
            .unwrap();
        assert_eq!(pending.list().unwrap().len(), 2);
        assert!(owner.is_departure_owed("room-a", Some("owner-a")).unwrap());

        let relay = Relay::default();
        assert_eq!(owner.drain(&relay).unwrap(), 2);
        assert!(pending.list().unwrap().is_empty());
        assert_eq!(relay.leaves.lock().unwrap().as_slice(), &["room-a"]);
        assert_eq!(relay.removals.lock().unwrap().as_slice(), &["agent-b"]);
        assert!(owner.is_room_abandoned("room-a", Some("owner-a")).unwrap());
        assert_eq!(tombstones.list().unwrap().len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_remote_leave_remains_durable_across_reopen() {
        let root = root("retry");
        {
            let pending = Arc::new(SandXuserPendingDepartureStore::new(&root));
            let tombstones = Arc::new(SandXuserRoomTombstoneStore::new(&root));
            let owner = SandXuserDepartureObligations::new(pending, tombstones);
            owner
                .record_leave(None, "room-fail".into(), "agent-a".into(), 100)
                .unwrap();
            let relay = Relay {
                fail_room: Some("room-fail".into()),
                ..Relay::default()
            };
            assert_eq!(owner.drain(&relay).unwrap(), 0);
        }

        let pending = Arc::new(SandXuserPendingDepartureStore::new(&root));
        let tombstones = Arc::new(SandXuserRoomTombstoneStore::new(&root));
        let reopened = SandXuserDepartureObligations::new(Arc::clone(&pending), tombstones);
        assert!(reopened.is_departure_owed("room-fail", None).unwrap());
        assert_eq!(reopened.drain(&Relay::default()).unwrap(), 1);
        assert!(pending.list().unwrap().is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
