use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use super::xuser_state_reconcile::XuserRoom;
use super::xuser_turn_dedupe_store::XuserTurnDedupe;

pub const REMOTE_TURN_BUDGET_WINDOW_MS: u64 = 10 * 60_000;
pub const REMOTE_TURN_BUDGET_MAX: usize = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedRemoteTurn {
    pub room_id: String,
    pub turn_nonce: String,
    pub agent_id: String,
    pub host_auth_id: String,
}

#[derive(Debug, Default)]
struct RemoteTurnBudget {
    starts: VecDeque<u64>,
}

impl RemoteTurnBudget {
    fn accept(&mut self, now_ms: u64) -> bool {
        while self
            .starts
            .front()
            .is_some_and(|started| now_ms.saturating_sub(*started) >= REMOTE_TURN_BUDGET_WINDOW_MS)
        {
            self.starts.pop_front();
        }
        if self.starts.len() >= REMOTE_TURN_BUDGET_MAX {
            return false;
        }
        self.starts.push_back(now_ms);
        true
    }
}

/// Platform-neutral admission gate for inbound shared-room Agent turns.
///
/// This owns no Agent execution. Relay/service code must pass a request through this gate before
/// calling the canonical Host turn runtime. Durable nonce de-duplication survives process death;
/// host/member checks prevent a remote room from selecting an Agent that was not explicitly shared
/// by this installation.
pub struct XuserRemoteTurnAdmission {
    dedupe: Arc<dyn XuserTurnDedupe>,
    budgets: Mutex<HashMap<String, RemoteTurnBudget>>,
}

impl XuserRemoteTurnAdmission {
    pub fn new(dedupe: Arc<dyn XuserTurnDedupe>) -> Self {
        Self {
            dedupe,
            budgets: Mutex::new(HashMap::new()),
        }
    }

    pub fn admit(
        &self,
        event: &Value,
        self_auth_id: Option<&str>,
        rooms: &[XuserRoom],
        now_ms: u64,
    ) -> Result<Option<AdmittedRemoteTurn>, String> {
        let Some(self_auth_id) = self_auth_id.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(None);
        };
        let Some(room_id) = non_empty_str(event, "roomId") else {
            return Ok(None);
        };
        let Some(turn_nonce) = non_empty_str(event, "turnNonce") else {
            return Ok(None);
        };
        let Some(agent_id) = non_empty_str(event, "agentId") else {
            return Ok(None);
        };
        let Some(host_auth_id) = non_empty_str(event, "hostAuthId") else {
            return Ok(None);
        };
        let Some(room) = rooms.iter().find(|room| room.room_id == room_id) else {
            return Ok(None);
        };
        if room.host_auth_id != host_auth_id {
            return Ok(None);
        }

        let has_self_human = room
            .members
            .iter()
            .any(|member| member.auth_id == self_auth_id && member.agent_id.is_none());
        let has_self_agent = room.members.iter().any(|member| {
            member.auth_id == self_auth_id && member.agent_id.as_deref() == Some(agent_id.as_str())
        });
        if !has_self_human || !has_self_agent {
            return Ok(None);
        }

        if !self
            .dedupe
            .mark_seen_if_new(&turn_nonce)
            .map_err(|error| format!("xuser turn dedupe failed: {error}"))?
        {
            return Ok(None);
        }

        let key = format!("{room_id}\0{host_auth_id}\0{agent_id}");
        let mut budgets = self
            .budgets
            .lock()
            .map_err(|_| "xuser turn budget lock poisoned".to_string())?;
        if !budgets.entry(key).or_default().accept(now_ms) {
            return Ok(None);
        }

        Ok(Some(AdmittedRemoteTurn {
            room_id,
            turn_nonce,
            agent_id,
            host_auth_id,
        }))
    }

    pub fn clear_budgets(&self) {
        self.budgets
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::cross_user_sharing::xuser_state_reconcile::RoomMember;
    use crate::extensions::cross_user_sharing::xuser_turn_dedupe_store::XuserTurnDedupeStore;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-xuser-admission-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        ))
    }

    fn room() -> XuserRoom {
        XuserRoom {
            room_id: "room-a".into(),
            name: "Room A".into(),
            host_auth_id: "host".into(),
            members: vec![
                RoomMember {
                    auth_id: "self".into(),
                    agent_id: None,
                    name: Some("Me".into()),
                },
                RoomMember {
                    auth_id: "self".into(),
                    agent_id: Some("agent-a".into()),
                    name: Some("Agent".into()),
                },
            ],
            avatar_data_url: None,
        }
    }

    fn event(nonce: &str) -> Value {
        json!({
            "roomId":"room-a",
            "turnNonce":nonce,
            "agentId":"agent-a",
            "hostAuthId":"host"
        })
    }

    #[test]
    fn host_membership_and_durable_nonce_fence_remote_turns() {
        let root = root("fence");
        let gate = XuserRemoteTurnAdmission::new(Arc::new(XuserTurnDedupeStore::new(
            &root,
            Box::new(|| 1_000),
        )));
        assert!(
            gate.admit(&event("n1"), Some("self"), &[room()], 1_000)
                .unwrap()
                .is_some()
        );
        assert!(
            gate.admit(&event("n1"), Some("self"), &[room()], 1_001)
                .unwrap()
                .is_none()
        );
        let mut wrong_host = event("n2");
        wrong_host["hostAuthId"] = Value::String("other".into());
        assert!(
            gate.admit(&wrong_host, Some("self"), &[room()], 1_002)
                .unwrap()
                .is_none()
        );
        drop(gate);

        let reopened = XuserRemoteTurnAdmission::new(Arc::new(XuserTurnDedupeStore::new(
            &root,
            Box::new(|| 1_100),
        )));
        assert!(
            reopened
                .admit(&event("n1"), Some("self"), &[room()], 1_100)
                .unwrap()
                .is_none()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn budget_is_per_room_host_agent_and_recovers_after_window() {
        let root = root("budget");
        let gate = XuserRemoteTurnAdmission::new(Arc::new(XuserTurnDedupeStore::new(
            &root,
            Box::new(|| 2_000),
        )));
        for index in 0..REMOTE_TURN_BUDGET_MAX {
            assert!(
                gate.admit(
                    &event(&format!("n-{index}")),
                    Some("self"),
                    &[room()],
                    2_000,
                )
                .unwrap()
                .is_some()
            );
        }
        assert!(
            gate.admit(&event("overflow"), Some("self"), &[room()], 2_001)
                .unwrap()
                .is_none()
        );
        assert!(
            gate.admit(
                &event("after-window"),
                Some("self"),
                &[room()],
                2_000 + REMOTE_TURN_BUDGET_WINDOW_MS,
            )
            .unwrap()
            .is_some()
        );
        let _ = fs::remove_dir_all(root);
    }
}
