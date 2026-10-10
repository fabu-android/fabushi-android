use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const ROSTER_SCHEMA_VERSION: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidAgentRecord {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_group: bool,
    #[serde(default)]
    pub member_ids: Vec<String>,
    #[serde(default)]
    pub is_hidden_from_sidebar: bool,
    #[serde(default)]
    pub has_unread: bool,
    #[serde(default)]
    pub is_pinned: bool,
    pub updated_at: u64,
}

impl AndroidAgentRecord {
    pub fn as_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| json!({
            "id": self.id,
            "name": self.name,
            "description": self.description,
            "isGroup": self.is_group,
            "memberIds": self.member_ids,
            "isHiddenFromSidebar": self.is_hidden_from_sidebar,
            "hasUnread": self.has_unread,
            "isPinned": self.is_pinned,
            "updatedAt": self.updated_at,
        }))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AndroidAgentRosterFile {
    schema_version: u8,
    next_id: u64,
    #[serde(default)]
    pinned_agent_ids: Vec<String>,
    #[serde(default)]
    agents: Vec<AndroidAgentRecord>,
    #[serde(default)]
    management_calls: std::collections::BTreeMap<String, Value>,
    #[serde(default)]
    management_call_order: Vec<String>,
}

impl Default for AndroidAgentRosterFile {
    fn default() -> Self {
        Self {
            schema_version: ROSTER_SCHEMA_VERSION,
            next_id: 0,
            pinned_agent_ids: Vec::new(),
            agents: Vec::new(),
            management_calls: std::collections::BTreeMap::new(),
            management_call_order: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AndroidAgentRoster {
    path: PathBuf,
    state: AndroidAgentRosterFile,
}

impl AndroidAgentRoster {
    pub fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        if !path.exists() {
            return Ok(Self {
                path,
                state: AndroidAgentRosterFile::default(),
            });
        }

        let raw = fs::read_to_string(&path)?;
        match serde_json::from_str::<AndroidAgentRosterFile>(&raw) {
            Ok(mut state) if state.schema_version == ROSTER_SCHEMA_VERSION => {
                normalize_state(&mut state);
                Ok(Self { path, state })
            }
            _ => {
                let quarantine = corrupt_path(&path);
                if let Some(parent) = quarantine.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(&path, &quarantine)?;
                Ok(Self {
                    path,
                    state: AndroidAgentRosterFile::default(),
                })
            }
        }
    }

    pub fn list(&self) -> Vec<AndroidAgentRecord> {
        let mut agents = self.state.agents.clone();
        agents.sort_by(|left, right| right.updated_at.cmp(&left.updated_at).then_with(|| left.id.cmp(&right.id)));
        agents
    }

    pub fn count(&self) -> usize {
        self.state.agents.len()
    }

    pub fn get(&self, id: &str) -> Option<AndroidAgentRecord> {
        self.state.agents.iter().find(|agent| agent.id == id).cloned()
    }

    pub fn create(&mut self, name: &str, description: &str) -> io::Result<AndroidAgentRecord> {
        let name = normalize_name(name)?;
        let description = normalize_description(description);
        let mut next = self.state.clone();
        next.next_id = next.next_id.saturating_add(1);
        let record = AndroidAgentRecord {
            id: format!("agent-{:08}", next.next_id),
            name,
            description,
            is_group: false,
            member_ids: Vec::new(),
            is_hidden_from_sidebar: false,
            has_unread: false,
            is_pinned: false,
            updated_at: now_ms(),
        };
        next.agents.push(record.clone());
        self.commit(next)?;
        Ok(record)
    }

    pub fn create_group(
        &mut self,
        name: &str,
        description: &str,
        member_ids: &[String],
    ) -> io::Result<AndroidAgentRecord> {
        let name = normalize_name(name)?;
        let description = normalize_description(description);
        let members = validate_group_members(&self.state, None, member_ids)?;
        let mut next = self.state.clone();
        next.next_id = next.next_id.saturating_add(1);
        let record = AndroidAgentRecord {
            id: format!("group-{:08}", next.next_id),
            name,
            description,
            is_group: true,
            member_ids: members,
            is_hidden_from_sidebar: false,
            has_unread: false,
            is_pinned: false,
            updated_at: now_ms(),
        };
        next.agents.push(record.clone());
        self.commit(next)?;
        Ok(record)
    }

    pub fn set_group_members(
        &mut self,
        id: &str,
        member_ids: &[String],
    ) -> io::Result<AndroidAgentRecord> {
        let id = id.trim();
        let current = self
            .state
            .agents
            .iter()
            .find(|agent| agent.id == id)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "agent group not found"))?;
        if !current.is_group {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "setGroupMembers target must be a group"));
        }
        let members = validate_group_members(&self.state, Some(id), member_ids)?;
        self.mutate_agent(id, |agent| agent.member_ids = members)
    }

    pub fn create_for_tool_call(
        &mut self,
        account_fence: &str,
        sender_agent_id: &str,
        tool_call_id: &str,
        name: &str,
        description: &str,
    ) -> Result<AndroidAgentRecord, String> {
        let key = management_call_key(account_fence, sender_agent_id, tool_call_id)?;
        let normalized_name = normalize_name(name).map_err(|error| error.to_string())?;
        let normalized_description = normalize_description(description);
        let args = json!({
            "tool":"CreateAgent",
            "name":normalized_name,
            "description":normalized_description
        });
        if let Some(call) = self.state.management_calls.get(&key) {
            if call.get("args") != Some(&args) {
                return Err("CreateAgent tool_call_id was reused with mismatched arguments".into());
            }
            return call
                .get("result")
                .cloned()
                .ok_or_else(|| "CreateAgent durable replay result is missing".to_string())
                .and_then(|value| serde_json::from_value(value)
                    .map_err(|error| format!("CreateAgent durable replay result is invalid: {error}")));
        }

        let mut next = self.state.clone();
        next.next_id = next.next_id.saturating_add(1);
        let created = AndroidAgentRecord {
            id: format!("agent-{:08}", next.next_id),
            name: normalized_name,
            description: normalized_description,
            is_group: false,
            member_ids: Vec::new(),
            is_hidden_from_sidebar: false,
            has_unread: false,
            is_pinned: false,
            updated_at: now_ms(),
        };
        next.agents.push(created.clone());
        record_management_call(
            &mut next,
            key,
            json!({"args":args,"result":created.as_json()}),
        );
        self.commit(next).map_err(|error| error.to_string())?;
        Ok(created)
    }

    pub fn update_for_tool_call(
        &mut self,
        account_fence: &str,
        sender_agent_id: &str,
        tool_call_id: &str,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<Option<AndroidAgentRecord>, String> {
        let key = management_call_key(account_fence, sender_agent_id, tool_call_id)?;
        let id = id.trim();
        let args = json!({
            "tool":"UpdateAgent",
            "agentId":id,
            "name":name.map(str::trim),
            "description":description.map(str::trim)
        });
        if let Some(call) = self.state.management_calls.get(&key) {
            if call.get("args") != Some(&args) {
                return Err("UpdateAgent tool_call_id was reused with mismatched arguments".into());
            }
            return match call.get("result") {
                Some(Value::Null) => Ok(None),
                Some(value) => serde_json::from_value(value.clone())
                    .map(Some)
                    .map_err(|error| format!("UpdateAgent durable replay result is invalid: {error}")),
                None => Err("UpdateAgent durable replay result is missing".into()),
            };
        }

        let mut next = self.state.clone();
        let Some(index) = next.agents.iter().position(|agent| agent.id == id) else {
            record_management_call(&mut next, key, json!({"args":args,"result":Value::Null}));
            self.commit(next).map_err(|error| error.to_string())?;
            return Ok(None);
        };
        let current = next.agents[index].clone();
        let next_name = match name {
            Some(value) => normalize_name(value).map_err(|error| error.to_string())?,
            None => current.name,
        };
        let next_description = description
            .map(normalize_description)
            .unwrap_or(current.description);
        next.agents[index].name = next_name;
        next.agents[index].description = next_description;
        next.agents[index].updated_at = now_ms();
        let updated = next.agents[index].clone();
        record_management_call(
            &mut next,
            key,
            json!({"args":args,"result":updated.as_json()}),
        );
        self.commit(next).map_err(|error| error.to_string())?;
        Ok(Some(updated))
    }

    /// Apply a presentation-originated roster mutation with durable operation identity.
    ///
    /// The mutation and its result/rejection are committed in the same canonical roster file.
    /// Replaying the same account-fenced operation id therefore cannot repeat side effects after
    /// Coordinator/Host process death. Reusing an operation id with different arguments is rejected.
    pub fn apply_presentation_operation(
        &mut self,
        account_fence: &str,
        operation_id: &str,
        mutation: &Value,
    ) -> Result<Value, String> {
        let key = management_call_key(account_fence, "android-presentation", operation_id)?;
        let args = json!({
            "tool":"PresentationMutation",
            "mutation":mutation,
        });
        if let Some(call) = self.state.management_calls.get(&key) {
            if call.get("args") != Some(&args) {
                return Err(
                    "presentation operation id was reused with mismatched arguments".into(),
                );
            }
            return call
                .get("result")
                .cloned()
                .ok_or_else(|| "presentation durable replay result is missing".to_string());
        }

        let kind = mutation
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| "presentation mutation kind is required".to_string())?;
        let mut next = self.state.clone();
        let outcome: Result<Value, String> = (|| match kind {
            "update" => {
                let id = mutation
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "presentation update id is required".to_string())?;
                let index = next
                    .agents
                    .iter()
                    .position(|agent| agent.id == id)
                    .ok_or_else(|| "agent not found".to_string())?;
                let current = next.agents[index].clone();
                let name = mutation
                    .get("name")
                    .and_then(Value::as_str)
                    .map(normalize_name)
                    .transpose()
                    .map_err(|error| error.to_string())?
                    .unwrap_or(current.name);
                let description = mutation
                    .get("description")
                    .and_then(Value::as_str)
                    .map(normalize_description)
                    .unwrap_or(current.description);
                next.agents[index].name = name;
                next.agents[index].description = description;
                next.agents[index].updated_at = now_ms();
                Ok(next.agents[index].as_json())
            }
            "hidden" | "unread" => {
                let id = mutation
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| format!("presentation {kind} id is required"))?;
                let value = mutation
                    .get("value")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| format!("presentation {kind} value is required"))?;
                let agent = next
                    .agents
                    .iter_mut()
                    .find(|agent| agent.id == id)
                    .ok_or_else(|| "agent not found".to_string())?;
                if kind == "hidden" {
                    agent.is_hidden_from_sidebar = value;
                } else {
                    agent.has_unread = value;
                }
                agent.updated_at = now_ms();
                Ok(agent.as_json())
            }
            "duplicate" => {
                let id = mutation
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "presentation duplicate id is required".to_string())?;
                let source = next
                    .agents
                    .iter()
                    .find(|agent| agent.id == id)
                    .cloned()
                    .ok_or_else(|| "agent not found".to_string())?;
                next.next_id = next.next_id.saturating_add(1);
                let duplicate = AndroidAgentRecord {
                    id: format!("agent-{:08}", next.next_id),
                    name: duplicate_name(&source.name),
                    description: source.description,
                    is_group: source.is_group,
                    member_ids: source.member_ids,
                    is_hidden_from_sidebar: false,
                    has_unread: false,
                    is_pinned: false,
                    updated_at: now_ms(),
                };
                next.agents.push(duplicate.clone());
                Ok(json!({"agent":duplicate.as_json()}))
            }
            "delete" => {
                let ids = mutation
                    .get("ids")
                    .and_then(Value::as_array)
                    .ok_or_else(|| "presentation delete ids are required".to_string())?
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .collect::<BTreeSet<_>>();
                let before = next.agents.len();
                next.agents.retain(|agent| !ids.contains(&agent.id));
                next.pinned_agent_ids.retain(|id| !ids.contains(id));
                for agent in &mut next.agents {
                    if agent.is_group {
                        agent.member_ids.retain(|member_id| !ids.contains(member_id));
                    }
                }
                let deleted = if next.agents.len() == before {
                    Vec::new()
                } else {
                    ids.into_iter()
                        .filter(|id| !next.agents.iter().any(|agent| agent.id == *id))
                        .collect::<Vec<_>>()
                };
                Ok(json!({"deletedIds":deleted}))
            }
            "pinned" => {
                let ids = mutation
                    .get("ids")
                    .and_then(Value::as_array)
                    .ok_or_else(|| "presentation pinned ids are required".to_string())?
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                let existing = next
                    .agents
                    .iter()
                    .map(|agent| agent.id.as_str())
                    .collect::<BTreeSet<_>>();
                let mut seen = BTreeSet::new();
                let ordered = ids
                    .into_iter()
                    .filter(|id| existing.contains(id.as_str()) && seen.insert(id.clone()))
                    .collect::<Vec<_>>();
                let pinned = ordered.iter().cloned().collect::<BTreeSet<_>>();
                next.pinned_agent_ids = ordered.clone();
                for agent in &mut next.agents {
                    agent.is_pinned = pinned.contains(&agent.id);
                }
                Ok(json!({"ids":ordered}))
            }
            "group-members" => {
                let id = mutation
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "presentation group-members id is required".to_string())?;
                let member_ids = mutation
                    .get("memberIds")
                    .and_then(Value::as_array)
                    .ok_or_else(|| "presentation group-members memberIds are required".to_string())?
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .ok_or_else(|| "presentation group member ids must be strings".to_string())
                            .map(str::to_string)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let current = next
                    .agents
                    .iter()
                    .find(|agent| agent.id == id)
                    .cloned()
                    .ok_or_else(|| "agent group not found".to_string())?;
                if !current.is_group {
                    return Err("group-members target must be a group".into());
                }
                let members = validate_group_members(&next, Some(id), &member_ids)
                    .map_err(|error| error.to_string())?;
                let agent = next
                    .agents
                    .iter_mut()
                    .find(|agent| agent.id == id)
                    .ok_or_else(|| "agent group not found".to_string())?;
                agent.member_ids = members;
                agent.updated_at = now_ms();
                Ok(agent.as_json())
            }
            other => Err(format!("unsupported presentation roster mutation: {other}")),
        })();

        let durable_result = match outcome {
            Ok(result) => json!({"status":"completed","result":result}),
            Err(error) => json!({"status":"rejected","error":error}),
        };
        record_management_call(
            &mut next,
            key,
            json!({"args":args,"result":durable_result.clone()}),
        );
        self.commit(next).map_err(|error| error.to_string())?;
        Ok(durable_result)
    }

    pub fn update_profile(
        &mut self,
        id: &str,
        name: &str,
        description: &str,
    ) -> io::Result<AndroidAgentRecord> {
        let name = normalize_name(name)?;
        let description = normalize_description(description);
        self.mutate_agent(id, |agent| {
            agent.name = name;
            agent.description = description;
        })
    }

    pub fn set_hidden(&mut self, id: &str, is_hidden: bool) -> io::Result<AndroidAgentRecord> {
        self.mutate_agent(id, |agent| agent.is_hidden_from_sidebar = is_hidden)
    }

    pub fn set_unread(&mut self, id: &str, is_unread: bool) -> io::Result<AndroidAgentRecord> {
        self.mutate_agent(id, |agent| agent.has_unread = is_unread)
    }

    pub fn duplicate(&mut self, id: &str) -> io::Result<AndroidAgentRecord> {
        let source = self
            .state
            .agents
            .iter()
            .find(|agent| agent.id == id)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "agent not found"))?;
        let mut next = self.state.clone();
        next.next_id = next.next_id.saturating_add(1);
        let duplicate = AndroidAgentRecord {
            id: format!("agent-{:08}", next.next_id),
            name: duplicate_name(&source.name),
            description: source.description,
            is_group: source.is_group,
            member_ids: source.member_ids,
            is_hidden_from_sidebar: false,
            has_unread: false,
            is_pinned: false,
            updated_at: now_ms(),
        };
        next.agents.push(duplicate.clone());
        self.commit(next)?;
        Ok(duplicate)
    }

    pub fn delete(&mut self, ids: &[String]) -> io::Result<Vec<String>> {
        let targets = ids
            .iter()
            .filter(|id| !id.trim().is_empty())
            .cloned()
            .collect::<BTreeSet<_>>();
        if targets.is_empty() {
            return Ok(Vec::new());
        }
        let mut next = self.state.clone();
        let before = next.agents.len();
        next.agents.retain(|agent| !targets.contains(&agent.id));
        if next.agents.len() == before {
            return Ok(Vec::new());
        }
        next.pinned_agent_ids.retain(|id| !targets.contains(id));
        for agent in &mut next.agents {
            if agent.is_group {
                agent.member_ids.retain(|member_id| !targets.contains(member_id));
            }
        }
        let deleted = targets
            .into_iter()
            .filter(|id| !next.agents.iter().any(|agent| agent.id == *id))
            .collect::<Vec<_>>();
        self.commit(next)?;
        Ok(deleted)
    }

    pub fn set_pinned_agents(&mut self, ids: &[String]) -> io::Result<Vec<String>> {
        let existing = self
            .state
            .agents
            .iter()
            .map(|agent| agent.id.as_str())
            .collect::<BTreeSet<_>>();
        let mut seen = BTreeSet::new();
        let ordered = ids
            .iter()
            .filter(|id| existing.contains(id.as_str()) && seen.insert((*id).clone()))
            .cloned()
            .collect::<Vec<_>>();
        let pinned = ordered.iter().cloned().collect::<BTreeSet<_>>();

        let mut next = self.state.clone();
        next.pinned_agent_ids = ordered.clone();
        for agent in &mut next.agents {
            agent.is_pinned = pinned.contains(&agent.id);
        }
        self.commit(next)?;
        Ok(ordered)
    }

    pub fn pinned_agent_ids(&self) -> Vec<String> {
        self.state.pinned_agent_ids.clone()
    }

    fn mutate_agent(
        &mut self,
        id: &str,
        mutate: impl FnOnce(&mut AndroidAgentRecord),
    ) -> io::Result<AndroidAgentRecord> {
        let mut next = self.state.clone();
        let agent = next
            .agents
            .iter_mut()
            .find(|agent| agent.id == id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "agent not found"))?;
        mutate(agent);
        agent.updated_at = now_ms();
        let updated = agent.clone();
        self.commit(next)?;
        Ok(updated)
    }

    fn commit(&mut self, mut next: AndroidAgentRosterFile) -> io::Result<()> {
        normalize_state(&mut next);
        persist(&self.path, &next)?;
        self.state = next;
        Ok(())
    }
}

fn record_management_call(
    state: &mut AndroidAgentRosterFile,
    key: String,
    call: Value,
) {
    state.management_calls.insert(key.clone(), call);
    state.management_call_order.retain(|value| value != &key);
    state.management_call_order.push(key);
    while state.management_call_order.len() > 512 {
        let expired = state.management_call_order.remove(0);
        state.management_calls.remove(&expired);
    }
}

fn management_call_key(
    account_fence: &str,
    sender_agent_id: &str,
    tool_call_id: &str,
) -> Result<String, String> {
    let account_fence = account_fence.trim();
    let sender_agent_id = sender_agent_id.trim();
    let tool_call_id = tool_call_id.trim();
    if account_fence.is_empty() || sender_agent_id.is_empty() || tool_call_id.is_empty() {
        return Err("Agent management account, sender and tool-call identity are required".into());
    }
    Ok(format!(
        "agent-management:{}",
        crate::sha256::sha256_hex(
            format!("{account_fence}\n{sender_agent_id}\n{tool_call_id}").as_bytes()
        )
    ))
}

fn persist(path: &Path, state: &AndroidAgentRosterFile) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("json.tmp-{}", std::process::id()));
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)
}

fn normalize_state(state: &mut AndroidAgentRosterFile) {
    let existing = state
        .agents
        .iter()
        .map(|agent| agent.id.clone())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    state
        .pinned_agent_ids
        .retain(|id| existing.contains(id) && seen.insert(id.clone()));
    let pinned = state
        .pinned_agent_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let non_group_ids = state
        .agents
        .iter()
        .filter(|agent| !agent.is_group)
        .map(|agent| agent.id.clone())
        .collect::<BTreeSet<_>>();
    for agent in &mut state.agents {
        agent.is_pinned = pinned.contains(&agent.id);
        if agent.is_group {
            let mut member_seen = BTreeSet::new();
            agent.member_ids.retain(|member_id| {
                non_group_ids.contains(member_id) && member_seen.insert(member_id.clone())
            });
            agent.member_ids.truncate(6);
        } else {
            agent.member_ids.clear();
        }
    }
}

fn validate_group_members(
    state: &AndroidAgentRosterFile,
    group_id: Option<&str>,
    member_ids: &[String],
) -> io::Result<Vec<String>> {
    let existing_non_groups = state
        .agents
        .iter()
        .filter(|agent| !agent.is_group)
        .map(|agent| agent.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut members = Vec::new();
    for raw in member_ids {
        let member_id = raw.trim();
        if member_id.is_empty() || group_id.is_some_and(|id| id == member_id) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "group member id is invalid"));
        }
        if !existing_non_groups.contains(member_id) {
            return Err(io::Error::new(io::ErrorKind::NotFound, format!("group member {member_id} not found or is another group")));
        }
        if seen.insert(member_id.to_string()) {
            members.push(member_id.to_string());
        }
    }
    if members.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "agent group requires at least one member"));
    }
    if members.len() > 6 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "agent group supports at most 6 members"));
    }
    Ok(members)
}

fn normalize_name(value: &str) -> io::Result<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "agent name is required"));
    }
    Ok(normalized.chars().take(72).collect())
}

fn normalize_description(value: &str) -> String {
    value.trim().chars().take(240).collect()
}

fn duplicate_name(value: &str) -> String {
    let base = value.trim();
    let candidate = if base.is_empty() {
        "New chat copy".to_string()
    } else {
        format!("{base} copy")
    };
    candidate.chars().take(72).collect()
}

fn corrupt_path(path: &Path) -> PathBuf {
    let stamp = now_ms();
    PathBuf::from(format!("{}.corrupt-{stamp}", path.display()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-agent-roster-{name}-{}-{}.json",
            std::process::id(),
            now_ms()
        ))
    }

    #[test]
    fn mutations_persist_one_canonical_roster() {
        let path = temp_path("mutations");
        let mut roster = AndroidAgentRoster::open(&path).unwrap();
        let created = roster.create("  Test   Agent  ", " desc ").unwrap();
        assert_eq!(created.name, "Test Agent");
        let renamed = roster
            .update_profile(&created.id, "Renamed", "updated")
            .unwrap();
        assert_eq!(renamed.name, "Renamed");
        roster.set_hidden(&created.id, true).unwrap();
        roster.set_unread(&created.id, true).unwrap();
        roster
            .set_pinned_agents(std::slice::from_ref(&created.id))
            .unwrap();

        let reopened = AndroidAgentRoster::open(&path).unwrap();
        let row = reopened.list().into_iter().next().unwrap();
        assert_eq!(row.name, "Renamed");
        assert!(row.is_hidden_from_sidebar);
        assert!(row.has_unread);
        assert!(row.is_pinned);
        assert_eq!(reopened.pinned_agent_ids(), vec![created.id.clone()]);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn duplicate_and_delete_do_not_create_a_second_truth() {
        let path = temp_path("duplicate");
        let mut roster = AndroidAgentRoster::open(&path).unwrap();
        let first = roster.create("Agent", "description").unwrap();
        let duplicate = roster.duplicate(&first.id).unwrap();
        assert_ne!(first.id, duplicate.id);
        assert_eq!(duplicate.name, "Agent copy");
        assert_eq!(roster.count(), 2);
        assert_eq!(roster.delete(std::slice::from_ref(&first.id)).unwrap(), vec![first.id]);
        assert_eq!(roster.count(), 1);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn group_members_are_canonical_durable_and_fenced_to_non_group_agents() {
        let path = temp_path("group-members");
        let mut roster = AndroidAgentRoster::open(&path).unwrap();
        let first = roster.create("First", "").unwrap();
        let second = roster.create("Second", "").unwrap();
        let group = roster
            .create_group("Team", "shared", &[first.id.clone(), second.id.clone(), first.id.clone()])
            .unwrap();
        assert!(group.is_group);
        assert_eq!(group.member_ids, vec![first.id.clone(), second.id.clone()]);

        let updated = roster
            .set_group_members(&group.id, std::slice::from_ref(&second.id))
            .unwrap();
        assert_eq!(updated.member_ids, vec![second.id.clone()]);
        assert!(roster
            .set_group_members(&group.id, std::slice::from_ref(&group.id))
            .is_err());

        let reopened = AndroidAgentRoster::open(&path).unwrap();
        assert_eq!(
            reopened.get(&group.id).unwrap().member_ids,
            vec![second.id.clone()]
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn deleting_an_agent_removes_it_from_group_membership_without_second_truth() {
        let path = temp_path("group-delete");
        let mut roster = AndroidAgentRoster::open(&path).unwrap();
        let first = roster.create("First", "").unwrap();
        let second = roster.create("Second", "").unwrap();
        let group = roster
            .create_group("Team", "", &[first.id.clone(), second.id.clone()])
            .unwrap();
        roster.delete(std::slice::from_ref(&first.id)).unwrap();
        assert_eq!(roster.get(&group.id).unwrap().member_ids, vec![second.id]);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn presentation_duplicate_replays_once_after_reopen_and_rejects_argument_reuse() {
        let path = temp_path("presentation-replay");
        let first_id = {
            let mut roster = AndroidAgentRoster::open(&path).unwrap();
            roster.create("Agent", "description").unwrap().id
        };
        let mutation = json!({"kind":"duplicate","id":first_id});
        let first_result = {
            let mut roster = AndroidAgentRoster::open(&path).unwrap();
            roster
                .apply_presentation_operation("session:account-a", "operation-1", &mutation)
                .unwrap()
        };
        assert_eq!(first_result["status"], "completed");
        let duplicate_id = first_result["result"]["agent"]["id"]
            .as_str()
            .unwrap()
            .to_string();

        let replay = {
            let mut reopened = AndroidAgentRoster::open(&path).unwrap();
            let replay = reopened
                .apply_presentation_operation("session:account-a", "operation-1", &mutation)
                .unwrap();
            assert_eq!(reopened.count(), 2, "durable replay must not duplicate twice");
            replay
        };
        assert_eq!(replay["result"]["agent"]["id"], duplicate_id);

        let mut reopened = AndroidAgentRoster::open(&path).unwrap();
        let mismatch = reopened.apply_presentation_operation(
            "session:account-a",
            "operation-1",
            &json!({"kind":"delete","ids":[first_id]}),
        );
        assert!(
            mismatch
                .unwrap_err()
                .contains("reused with mismatched arguments")
        );
        assert_eq!(reopened.count(), 2);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn presentation_rejection_is_durable_and_side_effect_free() {
        let path = temp_path("presentation-rejection");
        let mutation = json!({"kind":"duplicate","id":"missing-agent"});
        let first = {
            let mut roster = AndroidAgentRoster::open(&path).unwrap();
            roster
                .apply_presentation_operation("session:account-a", "operation-rejected", &mutation)
                .unwrap()
        };
        assert_eq!(first["status"], "rejected");
        assert_eq!(first["error"], "agent not found");

        let mut reopened = AndroidAgentRoster::open(&path).unwrap();
        let replay = reopened
            .apply_presentation_operation("session:account-a", "operation-rejected", &mutation)
            .unwrap();
        assert_eq!(replay, first);
        assert_eq!(reopened.count(), 0);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn corrupt_roster_is_quarantined_before_reset() {
        let path = temp_path("corrupt");
        fs::write(&path, "{not-json").unwrap();
        let roster = AndroidAgentRoster::open(&path).unwrap();
        assert_eq!(roster.count(), 0);
        let parent = path.parent().unwrap();
        let prefix = format!("{}.", path.file_name().unwrap().to_string_lossy());
        let quarantined = fs::read_dir(parent)
            .unwrap()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with(&prefix)
                && entry.file_name().to_string_lossy().contains(".corrupt-"));
        assert!(quarantined);
        let _ = fs::remove_file(path);
    }
}
