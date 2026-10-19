use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io,
    path::{Path, PathBuf},
};

const CONTACT_GROUPS_SCHEMA_VERSION: u8 = 1;
pub const CONTACT_GROUP_PINNED_ID: &str = "__pinned__";
pub const CONTACT_GROUP_UNASSIGNED_ID: &str = "__unassigned__";
const MAX_GROUPS: usize = 1_024;
const MAX_ID_BYTES: usize = 256;
const MAX_NAME_BYTES: usize = 512;
const MAX_PEERS_PER_GROUP: usize = 16_384;
const MAX_OPERATION_HISTORY: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidContactGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub peer_ids: Vec<String>,
    #[serde(default)]
    pub is_collapsed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidContactGroupsSnapshot {
    pub account_fence: String,
    pub revision: u64,
    pub groups: Vec<AndroidContactGroup>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountState {
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    groups: Vec<AndroidContactGroup>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppliedOperation {
    account_fence: String,
    expected_revision: u64,
    groups: Vec<AndroidContactGroup>,
    result: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContactGroupsFile {
    schema_version: u8,
    #[serde(default)]
    accounts: BTreeMap<String, AccountState>,
    #[serde(default)]
    operations: BTreeMap<String, AppliedOperation>,
    #[serde(default)]
    operation_order: Vec<String>,
}

impl Default for ContactGroupsFile {
    fn default() -> Self {
        Self {
            schema_version: CONTACT_GROUPS_SCHEMA_VERSION,
            accounts: BTreeMap::new(),
            operations: BTreeMap::new(),
            operation_order: Vec::new(),
        }
    }
}

pub struct AndroidContactGroups {
    path: PathBuf,
    state: ContactGroupsFile,
}

impl AndroidContactGroups {
    pub fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        if !path.exists() {
            return Ok(Self { path, state: ContactGroupsFile::default() });
        }
        let raw = fs::read_to_string(&path)?;
        match serde_json::from_str::<ContactGroupsFile>(&raw) {
            Ok(mut state) if state.schema_version == CONTACT_GROUPS_SCHEMA_VERSION => {
                normalize_loaded(&mut state);
                Ok(Self { path, state })
            }
            _ => {
                let quarantine = corrupt_path(&path);
                fs::rename(&path, &quarantine)?;
                Ok(Self { path, state: ContactGroupsFile::default() })
            }
        }
    }

    pub fn get_snapshot(
        &self,
        account_fence: &str,
        known_peer_ids: &BTreeSet<String>,
        pinned_peer_ids: &BTreeSet<String>,
        search_active: bool,
    ) -> AndroidContactGroupsSnapshot {
        let account = self.state.accounts.get(account_fence).cloned().unwrap_or_default();
        AndroidContactGroupsSnapshot {
            account_fence: account_fence.to_string(),
            revision: account.revision,
            groups: project_groups(account.groups, known_peer_ids, pinned_peer_ids, search_active),
        }
    }

    pub fn set(
        &mut self,
        account_fence: &str,
        operation_id: &str,
        expected_revision: u64,
        groups: &[AndroidContactGroup],
        known_peer_ids: &BTreeSet<String>,
        pinned_peer_ids: &BTreeSet<String>,
    ) -> Result<Value, String> {
        validate_identity("account fence", account_fence)?;
        validate_identity("operation", operation_id)?;
        let normalized = normalize_input(groups, known_peer_ids)?;
        let operation_key = format!("{account_fence}\n{operation_id}");
        if let Some(applied) = self.state.operations.get(&operation_key) {
            if applied.account_fence != account_fence
                || applied.expected_revision != expected_revision
                || applied.groups != normalized
            {
                return Err("contact-group operation identity was reused with different input".into());
            }
            return Ok(applied.result.clone());
        }
        let current = self.state.accounts.get(account_fence).cloned().unwrap_or_default();
        if expected_revision != current.revision {
            return Err(format!(
                "contact-group revision conflict: expected {expected_revision}, current {}",
                current.revision
            ));
        }
        let next_revision = current
            .revision
            .checked_add(1)
            .ok_or_else(|| "contact-group revision exhausted".to_string())?;
        let projected = project_groups(
            normalized.clone(),
            known_peer_ids,
            pinned_peer_ids,
            false,
        );
        let result = json!({
            "status":"completed",
            "result":{
                "accountFence":account_fence,
                "revision":next_revision,
                "groups":projected,
            }
        });
        let mut next = self.state.clone();
        next.accounts.insert(
            account_fence.to_string(),
            AccountState { revision: next_revision, groups: normalized.clone() },
        );
        next.operations.insert(
            operation_key.clone(),
            AppliedOperation {
                account_fence: account_fence.to_string(),
                expected_revision,
                groups: normalized,
                result: result.clone(),
            },
        );
        next.operation_order.retain(|candidate| candidate != &operation_key);
        next.operation_order.push(operation_key);
        while next.operation_order.len() > MAX_OPERATION_HISTORY {
            let oldest = next.operation_order.remove(0);
            next.operations.remove(&oldest);
        }
        persist(&self.path, &next).map_err(|error| error.to_string())?;
        self.state = next;
        Ok(result)
    }
}

fn normalize_loaded(state: &mut ContactGroupsFile) {
    state.accounts.retain(|fence, _| !fence.trim().is_empty());
    for account in state.accounts.values_mut() {
        account.groups = normalize_unchecked(&account.groups);
    }
    state.operation_order.retain(|key| state.operations.contains_key(key));
    state.operations.retain(|key, _| state.operation_order.contains(key));
}

fn normalize_input(
    groups: &[AndroidContactGroup],
    known_peer_ids: &BTreeSet<String>,
) -> Result<Vec<AndroidContactGroup>, String> {
    if groups.len() > MAX_GROUPS {
        return Err("contact-group count exceeds bounded maximum".into());
    }
    for group in groups {
        if group.name.len() > MAX_NAME_BYTES {
            return Err("contact-group name exceeds bounded maximum".into());
        }
        if group.peer_ids.len() > MAX_PEERS_PER_GROUP {
            return Err("contact-group peer count exceeds bounded maximum".into());
        }
    }
    Ok(normalize_unchecked(groups)
        .into_iter()
        .map(|mut group| {
            group.peer_ids.retain(|peer| known_peer_ids.contains(peer));
            group
        })
        .collect())
}

fn normalize_unchecked(groups: &[AndroidContactGroup]) -> Vec<AndroidContactGroup> {
    let mut seen_groups = BTreeSet::new();
    let mut seen_peers = BTreeSet::new();
    let mut normalized = Vec::new();
    for raw in groups {
        let id = raw.id.trim();
        if id.is_empty()
            || id == CONTACT_GROUP_PINNED_ID
            || id == CONTACT_GROUP_UNASSIGNED_ID
            || id.len() > MAX_ID_BYTES
            || id.chars().any(char::is_control)
            || !seen_groups.insert(id.to_string())
        {
            continue;
        }
        let peer_ids = raw
            .peer_ids
            .iter()
            .map(|peer| peer.trim())
            .filter(|peer| !peer.is_empty() && seen_peers.insert((*peer).to_string()))
            .map(str::to_string)
            .collect();
        normalized.push(AndroidContactGroup {
            id: id.to_string(),
            name: raw.name.clone(),
            peer_ids,
            is_collapsed: raw.is_collapsed,
        });
    }
    normalized
}

fn project_groups(
    editable: Vec<AndroidContactGroup>,
    known_peer_ids: &BTreeSet<String>,
    pinned_peer_ids: &BTreeSet<String>,
    search_active: bool,
) -> Vec<AndroidContactGroup> {
    let pinned = known_peer_ids
        .intersection(pinned_peer_ids)
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut claimed = pinned.clone();
    let mut projected = Vec::new();
    if !pinned.is_empty() {
        projected.push(AndroidContactGroup {
            id: CONTACT_GROUP_PINNED_ID.into(),
            name: "Pinned".into(),
            peer_ids: pinned.iter().cloned().collect(),
            is_collapsed: false,
        });
    }
    for mut group in editable {
        group.peer_ids.retain(|peer| known_peer_ids.contains(peer) && claimed.insert(peer.clone()));
        if search_active && group.peer_ids.is_empty() {
            continue;
        }
        if search_active {
            group.is_collapsed = false;
        }
        projected.push(group);
    }
    let unassigned = known_peer_ids
        .iter()
        .filter(|peer| !claimed.contains(*peer))
        .cloned()
        .collect::<Vec<_>>();
    if !unassigned.is_empty() {
        projected.push(AndroidContactGroup {
            id: CONTACT_GROUP_UNASSIGNED_ID.into(),
            name: "Unassigned".into(),
            peer_ids: unassigned,
            is_collapsed: false,
        });
    }
    projected
}

fn validate_identity(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > MAX_ID_BYTES || value.chars().any(char::is_control) {
        return Err(format!("{label} identity is invalid"));
    }
    Ok(())
}

fn persist(path: &Path, state: &ContactGroupsFile) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

fn corrupt_path(path: &Path) -> PathBuf {
    let mut candidate = path.to_path_buf();
    candidate.set_extension("json.corrupt");
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peers(values: &[&str]) -> BTreeSet<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn group(id: &str, name: &str, peer_ids: &[&str], collapsed: bool) -> AndroidContactGroup {
        AndroidContactGroup {
            id: id.into(),
            name: name.into(),
            peer_ids: peer_ids.iter().map(|value| (*value).to_string()).collect(),
            is_collapsed: collapsed,
        }
    }

    #[test]
    fn projects_pinned_named_and_unassigned_in_stable_order() {
        let editable = vec![
            group("ops", "Ops", &["peer:b"], false),
            group("qa", "QA", &["peer:c"], true),
        ];
        let projected = project_groups(
            editable,
            &peers(&["peer:a", "peer:b", "peer:c", "peer:d"]),
            &peers(&["peer:a"]),
            false,
        );
        assert_eq!(
            projected.iter().map(|group| group.id.as_str()).collect::<Vec<_>>(),
            vec![CONTACT_GROUP_PINNED_ID, "ops", "qa", CONTACT_GROUP_UNASSIGNED_ID],
        );
        assert_eq!(projected[0].peer_ids, vec!["peer:a"]);
        assert_eq!(projected[3].peer_ids, vec!["peer:d"]);
        assert!(projected[2].is_collapsed);
    }

    #[test]
    fn normalization_is_exclusive_and_rejects_reserved_sections() {
        let normalized = normalize_input(
            &[
                group("ops", "Ops", &["peer:b", "peer:b"], false),
                group("qa", "QA", &["peer:b", "peer:c"], true),
                group(CONTACT_GROUP_PINNED_ID, "Fake", &["peer:d"], false),
            ],
            &peers(&["peer:b", "peer:c", "peer:d"]),
        )
        .unwrap();
        assert_eq!(normalized.len(), 2);
        assert_eq!(normalized[0].peer_ids, vec!["peer:b"]);
        assert_eq!(normalized[1].peer_ids, vec!["peer:c"]);
    }

    #[test]
    fn search_expands_matching_collapsed_groups_and_omits_empty_named_groups() {
        let projected = project_groups(
            vec![
                group("ops", "Ops", &["peer:b"], false),
                group("qa", "QA", &["peer:c"], true),
            ],
            &peers(&["peer:c"]),
            &BTreeSet::new(),
            true,
        );
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].id, "qa");
        assert!(!projected[0].is_collapsed);
    }

    #[test]
    fn account_scope_revision_replay_and_process_reopen_are_durable() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("contact-groups.json");
        let known = peers(&["peer:a", "peer:b"]);
        let desired = vec![group("ops", "Ops", &["peer:a"], true)];
        let first = {
            let mut store = AndroidContactGroups::open(&path).unwrap();
            store.set("acct-a", "op-1", 0, &desired, &known, &BTreeSet::new()).unwrap()
        };
        let mut reopened = AndroidContactGroups::open(&path).unwrap();
        assert_eq!(reopened.set("acct-a", "op-1", 0, &desired, &known, &BTreeSet::new()).unwrap(), first);
        let snapshot = reopened.get_snapshot("acct-a", &known, &BTreeSet::new(), false);
        assert_eq!(snapshot.revision, 1);
        assert_eq!(snapshot.groups[0].id, "ops");
        assert_eq!(snapshot.groups[1].id, CONTACT_GROUP_UNASSIGNED_ID);
        assert_eq!(reopened.get_snapshot("acct-b", &known, &BTreeSet::new(), false).revision, 0);
        assert!(reopened.set("acct-a", "op-2", 0, &desired, &known, &BTreeSet::new()).is_err());
    }
}
