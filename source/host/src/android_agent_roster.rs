use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    io::{self, Write},
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub notify_on_updates_enabled: bool,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_shape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_data_url: Option<String>,
    pub updated_at: u64,
}

impl AndroidAgentRecord {
    pub fn as_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| json!({
            "id": self.id,
            "name": self.name,
            "description": self.description,
            "title": self.title,
            "notifyOnUpdatesEnabled": self.notify_on_updates_enabled,
            "isGroup": self.is_group,
            "memberIds": self.member_ids,
            "isHiddenFromSidebar": self.is_hidden_from_sidebar,
            "hasUnread": self.has_unread,
            "isPinned": self.is_pinned,
            "avatarShape": self.avatar_shape,
            "avatarColor": self.avatar_color,
            "avatarDataUrl": self.avatar_data_url,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AndroidAgentRosterRecoveryOutcome {
    RecoveredLastKnownGood,
    ResetCorruptStore,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AndroidAgentRosterRecovery {
    pub outcome: AndroidAgentRosterRecoveryOutcome,
    pub quarantine_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct AndroidAgentRoster {
    path: PathBuf,
    state: AndroidAgentRosterFile,
}

impl AndroidAgentRoster {
    pub fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        Self::open_recovering(path).map(|(roster, _)| roster)
    }

    pub fn open_recovering(
        path: impl Into<PathBuf>,
    ) -> io::Result<(Self, Option<AndroidAgentRosterRecovery>)> {
        let path = path.into();
        if !path.exists() {
            return Ok((
                Self {
                    path,
                    state: AndroidAgentRosterFile::default(),
                },
                None,
            ));
        }

        let raw = fs::read_to_string(&path)?;
        match serde_json::from_str::<AndroidAgentRosterFile>(&raw) {
            Ok(mut state) if state.schema_version == ROSTER_SCHEMA_VERSION => {
                normalize_state(&mut state);
                Ok((Self { path, state }, None))
            }
            _ => {
                let quarantine = corrupt_path(&path);
                if let Some(parent) = quarantine.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(&path, &quarantine)?;

                let backup = recovery_backup_path(&path);
                if let Ok(raw_backup) = fs::read_to_string(&backup) {
                    if let Ok(mut recovered) =
                        serde_json::from_str::<AndroidAgentRosterFile>(&raw_backup)
                    {
                        if recovered.schema_version == ROSTER_SCHEMA_VERSION {
                            normalize_state(&mut recovered);
                            persist(&path, &recovered)?;
                            return Ok((
                                Self {
                                    path,
                                    state: recovered,
                                },
                                Some(AndroidAgentRosterRecovery {
                                    outcome:
                                        AndroidAgentRosterRecoveryOutcome::RecoveredLastKnownGood,
                                    quarantine_path: quarantine,
                                }),
                            ));
                        }
                    }
                }

                Ok((
                    Self {
                        path,
                        state: AndroidAgentRosterFile::default(),
                    },
                    Some(AndroidAgentRosterRecovery {
                        outcome: AndroidAgentRosterRecoveryOutcome::ResetCorruptStore,
                        quarantine_path: quarantine,
                    }),
                ))
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
        self.create_with_persona(name, description, None, None)
    }

    pub fn create_with_persona(
        &mut self,
        name: &str,
        description: &str,
        avatar_shape: Option<&Value>,
        avatar_color: Option<&Value>,
    ) -> io::Result<AndroidAgentRecord> {
        let name = normalize_name(name)?;
        let description = normalize_description(description);
        let avatar_shape = parse_avatar_shape(avatar_shape)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let avatar_color = parse_avatar_color(avatar_color)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let mut next = self.state.clone();
        next.next_id = next.next_id.saturating_add(1);
        let record = AndroidAgentRecord {
            id: format!("agent-{:08}", next.next_id),
            name,
            description,
            title: None,
            notify_on_updates_enabled: false,
            is_group: false,
            member_ids: Vec::new(),
            is_hidden_from_sidebar: false,
            has_unread: false,
            is_pinned: false,
            avatar_shape,
            avatar_color,
            avatar_data_url: None,
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
            title: None,
            notify_on_updates_enabled: false,
            is_group: true,
            member_ids: members,
            is_hidden_from_sidebar: false,
            has_unread: false,
            is_pinned: false,
            avatar_shape: None,
            avatar_color: None,
            avatar_data_url: None,
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
            title: None,
            notify_on_updates_enabled: false,
            is_group: false,
            member_ids: Vec::new(),
            is_hidden_from_sidebar: false,
            has_unread: false,
            is_pinned: false,
            avatar_shape: None,
            avatar_color: None,
            avatar_data_url: None,
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
                let title = if mutation.get("title").is_some() {
                    normalize_title(mutation.get("title").and_then(Value::as_str))
                } else {
                    current.title
                };
                let avatar_shape = if mutation.get("avatarShape").is_some() {
                    parse_avatar_shape(mutation.get("avatarShape"))?
                } else {
                    current.avatar_shape
                };
                let avatar_color = if mutation.get("avatarColor").is_some() {
                    parse_avatar_color(mutation.get("avatarColor"))?
                } else {
                    current.avatar_color
                };
                let avatar_data_url = if mutation.get("avatarDataUrl").is_some() {
                    parse_avatar_data_url(mutation.get("avatarDataUrl"))?
                } else {
                    current.avatar_data_url
                };
                next.agents[index].name = name;
                next.agents[index].description = description;
                next.agents[index].title = title;
                next.agents[index].avatar_shape = avatar_shape;
                next.agents[index].avatar_color = avatar_color;
                next.agents[index].avatar_data_url = avatar_data_url;
                next.agents[index].updated_at = now_ms();
                Ok(next.agents[index].as_json())
            }
            "hidden" | "unread" | "notifications" => {
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
                } else if kind == "unread" {
                    agent.has_unread = value;
                } else {
                    if agent.is_group {
                        return Err("Agent group notifications are not supported".into());
                    }
                    agent.notify_on_updates_enabled = value;
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
                    title: source.title,
                    notify_on_updates_enabled: source.notify_on_updates_enabled,
                    is_group: source.is_group,
                    member_ids: source.member_ids,
                    is_hidden_from_sidebar: false,
                    has_unread: false,
                    is_pinned: false,
                    avatar_shape: source.avatar_shape,
                    avatar_color: source.avatar_color,
                    avatar_data_url: source.avatar_data_url,
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
            title: source.title,
            notify_on_updates_enabled: source.notify_on_updates_enabled,
            is_group: source.is_group,
            member_ids: source.member_ids,
            is_hidden_from_sidebar: false,
            has_unread: false,
            is_pinned: false,
            avatar_shape: source.avatar_shape,
            avatar_color: source.avatar_color,
            avatar_data_url: source.avatar_data_url,
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

fn recovery_backup_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.recovery-backup", path.display()))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "roster path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        if let Ok(directory) = fs::File::open(parent) {
            directory.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn persist(path: &Path, state: &AndroidAgentRosterFile) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    // The backup is a recovery replica, never a second canonical owner. Commit the complete next
    // state there first so a later canonical-file corruption can salvage the latest settled roster.
    write_atomic(&recovery_backup_path(path), &bytes)?;
    write_atomic(path, &bytes)
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

fn normalize_title(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(120).collect())
}

fn parse_avatar_shape(value: Option<&Value>) -> Result<Option<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(raw)) => {
            let shape = raw.trim().to_ascii_lowercase();
            match shape.as_str() {
                "blob" | "pebble" | "squircle" | "tablet" | "wedge" | "hex" | "cloud"
                | "teardrop" | "circle" | "square" => Ok(Some(shape)),
                _ => Err("avatarShape must be a supported Desktop persona or legacy Android shape".into()),
            }
        }
        Some(_) => Err("avatarShape must be a string or null".into()),
    }
}

fn parse_avatar_color(value: Option<&Value>) -> Result<Option<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(raw)) => {
            let color = raw.trim();
            let named = color.to_ascii_lowercase();
            if matches!(
                named.as_str(),
                "brown" | "red" | "orange" | "yellow" | "green" | "cyan" | "blue"
                    | "violet" | "magenta" | "gray"
            ) {
                return Ok(Some(named));
            }
            let bytes = color.as_bytes();
            if bytes.len() != 7 || bytes[0] != b'#' || !bytes[1..].iter().all(u8::is_ascii_hexdigit) {
                return Err("avatarColor must be a supported Desktop persona color, #RRGGBB, or null".into());
            }
            Ok(Some(color.to_ascii_uppercase()))
        }
        Some(_) => Err("avatarColor must be a string or null".into()),
    }
}

fn sniff_avatar_mime_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 8 && bytes[..8] == [137, 80, 78, 71, 13, 10, 26, 10] {
        return Some("image/png");
    }
    if bytes.len() >= 3 && bytes[..3] == [255, 216, 255] {
        return Some("image/jpeg");
    }
    if bytes.len() >= 6 && (&bytes[..6] == b"GIF87a" || &bytes[..6] == b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]);
    let normalized = head
        .trim_start_matches('\u{feff}')
        .trim_start()
        .to_ascii_lowercase();
    if normalized.starts_with('<') && normalized.contains("<svg") {
        return Some("image/svg+xml");
    }
    None
}

fn parse_avatar_data_url(value: Option<&Value>) -> Result<Option<String>, String> {
    // Desktop source/host/src/agents/agent_avatar.rs caps avatar source files at 5 MiB and
    // accepts PNG/JPEG/WEBP/GIF/SVG. Keep the canonical Android projection at the same boundary.
    const MAX_SOURCE_BYTES: usize = 5 * 1024 * 1024;
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(raw)) => {
            let data_url = raw.trim();
            if data_url.is_empty() {
                return Ok(None);
            }
            let comma = data_url
                .find(',')
                .ok_or_else(|| "avatarDataUrl must be an image data URL".to_string())?;
            let header = data_url[..comma].to_ascii_lowercase();
            let declared_mime = match header.as_str() {
                "data:image/png;base64" => "image/png",
                "data:image/jpeg;base64" | "data:image/jpg;base64" => "image/jpeg",
                "data:image/webp;base64" => "image/webp",
                "data:image/gif;base64" => "image/gif",
                "data:image/svg+xml;base64" => "image/svg+xml",
                _ => {
                    return Err(
                        "avatarDataUrl must use a Desktop-supported base64 image MIME type".into(),
                    )
                }
            };
            let payload = &data_url[comma + 1..];
            if payload.is_empty()
                || payload.len() % 4 != 0
                || !payload
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
            {
                return Err("avatarDataUrl contains invalid base64 image data".into());
            }
            let padding = payload.as_bytes().iter().rev().take_while(|byte| **byte == b'=').count();
            let content_len = payload.len().saturating_sub(padding);
            if padding > 2 || payload.as_bytes()[..content_len].contains(&b'=') {
                return Err("avatarDataUrl contains invalid base64 padding".into());
            }
            let decoded_bytes = payload
                .len()
                .checked_div(4)
                .and_then(|groups| groups.checked_mul(3))
                .and_then(|bytes| bytes.checked_sub(padding))
                .ok_or_else(|| "avatarDataUrl size overflow".to_string())?;
            if decoded_bytes == 0 || decoded_bytes > MAX_SOURCE_BYTES {
                return Err("avatarDataUrl exceeds the 5 MiB Desktop avatar source boundary".into());
            }
            let bytes = BASE64_STANDARD
                .decode(payload.as_bytes())
                .map_err(|_| "avatarDataUrl contains invalid base64 image data".to_string())?;
            if bytes.len() != decoded_bytes {
                return Err("avatarDataUrl decoded length is inconsistent".into());
            }
            let actual_mime = sniff_avatar_mime_type(&bytes)
                .ok_or_else(|| "avatarDataUrl bytes are not a supported image".to_string())?;
            if actual_mime != declared_mime {
                return Err("avatarDataUrl MIME type does not match decoded image bytes".into());
            }
            Ok(Some(data_url.to_string()))
        }
        Some(_) => Err("avatarDataUrl must be a string or null".into()),
    }
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
    fn corrupt_roster_is_quarantined_and_reports_reset_recovery_without_backup() {
        let path = temp_path("corrupt-recovery");
        fs::write(&path, b"{not-json").unwrap();

        let (roster, recovery) = AndroidAgentRoster::open_recovering(&path).unwrap();
        assert_eq!(roster.count(), 0);
        assert!(!path.exists());

        let recovery = recovery.expect("corrupt roster must report recovery");
        assert_eq!(
            recovery.outcome,
            AndroidAgentRosterRecoveryOutcome::ResetCorruptStore
        );
        assert!(recovery.quarantine_path.exists());
        assert_eq!(fs::read(&recovery.quarantine_path).unwrap(), b"{not-json");

        let _ = fs::remove_file(recovery.quarantine_path);
    }

    #[test]
    fn corrupt_roster_recovers_latest_settled_backup() {
        let path = temp_path("backup-recovery");
        let agent_id = {
            let mut roster = AndroidAgentRoster::open(&path).unwrap();
            let created = roster.create("Recover Me", "durable").unwrap();
            roster.set_unread(&created.id, true).unwrap();
            created.id
        };
        let backup = recovery_backup_path(&path);
        assert!(backup.exists(), "settled roster writes must refresh the recovery replica");

        fs::write(&path, b"{corrupt-canonical").unwrap();
        let (roster, recovery) = AndroidAgentRoster::open_recovering(&path).unwrap();
        let recovered = roster.get(&agent_id).expect("latest settled agent must be salvaged");
        assert!(recovered.has_unread);

        let recovery = recovery.expect("corrupt canonical roster must report recovery");
        assert_eq!(
            recovery.outcome,
            AndroidAgentRosterRecoveryOutcome::RecoveredLastKnownGood
        );
        assert!(recovery.quarantine_path.exists());
        assert!(path.exists(), "recovery must reinstall a canonical roster");
        assert!(AndroidAgentRoster::open(&path).unwrap().get(&agent_id).is_some());

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(backup);
        let _ = fs::remove_file(recovery.quarantine_path);
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
            let created = roster.create("Agent", "description").unwrap();
            let persona = roster
                .apply_presentation_operation(
                    "session:account-a",
                    "persona-before-duplicate",
                    &json!({"kind":"update","id":created.id,"avatarShape":"squircle","avatarColor":"#336699"}),
                )
                .unwrap();
            assert_eq!(persona["status"], "completed");
            created.id
        };
        let mutation = json!({"kind":"duplicate","id":first_id});
        let first_result = {
            let mut roster = AndroidAgentRoster::open(&path).unwrap();
            roster
                .apply_presentation_operation("session:account-a", "operation-1", &mutation)
                .unwrap()
        };
        assert_eq!(first_result["status"], "completed");
        assert_eq!(first_result["result"]["agent"]["avatarShape"], "squircle");
        assert_eq!(first_result["result"]["agent"]["avatarColor"], "#336699");
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
        let durable_duplicate = reopened.get(&duplicate_id).unwrap();
        assert_eq!(durable_duplicate.avatar_shape.as_deref(), Some("squircle"));
        assert_eq!(durable_duplicate.avatar_color.as_deref(), Some("#336699"));
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
    fn settings_profile_title_and_notifications_are_durable_and_duplicate_preserves_them() {
        let path = temp_path("settings-profile");
        let mut roster = AndroidAgentRoster::open(&path).unwrap();
        let agent = roster.create("Agent", "description").unwrap();
        let updated = roster
            .apply_presentation_operation(
                "session:account-a",
                "settings-title",
                &json!({"kind":"update","id":agent.id,"title":"  Research lead  "}),
            )
            .unwrap();
        assert_eq!(updated["status"], "completed");
        assert_eq!(updated["result"]["title"], "Research lead");
        let notified = roster
            .apply_presentation_operation(
                "session:account-a",
                "settings-notifications",
                &json!({"kind":"notifications","id":agent.id,"value":true}),
            )
            .unwrap();
        assert_eq!(notified["status"], "completed");
        assert_eq!(notified["result"]["notifyOnUpdatesEnabled"], true);

        let duplicate = roster.duplicate(&agent.id).unwrap();
        assert_eq!(duplicate.title.as_deref(), Some("Research lead"));
        assert!(duplicate.notify_on_updates_enabled);
        drop(roster);

        let reopened = AndroidAgentRoster::open(&path).unwrap();
        let restored = reopened.get(&agent.id).unwrap();
        assert_eq!(restored.title.as_deref(), Some("Research lead"));
        assert!(restored.notify_on_updates_enabled);
        let _ = fs::remove_file(path);
    }


    #[test]
    fn settings_notifications_replay_durably_and_groups_fail_closed() {
        let path = temp_path("settings-notifications-replay");
        let mut roster = AndroidAgentRoster::open(&path).unwrap();
        let agent = roster.create("Agent", "description").unwrap();
        let member = roster.create("Member", "description").unwrap();
        let group = roster
            .create_group("Group", "description", std::slice::from_ref(&member.id))
            .unwrap();

        let mutation = json!({"kind":"notifications","id":agent.id,"value":true});
        let first = roster
            .apply_presentation_operation(
                "session:account-a",
                "settings-notification-replay",
                &mutation,
            )
            .unwrap();
        assert_eq!(first["status"], "completed");
        assert_eq!(first["result"]["notifyOnUpdatesEnabled"], true);
        drop(roster);

        let mut reopened = AndroidAgentRoster::open(&path).unwrap();
        let replay = reopened
            .apply_presentation_operation(
                "session:account-a",
                "settings-notification-replay",
                &mutation,
            )
            .unwrap();
        assert_eq!(replay, first);
        assert!(reopened.get(&agent.id).unwrap().notify_on_updates_enabled);

        let rejected = reopened
            .apply_presentation_operation(
                "session:account-a",
                "settings-group-notification",
                &json!({"kind":"notifications","id":group.id,"value":true}),
            )
            .unwrap();
        assert_eq!(rejected["status"], "rejected");
        assert_eq!(
            rejected["error"],
            "Agent group notifications are not supported"
        );
        assert!(!reopened.get(&group.id).unwrap().notify_on_updates_enabled);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn legacy_persisted_agent_without_avatar_persona_uses_serde_defaults() {
        let path = temp_path("legacy-avatar-defaults");
        fs::write(
            &path,
            serde_json::to_vec_pretty(&json!({
                "schemaVersion": ROSTER_SCHEMA_VERSION,
                "nextId": 1,
                "pinnedAgentIds": [],
                "agents": [{
                    "id": "agent-00000001",
                    "name": "Legacy Agent",
                    "description": "persisted before avatar persona fields",
                    "isGroup": false,
                    "memberIds": [],
                    "isHiddenFromSidebar": false,
                    "hasUnread": false,
                    "isPinned": false,
                    "updatedAt": 1
                }]
            }))
            .unwrap(),
        )
        .unwrap();

        let mut roster = AndroidAgentRoster::open(&path).unwrap();
        let restored = roster.get("agent-00000001").unwrap();
        assert_eq!(restored.avatar_shape, None);
        assert_eq!(restored.avatar_color, None);
        assert_eq!(restored.title, None);
        assert!(!restored.notify_on_updates_enabled);

        roster.set_unread("agent-00000001", true).unwrap();
        drop(roster);
        let reopened = AndroidAgentRoster::open(&path).unwrap();
        let restored = reopened.get("agent-00000001").unwrap();
        assert!(restored.has_unread);
        assert_eq!(restored.avatar_shape, None);
        assert_eq!(restored.avatar_color, None);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn avatar_persona_update_is_durable_replayable_resettable_and_validated() {
        let path = temp_path("avatar-persona");
        let agent_id = {
            let mut roster = AndroidAgentRoster::open(&path).unwrap();
            roster.create("Agent", "description").unwrap().id
        };
        let mutation = json!({"kind":"update","id":agent_id,"avatarShape":"circle","avatarColor":"#1a2b3c"});
        let first = {
            let mut roster = AndroidAgentRoster::open(&path).unwrap();
            roster.apply_presentation_operation("session:account-a", "avatar-op-1", &mutation).unwrap()
        };
        assert_eq!(first["status"], "completed");
        assert_eq!(first["result"]["avatarShape"], "circle");
        assert_eq!(first["result"]["avatarColor"], "#1A2B3C");
        let mut reopened = AndroidAgentRoster::open(&path).unwrap();
        let replay = reopened.apply_presentation_operation("session:account-a", "avatar-op-1", &mutation).unwrap();
        assert_eq!(replay, first);
        let duplicate = reopened.duplicate(&agent_id).unwrap();
        assert_eq!(duplicate.avatar_shape.as_deref(), Some("circle"));
        assert_eq!(duplicate.avatar_color.as_deref(), Some("#1A2B3C"));
        let rejected = reopened.apply_presentation_operation(
            "session:account-a",
            "avatar-op-bad",
            &json!({"kind":"update","id":agent_id,"avatarShape":"triangle","avatarColor":"red"}),
        ).unwrap();
        assert_eq!(rejected["status"], "rejected");
        assert_eq!(reopened.get(&agent_id).unwrap().avatar_shape.as_deref(), Some("circle"));
        let reset = reopened.apply_presentation_operation(
            "session:account-a",
            "avatar-op-reset",
            &json!({"kind":"update","id":agent_id,"avatarShape":null,"avatarColor":null}),
        ).unwrap();
        assert_eq!(reset["status"], "completed");
        assert!(reset["result"].get("avatarShape").is_none());
        assert!(reset["result"].get("avatarColor").is_none());
        drop(reopened);
        let reopened = AndroidAgentRoster::open(&path).unwrap();
        assert_eq!(reopened.get(&agent_id).unwrap().avatar_shape, None);
        assert_eq!(reopened.get(&agent_id).unwrap().avatar_color, None);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn avatar_data_url_matches_desktop_mime_and_five_mib_boundary() {
        let svg = "data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=";
        assert_eq!(
            parse_avatar_data_url(Some(&Value::String(svg.into()))).unwrap().as_deref(),
            Some(svg)
        );
        assert!(parse_avatar_data_url(Some(&Value::String(
            "data:image/bmp;base64,AAAA".into()
        ))).is_err());
        assert!(parse_avatar_data_url(Some(&Value::String(
            "data:image/avif;base64,AAAA".into()
        ))).is_err());

        let mut max_png = vec![0u8; 5 * 1024 * 1024];
        max_png[..8].copy_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
        let max_value = format!(
            "data:image/png;base64,{}",
            BASE64_STANDARD.encode(&max_png),
        );
        assert!(parse_avatar_data_url(Some(&Value::String(max_value))).is_ok());
        assert!(parse_avatar_data_url(Some(&Value::String(
            "data:image/png;base64,A=AA".into()
        ))).is_err());

        let mut too_large_png = vec![0u8; (5 * 1024 * 1024) + 1];
        too_large_png[..8].copy_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
        let too_large = format!(
            "data:image/png;base64,{}",
            BASE64_STANDARD.encode(&too_large_png),
        );
        assert!(parse_avatar_data_url(Some(&Value::String(too_large))).is_err());
    }

    #[test]
    fn avatar_image_data_url_is_durable_replayable_clearable_and_validated() {
        let path = temp_path("avatar-image-data-url");
        let agent_id = {
            let mut roster = AndroidAgentRoster::open(&path).unwrap();
            roster.create("Agent", "").unwrap().id
        };
        let data_url = "data:image/png;base64,iVBORw0KGgo=";
        let first = {
            let mut roster = AndroidAgentRoster::open(&path).unwrap();
            roster
                .apply_presentation_operation(
                    "session:account-a",
                    "avatar-image-1",
                    &json!({"kind":"update","id":agent_id,"avatarDataUrl":data_url}),
                )
                .unwrap()
        };
        assert_eq!(first["status"], "completed");
        assert_eq!(first["result"]["avatarDataUrl"], data_url);

        let mut reopened = AndroidAgentRoster::open(&path).unwrap();
        assert_eq!(reopened.get(&agent_id).unwrap().avatar_data_url.as_deref(), Some(data_url));
        let replay = reopened
            .apply_presentation_operation(
                "session:account-a",
                "avatar-image-1",
                &json!({"kind":"update","id":agent_id,"avatarDataUrl":data_url}),
            )
            .unwrap();
        assert_eq!(replay, first);
        assert_eq!(reopened.duplicate(&agent_id).unwrap().avatar_data_url.as_deref(), Some(data_url));

        let rejected = reopened
            .apply_presentation_operation(
                "session:account-a",
                "avatar-image-invalid",
                &json!({"kind":"update","id":agent_id,"avatarDataUrl":"data:text/plain;base64,SGk="}),
            )
            .unwrap();
        assert_eq!(rejected["status"], "rejected");
        assert_eq!(reopened.get(&agent_id).unwrap().avatar_data_url.as_deref(), Some(data_url));

        let mime_spoof = reopened
            .apply_presentation_operation(
                "session:account-a",
                "avatar-image-mime-spoof",
                &json!({"kind":"update","id":agent_id,"avatarDataUrl":"data:image/jpeg;base64,iVBORw0KGgo="}),
            )
            .unwrap();
        assert_eq!(mime_spoof["status"], "rejected");
        assert_eq!(
            reopened.get(&agent_id).unwrap().avatar_data_url.as_deref(),
            Some(data_url)
        );

        let cleared = reopened
            .apply_presentation_operation(
                "session:account-a",
                "avatar-image-clear",
                &json!({"kind":"update","id":agent_id,"avatarDataUrl":Value::Null}),
            )
            .unwrap();
        assert_eq!(cleared["status"], "completed");
        assert!(cleared["result"].get("avatarDataUrl").is_none());
        drop(reopened);
        assert_eq!(
            AndroidAgentRoster::open(&path).unwrap().get(&agent_id).unwrap().avatar_data_url,
            None
        );
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
    #[test]
    fn desktop_onboarding_persona_is_validated_persisted_and_reopened() {
        let path = temp_path("desktop-onboarding-persona");
        let id = {
            let mut roster = AndroidAgentRoster::open(&path).unwrap();
            let created = roster
                .create_with_persona(
                    "First Agent",
                    "daily work",
                    Some(&json!("teardrop")),
                    Some(&json!("blue")),
                )
                .unwrap();
            assert_eq!(created.avatar_shape.as_deref(), Some("teardrop"));
            assert_eq!(created.avatar_color.as_deref(), Some("blue"));
            assert!(roster
                .create_with_persona("Bad", "", Some(&json!("triangle")), Some(&json!("blue")))
                .is_err());
            assert!(roster
                .create_with_persona("Bad", "", Some(&json!("blob")), Some(&json!("purple-ish")))
                .is_err());
            created.id
        };
        let reopened = AndroidAgentRoster::open(&path).unwrap();
        let persisted = reopened.get(&id).unwrap();
        assert_eq!(persisted.avatar_shape.as_deref(), Some("teardrop"));
        assert_eq!(persisted.avatar_color.as_deref(), Some("blue"));
        let _ = fs::remove_file(path);
    }

}
