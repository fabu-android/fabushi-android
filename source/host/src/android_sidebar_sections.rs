use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io,
    path::{Path, PathBuf},
};

const SIDEBAR_SECTIONS_SCHEMA_VERSION: u8 = 1;
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppliedSidebarSectionsOperation {
    account_fence: String,
    sections: Vec<AndroidSidebarSection>,
    result: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AndroidSidebarSectionsFile {
    schema_version: u8,
    #[serde(default)]
    accounts: BTreeMap<String, Vec<AndroidSidebarSection>>,
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
        match serde_json::from_str::<AndroidSidebarSectionsFile>(&raw) {
            Ok(mut state) if state.schema_version == SIDEBAR_SECTIONS_SCHEMA_VERSION => {
                normalize_loaded_state(&mut state);
                Ok(Self { path, state })
            }
            _ => {
                let quarantine = corrupt_path(&path);
                if let Some(parent) = quarantine.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(&path, &quarantine)?;
                Ok(Self { path, state: AndroidSidebarSectionsFile::default() })
            }
        }
    }

    pub fn get(&self, account_fence: &str, known_agent_ids: &BTreeSet<String>) -> Vec<AndroidSidebarSection> {
        let editable = self
            .state
            .accounts
            .get(account_fence)
            .cloned()
            .unwrap_or_default();
        project_sections(editable, known_agent_ids)
    }

    pub fn set(
        &mut self,
        account_fence: &str,
        operation_id: &str,
        sections: &[AndroidSidebarSection],
        known_agent_ids: &BTreeSet<String>,
    ) -> Result<Value, String> {
        validate_identity("account fence", account_fence)?;
        validate_identity("operation", operation_id)?;

        let normalized = normalize_input_sections(sections, known_agent_ids)?;
        let operation_key = format!("{account_fence}\n{operation_id}");
        if let Some(applied) = self.state.operations.get(&operation_key) {
            if applied.account_fence != account_fence || applied.sections != normalized {
                return Err("sidebar section operation identity was reused with different input".into());
            }
            return Ok(applied.result.clone());
        }

        let projected = project_sections(normalized.clone(), known_agent_ids);
        let result = json!({
            "status":"completed",
            "result":{"sections":projected},
        });

        let mut next = self.state.clone();
        next.accounts.insert(account_fence.to_string(), normalized.clone());
        next.operations.insert(
            operation_key.clone(),
            AppliedSidebarSectionsOperation {
                account_fence: account_fence.to_string(),
                sections: normalized,
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
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let encoded = serde_json::to_vec_pretty(&next)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, encoded)?;
        fs::rename(&temporary, &self.path)?;
        self.state = next;
        Ok(())
    }
}

fn normalize_loaded_state(state: &mut AndroidSidebarSectionsFile) {
    state
        .accounts
        .retain(|account_fence, _| !account_fence.trim().is_empty());
    for sections in state.accounts.values_mut() {
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
            section.is_collapsed = false;
            section.agent_ids.retain(|agent_id| {
                !agent_id.is_empty() && seen_agents.insert(agent_id.clone())
            });
            true
        });
    }
    state.operation_order.retain(|key| state.operations.contains_key(key));
    state.operations.retain(|key, _| state.operation_order.contains(key));
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
            is_collapsed: false,
        });
    }
    Ok(normalized)
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

    #[test]
    fn sections_are_account_scoped_and_project_unassigned_agents() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("sidebar-sections.json");
        let mut store = AndroidSidebarSections::open(&path).unwrap();
        let result = store
            .set(
                "acct-1",
                "op-1",
                &[AndroidSidebarSection {
                    id: "section-a".into(),
                    name: "Team".into(),
                    agent_ids: vec!["agent-1".into()],
                    is_collapsed: true,
                }],
                &known(&["agent-1", "agent-2"]),
            )
            .unwrap();
        assert_eq!(result["result"]["sections"][0]["isCollapsed"], false);
        assert_eq!(result["result"]["sections"][1]["id"], SIDEBAR_SYNTHETIC_SECTION_ID);
        assert_eq!(result["result"]["sections"][1]["agentIds"][0], "agent-2");
        assert!(store.get("acct-2", &known(&["agent-1"])).is_empty());
    }

    #[test]
    fn operation_replay_is_idempotent_and_input_mismatch_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("sidebar-sections.json");
        let sections = vec![AndroidSidebarSection {
            id: "section-a".into(),
            name: "Team".into(),
            agent_ids: vec!["agent-1".into()],
            is_collapsed: false,
        }];
        let expected = known(&["agent-1"]);
        let first = {
            let mut store = AndroidSidebarSections::open(&path).unwrap();
            store.set("acct-1", "op-1", &sections, &expected).unwrap()
        };
        let mut reopened = AndroidSidebarSections::open(&path).unwrap();
        assert_eq!(
            reopened.set("acct-1", "op-1", &sections, &expected).unwrap(),
            first
        );
        let changed = vec![AndroidSidebarSection {
            id: "section-a".into(),
            name: "Changed".into(),
            agent_ids: vec!["agent-1".into()],
            is_collapsed: false,
        }];
        assert!(reopened.set("acct-1", "op-1", &changed, &expected).is_err());
    }

    #[test]
    fn synthetic_section_is_never_persisted_and_duplicate_membership_is_normalized() {
        let root = tempfile::tempdir().unwrap();
        let mut store = AndroidSidebarSections::open(root.path().join("sidebar-sections.json")).unwrap();
        let result = store
            .set(
                "acct-1",
                "op-1",
                &[
                    AndroidSidebarSection {
                        id: "section-a".into(),
                        name: "A".into(),
                        agent_ids: vec!["agent-1".into()],
                        is_collapsed: false,
                    },
                    AndroidSidebarSection {
                        id: "section-b".into(),
                        name: "B".into(),
                        agent_ids: vec!["agent-1".into(), "agent-2".into()],
                        is_collapsed: false,
                    },
                    AndroidSidebarSection {
                        id: SIDEBAR_SYNTHETIC_SECTION_ID.into(),
                        name: "Unassigned".into(),
                        agent_ids: vec!["agent-3".into()],
                        is_collapsed: false,
                    },
                ],
                &known(&["agent-1", "agent-2", "agent-3"]),
            )
            .unwrap();
        assert_eq!(result["result"]["sections"][0]["agentIds"], json!(["agent-1"]));
        assert_eq!(result["result"]["sections"][1]["agentIds"], json!(["agent-2"]));
        assert_eq!(result["result"]["sections"][2]["agentIds"], json!(["agent-3"]));
    }
}
