use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomMember {
    pub auth_id: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XuserRoom {
    pub room_id: String,
    pub name: String,
    pub host_auth_id: String,
    #[serde(default)]
    pub members: Vec<RoomMember>,
    #[serde(default)]
    pub avatar_data_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReconcileResult {
    pub rooms: Vec<XuserRoom>,
    pub revoked_room_ids: Vec<String>,
    pub added_room_ids: Vec<String>,
}

pub fn reconcile_rooms(
    previous: &[XuserRoom],
    remote: Vec<XuserRoom>,
    self_auth_id: Option<&str>,
) -> ReconcileResult {
    let old: BTreeMap<_, _> = previous
        .iter()
        .map(|room| (room.room_id.clone(), room.clone()))
        .collect();
    let new: BTreeMap<_, _> = remote
        .into_iter()
        .filter(|room| !room.room_id.is_empty())
        .map(|room| (room.room_id.clone(), room))
        .collect();
    let old_ids: BTreeSet<_> = old.keys().cloned().collect();
    let new_ids: BTreeSet<_> = new.keys().cloned().collect();

    let mut revoked_room_ids = old_ids.difference(&new_ids).cloned().collect::<Vec<_>>();
    let mut added_room_ids = new_ids.difference(&old_ids).cloned().collect::<Vec<_>>();
    revoked_room_ids.sort();
    added_room_ids.sort();

    let mut rooms = new
        .into_values()
        .filter(|room| match self_auth_id {
            None => true,
            Some(id) => {
                room.members.is_empty()
                    || room.members.iter().any(|member| member.auth_id == id)
                    || room.host_auth_id == id
            }
        })
        .collect::<Vec<_>>();
    rooms.sort_by(|left, right| left.room_id.cmp(&right.room_id));

    ReconcileResult {
        rooms,
        revoked_room_ids,
        added_room_ids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(id: &str, host: &str, members: &[&str]) -> XuserRoom {
        XuserRoom {
            room_id: id.to_string(),
            name: format!("Room {id}"),
            host_auth_id: host.to_string(),
            members: members
                .iter()
                .map(|auth_id| RoomMember {
                    auth_id: (*auth_id).to_string(),
                    agent_id: None,
                    name: None,
                })
                .collect(),
            avatar_data_url: None,
        }
    }

    #[test]
    fn reconcile_deduplicates_sorts_and_reports_remote_set_differences() {
        let previous = vec![room("a", "self", &["self"]), room("b", "self", &["self"])];
        let result = reconcile_rooms(
            &previous,
            vec![
                room("c", "remote", &["self"]),
                room("b", "remote", &["self"]),
                room("c", "self", &["self"]),
                room("", "self", &["self"]),
            ],
            Some("self"),
        );

        assert_eq!(
            result.rooms.iter().map(|room| room.room_id.as_str()).collect::<Vec<_>>(),
            vec!["b", "c"],
        );
        assert_eq!(result.rooms[1].host_auth_id, "self", "last duplicate room wins");
        assert_eq!(result.revoked_room_ids, vec!["a"]);
        assert_eq!(result.added_room_ids, vec!["c"]);
    }

    #[test]
    fn membership_fence_filters_rooms_but_diff_tracks_canonical_remote_ids() {
        let result = reconcile_rooms(
            &[],
            vec![
                room("hosted", "self", &[]),
                room("member", "remote", &["self"]),
                room("foreign", "remote", &["someone-else"]),
                room("open", "remote", &[]),
            ],
            Some("self"),
        );

        assert_eq!(
            result.rooms.iter().map(|room| room.room_id.as_str()).collect::<Vec<_>>(),
            vec!["hosted", "member", "open"],
        );
        assert_eq!(
            result.added_room_ids,
            vec!["foreign", "hosted", "member", "open"],
            "added/revoked is computed from canonical remote ids before local visibility filtering",
        );
    }
}
