use std::collections::BTreeSet;
use std::io;
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
        Self {
            pending,
            tombstones,
        }
    }

    pub fn record_leave(
        &self,
        owner_auth_id: Option<String>,
        room_id: String,
        agent_id: String,
        now_ms: u64,
    ) -> io::Result<()> {
        if room_id.trim().is_empty() || agent_id.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "xuser leave room and agent ids are required",
            ));
        }

        // The pending obligation is the recovery authority: persist it first so a process death
        // can never turn an uncertain leave into "nothing owed". A missing tombstone is safe
        // because is_room_abandoned also checks the pending obligation.
        self.pending.record(XuserDeparture::LeaveRoom {
            owner_auth_id: owner_auth_id.clone(),
            room_id: room_id.clone(),
            agent_id,
        })?;
        self.tombstones.record(XuserRoomTombstone {
            room_id,
            owner_auth_id,
            torn_down_at_ms: now_ms,
        })?;
        Ok(())
    }

    pub fn record_agent_removal(
        &self,
        owner_auth_id: Option<String>,
        agent_id: String,
    ) -> io::Result<()> {
        if agent_id.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "xuser removed agent id is required",
            ));
        }
        self.pending
            .record(XuserDeparture::RemoveAgent {
                owner_auth_id,
                agent_id,
            })
            .map(|_| ())
    }

    pub fn drain(&self, relay: &dyn XuserDepartureRelay) -> io::Result<usize> {
        let mut completed = 0;
        for departure in self.pending.list()? {
            let result = match &departure {
                XuserDeparture::LeaveRoom { room_id, .. } => relay.leave_room(room_id),
                XuserDeparture::RemoveAgent { agent_id, .. } => {
                    relay.remove_deleted_agent(agent_id)
                }
            };
            if result.is_ok() && self.pending.clear(&departure)? {
                completed += 1;
            }
        }
        Ok(completed)
    }

    pub fn is_departure_owed(
        &self,
        room_id: &str,
        self_auth_id: Option<&str>,
    ) -> io::Result<bool> {
        Ok(self.pending.list()?.iter().any(|value| {
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
    ) -> io::Result<bool> {
        let tombstoned = self.tombstones.list()?.iter().any(|value| {
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
    ) -> io::Result<usize> {
        let mut cleared = 0;
        for value in self.tombstones.list()? {
            if value.room_id == room_id
                && (value.owner_auth_id.is_none()
                    || value.owner_auth_id.as_deref() == self_auth_id)
                && self
                    .tombstones
                    .clear(&value.room_id, value.owner_auth_id.as_deref())?
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
    ) -> io::Result<usize> {
        let mut pruned = 0;
        for value in self.tombstones.list()? {
            if (value.owner_auth_id.is_none()
                || value.owner_auth_id.as_deref() == self_auth_id)
                && !registry_room_ids.contains(&value.room_id)
                && now_ms.saturating_sub(value.torn_down_at_ms)
                    >= ROOM_TOMBSTONE_MIN_RETENTION_MS
                && self
                    .tombstones
                    .clear(&value.room_id, value.owner_auth_id.as_deref())?
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

    #[derive(Default)]
    struct Relay {
        leaves: Mutex<Vec<String>>,
        removals: Mutex<Vec<String>>,
        fail_leave: bool,
    }

    impl XuserDepartureRelay for Relay {
        fn leave_room(&self, room_id: &str) -> Result<(), String> {
            self.leaves.lock().unwrap().push(room_id.to_string());
            if self.fail_leave {
                Err("relay unavailable".into())
            } else {
                Ok(())
            }
        }

        fn remove_deleted_agent(&self, agent_id: &str) -> Result<(), String> {
            self.removals.lock().unwrap().push(agent_id.to_string());
            Ok(())
        }
    }

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-xuser-obligations-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        ))
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
        assert!(owner
            .is_departure_owed("room-a", Some("owner-a"))
            .unwrap());

        let relay = Relay::default();
        assert_eq!(owner.drain(&relay).unwrap(), 2);
        assert!(pending.list().unwrap().is_empty());
        assert_eq!(relay.leaves.lock().unwrap().as_slice(), &["room-a"]);
        assert_eq!(relay.removals.lock().unwrap().as_slice(), &["agent-b"]);
        assert!(owner
            .is_room_abandoned("room-a", Some("owner-a"))
            .unwrap());
        assert_eq!(tombstones.list().unwrap().len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_relay_keeps_durable_obligation_for_restart_retry() {
        let root = root("failure");
        let pending = Arc::new(SandXuserPendingDepartureStore::new(&root));
        let tombstones = Arc::new(SandXuserRoomTombstoneStore::new(&root));
        let owner =
            SandXuserDepartureObligations::new(Arc::clone(&pending), Arc::clone(&tombstones));

        owner
            .record_leave(None, "room-a".into(), "agent-a".into(), 100)
            .unwrap();
        assert_eq!(
            owner
                .drain(&Relay {
                    fail_leave: true,
                    ..Relay::default()
                })
                .unwrap(),
            0
        );
        drop(owner);
        drop(pending);
        drop(tombstones);

        let reopened_pending = Arc::new(SandXuserPendingDepartureStore::new(&root));
        let reopened_tombstones = Arc::new(SandXuserRoomTombstoneStore::new(&root));
        let reopened = SandXuserDepartureObligations::new(
            Arc::clone(&reopened_pending),
            Arc::clone(&reopened_tombstones),
        );
        assert!(reopened
            .is_departure_owed("room-a", None)
            .unwrap());
        assert_eq!(reopened.drain(&Relay::default()).unwrap(), 1);
        assert!(reopened_pending.list().unwrap().is_empty());
        assert!(reopened
            .is_room_abandoned("room-a", None)
            .unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tombstones_are_pruned_only_after_retention_and_registry_absence() {
        let root = root("prune");
        let pending = Arc::new(SandXuserPendingDepartureStore::new(&root));
        let tombstones = Arc::new(SandXuserRoomTombstoneStore::new(&root));
        let owner =
            SandXuserDepartureObligations::new(Arc::clone(&pending), Arc::clone(&tombstones));
        owner
            .record_leave(None, "room-a".into(), "agent-a".into(), 100)
            .unwrap();
        owner.drain(&Relay::default()).unwrap();

        let registry = BTreeSet::new();
        assert_eq!(
            owner
                .prune_tombstones_for_registry(
                    100 + ROOM_TOMBSTONE_MIN_RETENTION_MS - 1,
                    &registry,
                    None,
                )
                .unwrap(),
            0
        );
        assert_eq!(
            owner
                .prune_tombstones_for_registry(
                    100 + ROOM_TOMBSTONE_MIN_RETENTION_MS,
                    &registry,
                    None,
                )
                .unwrap(),
            1
        );
        assert!(!owner.is_room_abandoned("room-a", None).unwrap());
        let _ = fs::remove_dir_all(root);
    }
}
