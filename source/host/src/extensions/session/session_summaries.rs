use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use serde_json::Value;

use crate::agents::agent_avatar::{
    read_avatar_within_dir, resolve_derived_avatar_filename,
};
use crate::agents::agent_profile::{
    SandAgentProfile, get_sand_profile_path, read_sand_profile_file,
};
use crate::agents::settings_file::{
    get_sand_settings_path, read_sand_settings_file,
};
use crate::groups::group_store::read_sand_group_config;
use crate::groups::remote_room_store::{
    SandRemoteRoomConfig, read_sand_remote_room_config,
};

use super::agent_db::{
    AgentMetadataProjection, mark_persisted_activity, read_persisted_agent_metadata_projection,
    read_persisted_agent_origin, read_persisted_agent_purpose,
    read_persisted_agent_serde_snapshot, read_persisted_conversation_partner_ids,
    read_persisted_transcript_entries,
};
use super::agent_db_serde::UnreadState;
use super::session_projection::{
    LastMessage, get_last_entry_from_transcript, get_last_message_from_transcript,
    get_summary_updated_at, seed_activity_from_mtime_value,
};
use super::session_recovery::{ensure_profile_file, ensure_settings_file};

#[derive(Debug, Clone, PartialEq)]
pub struct DbExtras {
    pub agent_id: String,
    pub created_at: f64,
    pub updated_at: f64,
    pub has_transcript: bool,
    pub last_entry: Option<Value>,
    pub last_message: Option<LastMessage>,
    pub newest_entry_id: Option<String>,
    pub unread_state: UnreadState,
    pub awaiting_user_response: Option<Value>,
    pub origin: String,
    pub purpose: Option<String>,
    pub conversation_partner_ids: Vec<String>,
    pub legacy_avatar_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub title: String,
    pub avatar_data_url: Option<String>,
    pub avatar_version: Option<String>,
    pub avatar_shape: Option<String>,
    pub avatar_color: Option<String>,
    pub created_at: f64,
    pub updated_at: f64,
    pub path: String,
    pub is_active: bool,
    pub is_running: bool,
    pub is_composing_message: bool,
    pub last_entry: Option<Value>,
    pub last_message_id: Option<String>,
    pub last_message_preview: Option<String>,
    pub newest_entry_id: Option<String>,
    pub has_unread: bool,
    pub unread_count: f64,
    pub last_viewed_at: f64,
    pub last_activity_at: f64,
    pub awaiting_user_response: Option<Value>,
    pub notifications_enabled: bool,
    pub notify_on_updates_enabled: bool,
    pub is_hidden_from_sidebar: bool,
    pub origin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    pub is_group: bool,
    pub member_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_room: Option<SandRemoteRoomConfig>,
    pub conversation_partner_ids: Vec<String>,
}

pub fn file_mtime_ms(path: &Path) -> Option<f64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_secs_f64() * 1000.0)
}

pub fn load_agent_db_extras(
    db_path: &Path,
    busy_timeout_ms: u64,
    dir_name: &str,
    mtime_ms: Option<f64>,
) -> Result<DbExtras, String> {
    let metadata = read_persisted_agent_metadata_projection(db_path, busy_timeout_ms)
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| AgentMetadataProjection {
            agent_id: dir_name.to_string(),
            created_at: 0.0,
            latest_root_blob_id: Vec::new(),
        });
    let mut state = read_persisted_agent_serde_snapshot(db_path, busy_timeout_ms)
        .map_err(|error| error.to_string())?;

    let _ = ensure_profile_file(
        db_path,
        Some(&metadata.agent_id),
        &state.profile.description,
    )
    .map_err(|error| error.to_string())?;

    if let Some(seed) = seed_activity_from_mtime_value(
        state.unread_state.last_activity_at,
        !metadata.latest_root_blob_id.is_empty(),
        metadata.created_at,
        mtime_ms,
    ) {
        let _ = mark_persisted_activity(db_path, busy_timeout_ms, seed)
            .map_err(|error| error.to_string())?;
        state = read_persisted_agent_serde_snapshot(db_path, busy_timeout_ms)
            .map_err(|error| error.to_string())?;
    }

    let entries = read_persisted_transcript_entries(db_path, busy_timeout_ms)
        .map_err(|error| error.to_string())?;
    let awaiting = state.awaiting_user_response.as_ref().map(|value| {
        serde_json::json!({
            "tabId": value.tab_id.clone(),
            "reason": value.reason.clone(),
            "since": value.since,
        })
    });
    Ok(DbExtras {
        agent_id: if metadata.agent_id.is_empty() {
            dir_name.to_string()
        } else {
            metadata.agent_id
        },
        created_at: metadata.created_at,
        updated_at: get_summary_updated_at(
            metadata.created_at,
            state.unread_state.last_activity_at,
        ),
        has_transcript: !entries.is_empty(),
        last_entry: get_last_entry_from_transcript(&entries),
        last_message: get_last_message_from_transcript(&entries),
        newest_entry_id: entries
            .last()
            .and_then(|entry| entry.get("id"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        unread_state: state.unread_state,
        awaiting_user_response: awaiting,
        origin: read_persisted_agent_origin(db_path, busy_timeout_ms)
            .map_err(|error| error.to_string())?,
        purpose: read_persisted_agent_purpose(db_path, busy_timeout_ms)
            .map_err(|error| error.to_string())?,
        conversation_partner_ids: read_persisted_conversation_partner_ids(
            db_path,
            busy_timeout_ms,
        )
        .map_err(|error| error.to_string())?,
        legacy_avatar_path: state.profile.avatar_path,
    })
}

pub fn minimal_agent_summary(
    dir_name: &str,
    db_path: &Path,
    mtime_ms: Option<f64>,
    active_agent_id: Option<&str>,
) -> AgentSummary {
    let agent_dir = db_path.parent().unwrap_or_else(|| Path::new("."));
    let profile = read_sand_profile_file(get_sand_profile_path(agent_dir));
    let time = mtime_ms.unwrap_or_default().floor();
    let name = profile
        .as_ref()
        .map(|profile| profile.name.trim())
        .filter(|name| !name.is_empty())
        .unwrap_or("Grok")
        .to_string();
    let description = profile
        .as_ref()
        .map(|profile| profile.description.clone())
        .unwrap_or_default();
    let title = profile
        .as_ref()
        .map(|profile| profile.title.clone())
        .unwrap_or_default();
    AgentSummary {
        id: dir_name.to_string(),
        name,
        description,
        title,
        avatar_data_url: None,
        avatar_version: None,
        avatar_shape: non_empty(profile.as_ref().map(|profile| profile.avatar_shape.as_str())),
        avatar_color: non_empty(profile.as_ref().map(|profile| profile.avatar_color.as_str())),
        created_at: time,
        updated_at: time,
        path: db_path.to_string_lossy().into_owned(),
        is_active: active_agent_id == Some(dir_name),
        is_running: false,
        is_composing_message: false,
        last_entry: None,
        last_message_id: None,
        last_message_preview: None,
        newest_entry_id: None,
        has_unread: false,
        unread_count: 0.0,
        last_viewed_at: 0.0,
        last_activity_at: 0.0,
        awaiting_user_response: None,
        notifications_enabled: false,
        notify_on_updates_enabled: true,
        is_hidden_from_sidebar: false,
        origin: "user".to_string(),
        purpose: None,
        is_group: false,
        member_ids: Vec::new(),
        remote_room: None,
        conversation_partner_ids: Vec::new(),
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DurableFootprint {
    pub has_memory: bool,
    pub has_automations: bool,
    pub has_workflows: bool,
}

pub fn agent_has_quarantined_store_db(agent_dir: &Path) -> bool {
    fs::read_dir(agent_dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| entry.file_name().to_string_lossy().starts_with("store.db.corrupt-"))
}

pub fn agent_has_durable_footprint(agent_dir: &Path, footprint: DurableFootprint) -> bool {
    agent_has_quarantined_store_db(agent_dir)
        || footprint.has_memory
        || footprint.has_automations
        || footprint.has_workflows
}

pub fn build_summary(
    extras: Option<&DbExtras>,
    db_path: &Path,
    dir_name: &str,
    mtime_ms: Option<f64>,
    active_agent_id: Option<&str>,
    include_blank: bool,
    footprint: DurableFootprint,
) -> Result<Option<AgentSummary>, String> {
    let agent_dir = db_path.parent().unwrap_or_else(|| Path::new("."));
    let _ = ensure_settings_file(db_path).map_err(|error| error.to_string())?;
    let base = minimal_agent_summary(dir_name, db_path, mtime_ms, active_agent_id);
    let profile = read_sand_profile_file(get_sand_profile_path(agent_dir));
    let settings = read_sand_settings_file(get_sand_settings_path(agent_dir));
    let identity = profile.clone().unwrap_or_else(|| SandAgentProfile {
        name: "Grok".to_string(),
        description: String::new(),
        title: String::new(),
        avatar_shape: String::new(),
        avatar_color: String::new(),
    });
    let name = if identity.name.trim().is_empty() {
        "Grok".to_string()
    } else {
        identity.name.trim().to_string()
    };
    let group_config = read_sand_group_config(agent_dir);
    let remote_room = read_sand_remote_room_config(agent_dir);
    let is_group = group_config.is_some();
    let has_identity = is_group
        || name != "Grok"
        || !identity.description.trim().is_empty()
        || !identity.title.is_empty();

    if extras.is_some_and(|extras| !extras.has_transcript)
        && !base.is_active
        && !has_identity
        && !include_blank
        && !agent_has_durable_footprint(agent_dir, footprint)
    {
        return Ok(None);
    }

    let is_unread = extras.is_some_and(|extras| {
        extras.unread_state.is_manually_unread
            || extras.unread_state.last_activity_at > extras.unread_state.last_viewed_at
    });

    let mut summary = base;
    summary.id = extras
        .map(|extras| extras.agent_id.clone())
        .unwrap_or_else(|| dir_name.to_string());
    summary.name = name;
    summary.description = identity.description;
    summary.title = identity.title;
    summary.avatar_shape = non_empty(Some(&identity.avatar_shape));
    summary.avatar_color = non_empty(Some(&identity.avatar_color));
    let avatar = resolve_derived_avatar_filename(
        agent_dir,
        extras.and_then(|extras| extras.legacy_avatar_path.as_deref()),
    )
    .as_deref()
    .and_then(|candidate| read_avatar_within_dir(agent_dir, candidate));
    summary.avatar_data_url = avatar.as_ref().map(|avatar| avatar.data_url.clone());
    summary.avatar_version = avatar.map(|avatar| avatar.version);
    summary.created_at = extras.map(|extras| extras.created_at).unwrap_or(summary.created_at);
    summary.updated_at = extras.map(|extras| extras.updated_at).unwrap_or(summary.updated_at);
    summary.last_entry = extras.and_then(|extras| extras.last_entry.clone());
    summary.last_message_id = extras
        .and_then(|extras| extras.last_message.as_ref())
        .map(|message| message.id.clone());
    summary.last_message_preview = extras
        .and_then(|extras| extras.last_message.as_ref())
        .map(|message| message.preview.clone());
    summary.newest_entry_id = extras.and_then(|extras| extras.newest_entry_id.clone());
    summary.has_unread = is_unread;
    summary.unread_count = if is_unread {
        extras
            .map(|extras| extras.unread_state.unread_count.max(1.0))
            .unwrap_or(1.0)
    } else {
        0.0
    };
    summary.last_viewed_at = extras
        .map(|extras| extras.unread_state.last_viewed_at)
        .unwrap_or_default();
    summary.last_activity_at = extras
        .map(|extras| extras.unread_state.last_activity_at)
        .unwrap_or_default();
    summary.awaiting_user_response =
        extras.and_then(|extras| extras.awaiting_user_response.clone());
    summary.notify_on_updates_enabled = settings.notify_on_agent_updates;
    summary.is_hidden_from_sidebar = settings.hidden_from_sidebar;
    summary.origin = extras
        .map(|extras| extras.origin.clone())
        .unwrap_or_else(|| "user".to_string());
    summary.purpose = extras.and_then(|extras| extras.purpose.clone());
    summary.is_group = is_group;
    summary.member_ids = group_config
        .map(|config| config.member_ids)
        .unwrap_or_default();
    summary.remote_room = remote_room;
    summary.conversation_partner_ids = extras
        .map(|extras| extras.conversation_partner_ids.clone())
        .unwrap_or_default();
    Ok(Some(summary))
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn agent_dir_from_db_path(db_path: &Path) -> PathBuf {
    db_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
}
