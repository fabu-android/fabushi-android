use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io,
    path::{Path, PathBuf},
};

const SIDEBAR_SECTIONS_SCHEMA_VERSION: u8 = 2;
pub const SIDEBAR_SYNTHETIC_SECTION_ID: &str = "__agents__";
const MAX_SECTION_ID_BYTES: usize = 256;
const MAX_SECTION_NAME_BYTES: usize = 512;
const MAX_SECTIONS: usize = 1_024;
const MAX_AGENT_IDS_PER_SECTION: usize = 16_384;
const MAX_OPERATION_HISTORY: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidSidebarSection {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub agent_ids: Vec<String>,
    #[serde(default)]
    pub is_collapsed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidSidebarSectionsSnapshot {
    pub account_fence: String,
    pub revision: u64,
    pub sections: Vec<AndroidSidebarSection>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AndroidSidebarAccountState {
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    sections: Vec<AndroidSidebarSection>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppliedSidebarSectionsOperation {
    account_fence: String,
    expected_revision: u64,
    base_sections: Vec<AndroidSidebarSection>,
    sections: Vec<AndroidSidebarSection>,
    result: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AndroidSidebarSectionsFile {
    schema_version: u8,
    #[serde(default)]
    accounts: BTreeMap<String, AndroidSidebarAccountState>,
    #[serde(default)]
    operations: BTreeMap<String, AppliedSidebarSectionsOperation>,
    #[serde(default)]
    operation_order: Vec<String>,
}

impl Default for AndroidSidebarSectionsFile {
    fn default() -> Self {
        Self {
            schema_version: SIDEBAR_SECTIONS_SCHEMA_VERSION,
            accounts: BTreeMap::new(),
            operations: BTreeMap::new(),
            operation_order: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacySidebarSectionsFileV1 {
    schema_version: u8,
    #[serde(default)]
    accounts: BTreeMap<String, Vec<AndroidSidebarSection>>,
}

pub struct AndroidSidebarSections {
    path: PathBuf,
    state: AndroidSidebarSectionsFile,
}

impl AndroidSidebarSections {
    pub fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        if !path.exists() {
            return Ok(Self { path, state: AndroidSidebarSectionsFile::default() });
        }
        let raw = fs::read_to_string(&path)?;
        if let Ok(mut state) = serde_json::from_str::<AndroidSidebarSectionsFile>(&raw) {
            if state.schema_version == SIDEBAR_SECTIONS_SCHEMA_VERSION {
                normalize_loaded_state(&mut state);
                return Ok(Self { path, state });
            }
        }
        if let Ok(legacy) = serde_json::from_str::<LegacySidebarSectionsFileV1>(&raw) {
            if legacy.schema_version == 1 {
                let mut state = AndroidSidebarSectionsFile::default();
                state.accounts = legacy
                    .accounts
                    .into_iter()
                    .map(|(account, sections)| {
                        (account, AndroidSidebarAccountState { revision: 0, sections })
                    })
                    .collect();
                normalize_loaded_state(&mut state);
                persist_state(&path, &state)?;
                return Ok(Self { path, state });
            }
        }
        let quarantine = corrupt_path(&path);
        if let Some(parent) = quarantine.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&path, &quarantine)?;
        Ok(Self { path, state: AndroidSidebarSectionsFile::default() })
    }

    pub fn get_snapshot(
        &self,
        account_fence: &str,
        known_agent_ids: &BTreeSet<String>,
    ) -> AndroidSidebarSectionsSnapshot {
        let account = self.state.accounts.get(account_fence).cloned().unwrap_or_default();
        AndroidSidebarSectionsSnapshot {
            account_fence: account_fence.to_string(),
            revision: account.revision,
            sections: project_sections(account.sections, known_agent_ids),
        }
    }

    pub fn set(
        &mut self,
        account_fence: &str,
        operation_id: &str,
        expected_revision: u64,
        base_sections: &[AndroidSidebarSection],
        sections: &[AndroidSidebarSection],
        known_agent_ids: &BTreeSet<String>,
    ) -> Result<Value, String> {
        validate_identity("account fence", account_fence)?;
        validate_identity("operation", operation_id)?;

        let normalized_base = normalize_input_sections(base_sections, known_agent_ids)?;
        let normalized_desired = normalize_input_sections(sections, known_agent_ids)?;
        let operation_key = format!("{account_fence}\n{operation_id}");
        if let Some(applied) = self.state.operations.get(&operation_key) {
            if applied.account_fence != account_fence
                || applied.expected_revision != expected_revision
                || applied.base_sections != normalized_base
                || applied.sections != normalized_desired
            {
                return Err("sidebar section operation identity was reused with different input".into());
            }
            return Ok(applied.result.clone());
        }

        let current = self.state.accounts.get(account_fence).cloned().unwrap_or_default();
        let settled = if expected_revision == current.revision {
            normalized_desired.clone()
        } else {
            merge_sections(&normalized_base, &current.sections, &normalized_desired).map_err(|detail| {
                format!(
                    "sidebar section revision conflict: expected {expected_revision}, current {}: {detail}",
                    current.revision
                )
            })?
        };
        let next_revision = current
            .revision
            .checked_add(1)
            .ok_or_else(|| "sidebar section revision exhausted".to_string())?;
        let projected = project_sections(settled.clone(), known_agent_ids);
        let result = json!({
            "status":"completed",
            "result":{"accountFence":account_fence,"revision":next_revision,"sections":projected},
        });

        let mut next = self.state.clone();
        next.accounts.insert(
            account_fence.to_string(),
            AndroidSidebarAccountState { revision: next_revision, sections: settled },
        );
        next.operations.insert(
            operation_key.clone(),
            AppliedSidebarSectionsOperation {
                account_fence: account_fence.to_string(),
                expected_revision,
                base_sections: normalized_base,
                sections: normalized_desired,
                result: result.clone(),
            },
        );
        next.operation_order.retain(|candidate| candidate != &operation_key);
        next.operation_order.push(operation_key);
        while next.operation_order.len() > MAX_OPERATION_HISTORY {
            let oldest = next.operation_order.remove(0);
            next.operations.remove(&oldest);
        }
        self.commit(next).map_err(|error| error.to_string())?;
        Ok(result)
    }

    fn commit(&mut self, next: AndroidSidebarSectionsFile) -> io::Result<()> {
        persist_state(&self.path, &next)?;
        self.state = next;
        Ok(())
    }
}

fn persist_state(path: &Path, state: &AndroidSidebarSectionsFile) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let encoded = serde_json::to_vec_pretty(state)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, encoded)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

fn normalize_loaded_state(state: &mut AndroidSidebarSectionsFile) {
    state.accounts.retain(|account_fence, _| !account_fence.trim().is_empty());
    for account in state.accounts.values_mut() {
        normalize_loaded_sections(&mut account.sections);
    }
    state.operation_order.retain(|key| state.operations.contains_key(key));
    state.operations.retain(|key, _| state.operation_order.contains(key));
}

fn normalize_loaded_sections(sections: &mut Vec<AndroidSidebarSection>) {
    let mut seen_sections = BTreeSet::new();
    let mut seen_agents = BTreeSet::new();
    sections.retain_mut(|section| {
        section.id = section.id.trim().to_string();
        if section.id.is_empty()
            || section.id == SIDEBAR_SYNTHETIC_SECTION_ID
            || !seen_sections.insert(section.id.clone())
        {
            return false;
        }
        section.agent_ids.retain(|agent_id| {
            !agent_id.is_empty() && seen_agents.insert(agent_id.clone())
        });
        true
    });
}

fn normalize_input_sections(
    sections: &[AndroidSidebarSection],
    known_agent_ids: &BTreeSet<String>,
) -> Result<Vec<AndroidSidebarSection>, String> {
    if sections.len() > MAX_SECTIONS {
        return Err("sidebar section count exceeds bounded maximum".into());
    }
    let mut seen_sections = BTreeSet::new();
    let mut seen_agents = BTreeSet::new();
    let mut normalized = Vec::new();
    for section in sections {
        let id = section.id.trim();
        if id == SIDEBAR_SYNTHETIC_SECTION_ID {
            continue;
        }
        validate_section_id(id)?;
        if !seen_sections.insert(id.to_string()) {
            return Err("sidebar section ids must be unique".into());
        }
        if section.name.len() > MAX_SECTION_NAME_BYTES {
            return Err("sidebar section name exceeds bounded maximum".into());
        }
        if section.agent_ids.len() > MAX_AGENT_IDS_PER_SECTION {
            return Err("sidebar section agent count exceeds bounded maximum".into());
        }
        let mut agent_ids = Vec::new();
        for agent_id in &section.agent_ids {
            let agent_id = agent_id.trim();
            if agent_id.is_empty() || !known_agent_ids.contains(agent_id) {
                continue;
            }
            if seen_agents.insert(agent_id.to_string()) {
                agent_ids.push(agent_id.to_string());
            }
        }
        normalized.push(AndroidSidebarSection {
            id: id.to_string(),
            name: section.name.clone(),
            agent_ids,
            is_collapsed: section.is_collapsed,
        });
    }
    Ok(normalized)
}

fn merge_scalar<T: Clone + PartialEq>(base: &T, current: &T, desired: &T) -> Result<T, ()> {
    if current == desired {
        Ok(current.clone())
    } else if current == base {
        Ok(desired.clone())
    } else if desired == base {
        Ok(current.clone())
    } else {
        Err(())
    }
}

fn merge_sections(
    base: &[AndroidSidebarSection],
    current: &[AndroidSidebarSection],
    desired: &[AndroidSidebarSection],
) -> Result<Vec<AndroidSidebarSection>, String> {
    let base_map = base.iter().map(|s| (s.id.as_str(), s)).collect::<BTreeMap<_, _>>();
    let current_map = current.iter().map(|s| (s.id.as_str(), s)).collect::<BTreeMap<_, _>>();
    let desired_map = desired.iter().map(|s| (s.id.as_str(), s)).collect::<BTreeMap<_, _>>();
    let ids = base_map.keys().chain(current_map.keys()).chain(desired_map.keys()).copied().collect::<BTreeSet<_>>();
    let mut merged = BTreeMap::<String, AndroidSidebarSection>::new();

    for id in ids {
        let b = base_map.get(id).copied();
        let c = current_map.get(id).copied();
        let d = desired_map.get(id).copied();
        let value = match (b, c, d) {
            // `id` is drawn from the union of base/current/desired keys, so this arm is
            // unreachable by construction. Keep it explicit so the merge stays exhaustive
            // without turning a malformed future caller into a panic.
            (None, None, None) => continue,
            (None, None, Some(value)) | (None, Some(value), None) => Some(value.clone()),
            (None, Some(left), Some(right)) if left == right => Some(left.clone()),
            (None, Some(_), Some(_)) => return Err(format!("concurrent section addition conflicts for {id}")),
            (Some(base), None, None) => { let _ = base; None },
            (Some(base), None, Some(desired)) if desired == base => None,
            (Some(base), Some(current), None) if current == base => None,
            (Some(_), None, Some(_)) | (Some(_), Some(_), None) => {
                return Err(format!("section delete conflicts with concurrent edit for {id}"));
            }
            (Some(base), Some(current), Some(desired)) => Some(AndroidSidebarSection {
                id: id.to_string(),
                name: merge_scalar(&base.name, &current.name, &desired.name)
                    .map_err(|_| format!("section name conflicts for {id}"))?,
                agent_ids: merge_scalar(&base.agent_ids, &current.agent_ids, &desired.agent_ids)
                    .map_err(|_| format!("section membership conflicts for {id}"))?,
                is_collapsed: merge_scalar(&base.is_collapsed, &current.is_collapsed, &desired.is_collapsed)
                    .map_err(|_| format!("section collapse conflicts for {id}"))?,
            }),
        };
        if let Some(value) = value {
            merged.insert(id.to_string(), value);
        }
    }

    let base_order = base.iter().map(|s| s.id.as_str()).collect::<Vec<_>>();
    let current_order = current.iter().map(|s| s.id.as_str()).collect::<Vec<_>>();
    let desired_order = desired.iter().map(|s| s.id.as_str()).collect::<Vec<_>>();
    let chosen_order = if current_order == desired_order {
        current_order
    } else if current_order == base_order {
        desired_order
    } else if desired_order == base_order {
        current_order
    } else {
        return Err("section order changed concurrently".into());
    };
    let mut out = Vec::new();
    for id in chosen_order {
        if let Some(section) = merged.remove(id) {
            out.push(section);
        }
    }
    out.extend(merged.into_values());
    Ok(out)
}

fn project_sections(
    editable: Vec<AndroidSidebarSection>,
    known_agent_ids: &BTreeSet<String>,
) -> Vec<AndroidSidebarSection> {
    if editable.is_empty() {
        return Vec::new();
    }
    let claimed = editable
        .iter()
        .flat_map(|section| section.agent_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    let unassigned = known_agent_ids
        .iter()
        .filter(|agent_id| !claimed.contains(*agent_id))
        .cloned()
        .collect::<Vec<_>>();
    let mut projected = editable;
    projected.push(AndroidSidebarSection {
        id: SIDEBAR_SYNTHETIC_SECTION_ID.into(),
        name: "Unassigned".into(),
        agent_ids: unassigned,
        is_collapsed: false,
    });
    projected
}

fn validate_identity(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.len() > MAX_SECTION_ID_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(format!("{label} identity is invalid"));
    }
    Ok(())
}

fn validate_section_id(value: &str) -> Result<(), String> {
    validate_identity("sidebar section", value)
}

fn corrupt_path(path: &Path) -> PathBuf {
    let mut candidate = path.to_path_buf();
    let extension = path.extension().and_then(|value| value.to_str()).unwrap_or("json");
    candidate.set_extension(format!("{extension}.corrupt"));
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| (*id).to_string()).collect()
    }

    fn section(id: &str, name: &str, ids: &[&str], collapsed: bool) -> AndroidSidebarSection {
        AndroidSidebarSection {
            id: id.into(),
            name: name.into(),
            agent_ids: ids.iter().map(|id| (*id).to_string()).collect(),
            is_collapsed: collapsed,
        }
    }

    #[test]
    fn sections_are_account_scoped_and_project_unassigned_agents() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("sidebar-sections.json");
        let mut store = AndroidSidebarSections::open(&path).unwrap();
        let result = store.set("acct-1", "op-1", 0, &[], &[section("section-a", "Team", &["agent-1"], true)], &known(&["agent-1", "agent-2"])).unwrap();
        assert_eq!(result["result"]["revision"], 1);
        assert_eq!(result["result"]["sections"][0]["isCollapsed"], true);
        assert_eq!(result["result"]["sections"][1]["id"], SIDEBAR_SYNTHETIC_SECTION_ID);
        let other = store.get_snapshot("acct-2", &known(&["agent-1"]));
        assert_eq!(other.account_fence, "acct-2");
        assert_eq!(other.revision, 0);
    }

    #[test]
    fn revision_and_collapsed_state_survive_process_reopen() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("sidebar-sections.json");
        {
            let mut store = AndroidSidebarSections::open(&path).unwrap();
            store.set("acct-1", "op-collapse", 0, &[], &[section("section-a", "Team", &["agent-1"], true)], &known(&["agent-1", "agent-2"])).unwrap();
        }
        let reopened = AndroidSidebarSections::open(&path).unwrap();
        let projected = reopened.get_snapshot("acct-1", &known(&["agent-1", "agent-2"]));
        assert_eq!(projected.revision, 1);
        assert!(projected.sections[0].is_collapsed);
        assert_eq!(projected.sections[1].id, SIDEBAR_SYNTHETIC_SECTION_ID);
    }

    #[test]
    fn operation_replay_is_idempotent_and_input_mismatch_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("sidebar-sections.json");
        let sections = vec![section("section-a", "Team", &["agent-1"], false)];
        let expected = known(&["agent-1"]);
        let first = {
            let mut store = AndroidSidebarSections::open(&path).unwrap();
            store.set("acct-1", "op-1", 0, &[], &sections, &expected).unwrap()
        };
        let mut reopened = AndroidSidebarSections::open(&path).unwrap();
        assert_eq!(reopened.set("acct-1", "op-1", 0, &[], &sections, &expected).unwrap(), first);
        let changed = vec![section("section-a", "Changed", &["agent-1"], false)];
        assert!(reopened.set("acct-1", "op-1", 0, &[], &changed, &expected).is_err());
    }

    #[test]
    fn stale_independent_edits_merge_but_same_field_conflict_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        let mut store = AndroidSidebarSections::open(root.path().join("sidebar-sections.json")).unwrap();
        let ids = known(&["agent-1"]);
        let base = vec![section("section-a", "Team", &["agent-1"], false)];
        store.set("acct-1", "seed", 0, &[], &base, &ids).unwrap();
        let collapsed = vec![section("section-a", "Team", &["agent-1"], true)];
        store.set("acct-1", "collapse", 1, &base, &collapsed, &ids).unwrap();
        let renamed = vec![section("section-a", "Renamed", &["agent-1"], false)];
        let merged = store.set("acct-1", "rename-stale", 1, &base, &renamed, &ids).unwrap();
        assert_eq!(merged["result"]["revision"], 3);
        assert_eq!(merged["result"]["sections"][0]["name"], "Renamed");
        assert_eq!(merged["result"]["sections"][0]["isCollapsed"], true);
        let conflicting = vec![section("section-a", "Other", &["agent-1"], false)];
        assert!(store.set("acct-1", "rename-conflict", 1, &base, &conflicting, &ids).unwrap_err().contains("name conflicts"));
    }

    #[test]
    fn legacy_v1_migrates_without_losing_account_sections() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("sidebar-sections.json");
        fs::write(&path, serde_json::to_vec_pretty(&json!({
            "schemaVersion":1,
            "accounts":{"acct-1":[{"id":"section-a","name":"Legacy","agentIds":["agent-1"],"isCollapsed":true}]},
            "operations":{},"operationOrder":[]
        })).unwrap()).unwrap();
        let reopened = AndroidSidebarSections::open(&path).unwrap();
        let snapshot = reopened.get_snapshot("acct-1", &known(&["agent-1"]));
        assert_eq!(snapshot.revision, 0);
        assert_eq!(snapshot.sections[0].name, "Legacy");
        let persisted: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(persisted["schemaVersion"], 2);
    }

    #[test]
    fn synthetic_section_is_never_persisted_and_duplicate_membership_is_normalized() {
        let root = tempfile::tempdir().unwrap();
        let mut store = AndroidSidebarSections::open(root.path().join("sidebar-sections.json")).unwrap();
        let result = store.set("acct-1", "op-1", 0, &[], &[
            section("section-a", "A", &["agent-1"], false),
            section("section-b", "B", &["agent-1", "agent-2"], false),
            section(SIDEBAR_SYNTHETIC_SECTION_ID, "Unassigned", &["agent-3"], false),
        ], &known(&["agent-1", "agent-2", "agent-3"])).unwrap();
        assert_eq!(result["result"]["sections"][0]["agentIds"], json!(["agent-1"]));
        assert_eq!(result["result"]["sections"][1]["agentIds"], json!(["agent-2"]));
        assert_eq!(result["result"]["sections"][2]["agentIds"], json!(["agent-3"]));
    }
}
